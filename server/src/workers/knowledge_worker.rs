use crate::domain::knowledge::{KnowledgeRevisionInput, KnowledgeScope};
use crate::knowledge::embedder::{vector_literal, EmbeddingProvider};
use crate::knowledge::embedding_provider;
use crate::knowledge::extractor::{
    policy_decision, ExtractionInput, ExtractionProvider, MockExtractionProvider,
};
use crate::services::knowledge_job_service::{KnowledgeJob, KnowledgeJobService};
use crate::services::knowledge_service::KnowledgeService;
use crate::AppState;
use anyhow::Context;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub fn spawn_workers(state: Arc<AppState>) -> anyhow::Result<()> {
    if !state.config.knowledge.enabled || state.config.knowledge.worker_count == 0 {
        return Ok(());
    }
    let embedder = embedding_provider(&state.config.knowledge.embedding)?;
    let extractor: Arc<dyn ExtractionProvider> = Arc::new(MockExtractionProvider);
    for index in 0..state.config.knowledge.worker_count {
        let state = state.clone();
        let embedder = embedder.clone();
        let extractor = extractor.clone();
        tokio::spawn(async move {
            let worker_id = format!("knowledge-worker-{index}");
            loop {
                if let Err(error) = process_one(&state, &worker_id, &embedder, &extractor).await {
                    tracing::error!(%error, worker_id, "knowledge worker error");
                }
                tokio::time::sleep(Duration::from_millis(
                    state.config.knowledge.poll_interval_ms.max(50),
                ))
                .await;
            }
        });
    }
    Ok(())
}

pub async fn process_one(
    state: &AppState,
    worker_id: &str,
    embedder: &Arc<dyn EmbeddingProvider>,
    extractor: &Arc<dyn ExtractionProvider>,
) -> anyhow::Result<bool> {
    let pool = state.db.as_ref().context("no db")?;
    let jobs = KnowledgeJobService::new(pool);
    let Some(job) = jobs
        .claim_next(worker_id, state.config.knowledge.stale_lock_secs)
        .await?
    else {
        return Ok(false);
    };
    let result = match job.kind.as_str() {
        "embed_revision" => process_embedding(pool, &job, embedder).await,
        "extract_ticket" => process_extraction(state, pool, &job, extractor).await,
        other => Err(anyhow::anyhow!("unsupported knowledge job kind: {other}")),
    };
    match result {
        Ok(()) => jobs.mark_completed(job.id).await?,
        Err(error) => {
            jobs.mark_error(&job, &error.to_string()).await?;
            return Err(error);
        }
    }
    Ok(true)
}

async fn process_embedding(
    pool: &PgPool,
    job: &KnowledgeJob,
    embedder: &Arc<dyn EmbeddingProvider>,
) -> anyhow::Result<()> {
    let revision_id = job.revision_id.context("embedding job has no revision")?;
    let row = sqlx::query(
        r#"
        SELECT r.item_id, r.title, r.content
        FROM knowledge_revisions r
        WHERE r.id = $1
        "#,
    )
    .bind(revision_id)
    .fetch_optional(pool)
    .await?
    .context("knowledge revision not found")?;
    let item_id: Uuid = row.try_get("item_id")?;
    let title: String = row.try_get("title")?;
    let content: String = row.try_get("content")?;
    let vectors = embedder.embed(&[format!("{title}\n\n{content}")]).await?;
    let vector = vectors
        .first()
        .context("embedding provider returned no vector")?;
    if vector.len() != embedder.dimension() {
        anyhow::bail!(
            "embedding dimension {} does not match configured {}",
            vector.len(),
            embedder.dimension()
        );
    }
    let literal = vector_literal(vector)?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO knowledge_embeddings (
            revision_id, provider, model, embedding_dimension, embedding
        ) VALUES ($1, $2, $3, $4, $5::vector)
        ON CONFLICT (revision_id) DO UPDATE SET
            provider = EXCLUDED.provider,
            model = EXCLUDED.model,
            embedding_dimension = EXCLUDED.embedding_dimension,
            embedding = EXCLUDED.embedding,
            created_at = now()
        "#,
    )
    .bind(revision_id)
    .bind(embedder.provider_name())
    .bind(embedder.model_name())
    .bind(i32::try_from(embedder.dimension()).context("embedding dimension exceeds i32")?)
    .bind(literal)
    .execute(&mut *tx)
    .await?;
    let activated = sqlx::query(
        r#"
        UPDATE knowledge_items
        SET active_revision_id = $2, updated_at = now()
        WHERE id = $1 AND current_revision_id = $2 AND status = 'approved'
        RETURNING supersedes_item_id
        "#,
    )
    .bind(item_id)
    .bind(revision_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = activated {
        let supersedes: Option<Uuid> = row.try_get("supersedes_item_id")?;
        if let Some(old_item_id) = supersedes {
            sqlx::query(
                r#"
                UPDATE knowledge_items
                SET superseded_by = $2, version = version + 1, updated_at = now()
                WHERE id = $1 AND superseded_by IS NULL
                "#,
            )
            .bind(old_item_id)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn process_extraction(
    state: &AppState,
    pool: &PgPool,
    job: &KnowledgeJob,
    extractor: &Arc<dyn ExtractionProvider>,
) -> anyhow::Result<()> {
    let ticket_id = job.ticket_id.context("extraction job has no ticket")?;
    let ticket = sqlx::query("SELECT project_id, title, description FROM tickets WHERE id = $1")
        .bind(ticket_id)
        .fetch_optional(pool)
        .await?
        .context("ticket not found")?;
    let project_id: Uuid = ticket.try_get("project_id")?;
    let mut title: String = ticket.try_get("title")?;
    let mut description: String = ticket.try_get("description")?;
    title = truncate_utf8(&title, 1_000);
    description = truncate_utf8(
        &description,
        state.config.knowledge.extraction.max_source_bytes / 2,
    );
    let rows = sqlx::query(
        r#"
        SELECT body FROM ticket_comments
        WHERE ticket_id = $1
        ORDER BY created_at DESC, id DESC
        LIMIT 20
        "#,
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;
    let mut remaining = state
        .config
        .knowledge
        .extraction
        .max_source_bytes
        .saturating_sub(title.len() + description.len());
    let mut comments = Vec::new();
    for row in rows.into_iter().rev() {
        if remaining == 0 {
            break;
        }
        let body: String = row.try_get("body")?;
        let bounded = truncate_utf8(&body, remaining.min(2_000));
        remaining = remaining.saturating_sub(bounded.len());
        comments.push(bounded);
    }
    let input = ExtractionInput {
        ticket_id,
        project_id,
        title,
        description,
        comments,
    };
    let candidates = extractor.extract(&input).await?;
    let service = KnowledgeService::new(pool, &state.config.knowledge);
    for (index, candidate) in candidates
        .into_iter()
        .take(state.config.knowledge.extraction.max_candidates)
        .enumerate()
    {
        let (status, decision, reason) = policy_decision(&state.config.knowledge, &candidate);
        let revision = KnowledgeRevisionInput {
            scope: KnowledgeScope::Project,
            project_id: Some(project_id),
            agent_id: None,
            knowledge_type: candidate.knowledge_type,
            title: candidate.title,
            content: candidate.content,
            source_type: candidate.source_type,
            source_id: Some(ticket_id),
            source_run_id: None,
            confidence: candidate.confidence,
        };
        service
            .create_extracted(
                job.id,
                i32::try_from(index).context("candidate index exceeds i32")?,
                revision,
                status,
                decision,
                &reason,
            )
            .await?;
    }
    Ok(())
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_truncation_preserves_utf8() {
        assert_eq!(truncate_utf8("a🌱b", 4), "a");
        assert_eq!(truncate_utf8("a🌱b", 5), "a🌱");
    }
}
