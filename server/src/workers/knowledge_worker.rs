use crate::domain::comment::{intent_from_str, CommentIntent};
use crate::domain::knowledge::{KnowledgeRevisionInput, KnowledgeScope, KnowledgeSourceType};
use crate::knowledge::embedder::{vector_literal, EmbeddingProvider};
use crate::knowledge::embedding_provider;
use crate::knowledge::extractor::{
    policy_decision, ExtractedCandidate, ExtractionComment, ExtractionInput, ExtractionProvider,
    MockExtractionProvider,
};
use crate::services::knowledge_job_service::{KnowledgeJob, KnowledgeJobService};
use crate::services::knowledge_service::{activate_embedded_revision, KnowledgeService};
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
        "embed_revision" => process_embedding(state, pool, &job, embedder).await,
        "extract_ticket" => process_extraction(state, pool, &job, extractor).await,
        other => Err(anyhow::anyhow!("unsupported knowledge job kind: {other}")),
    };
    match result {
        Ok(()) => {}
        Err(error) => {
            if let Err(mark_error) = jobs.mark_error(&job, &error.to_string()).await {
                return Err(error).context(format!(
                    "knowledge job failure could not update its claim: {mark_error}"
                ));
            }
            return Err(error);
        }
    }
    Ok(true)
}

async fn process_embedding(
    state: &AppState,
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
    let jobs = KnowledgeJobService::new(pool);
    jobs.lock_active_claim(&mut tx, job).await?;
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
    activate_embedded_revision(&mut tx, item_id, revision_id, &state.config.knowledge).await?;
    jobs.mark_completed_in_tx(&mut tx, job).await?;
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
    let max_source_bytes = state.config.knowledge.extraction.max_source_bytes;
    title = truncate_utf8(&title, max_source_bytes.min(1_000));
    let remaining_after_title = max_source_bytes.saturating_sub(title.len());
    description = truncate_utf8(
        &description,
        (max_source_bytes / 2).min(remaining_after_title),
    );
    let rows = sqlx::query(
        r#"
        SELECT id, body, intent FROM ticket_comments
        WHERE ticket_id = $1
        ORDER BY created_at DESC, id DESC
        LIMIT 20
        "#,
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;
    let mut remaining = max_source_bytes.saturating_sub(title.len() + description.len());
    let mut comments = Vec::new();
    // Allocate the bounded snapshot newest-first so older comments cannot crowd
    // the latest review evidence out of the extraction input.
    for row in rows {
        if remaining == 0 {
            break;
        }
        let body: String = row.try_get("body")?;
        let intent: String = row.try_get("intent")?;
        let bounded = truncate_utf8(&body, remaining.min(2_000));
        remaining = remaining.saturating_sub(bounded.len());
        let source_type = match intent_from_str(&intent) {
            Some(CommentIntent::ReviewFeedback) => KnowledgeSourceType::Review,
            Some(_) => KnowledgeSourceType::Comment,
            None => anyhow::bail!("ticket comment has invalid intent: {intent}"),
        };
        comments.push(ExtractionComment {
            id: row.try_get("id")?,
            body: bounded,
            source_type,
        });
    }
    // Providers receive retained comments in chronological order.
    comments.reverse();
    let input = ExtractionInput {
        ticket_id,
        project_id,
        title,
        description,
        comments,
    };
    let candidates = extractor.extract(&input).await?;
    let mut tx = pool.begin().await?;
    let jobs = KnowledgeJobService::new(pool);
    jobs.lock_active_claim(&mut tx, job).await?;
    let service = KnowledgeService::new(pool, &state.config.knowledge);
    for (index, candidate) in candidates
        .into_iter()
        .take(state.config.knowledge.extraction.max_candidates)
        .enumerate()
    {
        let (status, decision, reason) = policy_decision(&state.config.knowledge, &candidate);
        let source_id = validated_candidate_source_id(&input, &candidate)?;
        let revision = KnowledgeRevisionInput {
            scope: KnowledgeScope::Project,
            project_id: Some(project_id),
            agent_id: None,
            knowledge_type: candidate.knowledge_type,
            title: candidate.title,
            content: candidate.content,
            source_type: candidate.source_type,
            source_id,
            source_run_id: None,
            confidence: candidate.confidence,
        };
        service
            .create_extracted_in_tx(
                &mut tx,
                job.id,
                i32::try_from(index).context("candidate index exceeds i32")?,
                revision,
                status,
                decision,
                &reason,
            )
            .await?;
    }
    jobs.mark_completed_in_tx(&mut tx, job).await?;
    tx.commit().await?;
    Ok(())
}

fn validated_candidate_source_id(
    input: &ExtractionInput,
    candidate: &ExtractedCandidate,
) -> anyhow::Result<Option<Uuid>> {
    match candidate.source_type {
        KnowledgeSourceType::Ticket | KnowledgeSourceType::AgentSummary => {
            if let Some(source_id) = candidate.source_id {
                anyhow::ensure!(
                    source_id == input.ticket_id,
                    "ticket-derived candidate source does not match extraction ticket"
                );
            }
            Ok(Some(input.ticket_id))
        }
        KnowledgeSourceType::Comment | KnowledgeSourceType::Review => {
            let source_id = candidate
                .source_id
                .context("comment-derived candidate has no source id")?;
            anyhow::ensure!(
                input.comments.iter().any(|comment| {
                    comment.id == source_id && comment.source_type == candidate.source_type
                }),
                "comment-derived candidate source is not present in the extraction input"
            );
            Ok(Some(source_id))
        }
        KnowledgeSourceType::HumanNote
        | KnowledgeSourceType::WorkspaceSignal
        | KnowledgeSourceType::ObservationRun => {
            anyhow::bail!("unsupported extracted knowledge source type")
        }
    }
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
