use super::embedder::{vector_literal, EmbeddingError};
use crate::config::KnowledgeRetrievalConfig;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RetrievedKnowledge {
    pub item_id: Uuid,
    pub revision_id: Uuid,
    pub scope: String,
    pub knowledge_type: String,
    pub title: String,
    pub content: String,
    pub source_type: String,
    pub source_id: Option<Uuid>,
    pub confidence: String,
    pub similarity: f64,
    pub revision_created_at: OffsetDateTime,
}

#[derive(Debug, thiserror::Error)]
pub enum RetrievalError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Embedding(#[from] EmbeddingError),
}

pub const RETRIEVAL_QUERY_SQL: &str = r#"
WITH eligible AS MATERIALIZED (
    SELECT
        i.id AS item_id,
        r.id AS revision_id,
        r.scope,
        r.knowledge_type,
        r.title,
        r.content,
        r.source_type,
        r.source_id,
        r.confidence,
        r.created_at AS revision_created_at,
        e.embedding
    FROM knowledge_items i
    JOIN knowledge_revisions r ON r.id = i.active_revision_id
    JOIN knowledge_embeddings e ON e.revision_id = r.id
    WHERE i.status = 'approved'
      AND i.superseded_by IS NULL
      AND (i.expires_at IS NULL OR i.expires_at > now())
      AND CASE $3
            WHEN 'high' THEN r.confidence = 'high'
            WHEN 'medium' THEN r.confidence IN ('medium', 'high')
            ELSE r.confidence IN ('low', 'medium', 'high')
          END
      AND (
            r.scope = 'workspace'
            OR (r.scope = 'project' AND r.project_id = $1)
            OR (r.scope = 'agent' AND r.project_id = $1 AND r.agent_id = $2)
          )
), ranked AS (
    SELECT eligible.*, (eligible.embedding <=> $4::vector) AS distance
    FROM eligible
)
SELECT
    item_id, revision_id, scope, knowledge_type, title, content,
    source_type, source_id, confidence, revision_created_at,
    1.0 - distance AS similarity
FROM ranked
WHERE 1.0 - distance >= $5
ORDER BY distance ASC, revision_created_at DESC, item_id ASC
LIMIT $6
"#;

pub async fn has_eligible(
    pool: &PgPool,
    project_id: Uuid,
    agent_id: Uuid,
    minimum_confidence: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM knowledge_items i
            JOIN knowledge_revisions r ON r.id = i.active_revision_id
            JOIN knowledge_embeddings e ON e.revision_id = r.id
            WHERE i.status = 'approved'
              AND i.superseded_by IS NULL
              AND (i.expires_at IS NULL OR i.expires_at > now())
              AND CASE $3
                    WHEN 'high' THEN r.confidence = 'high'
                    WHEN 'medium' THEN r.confidence IN ('medium', 'high')
                    ELSE r.confidence IN ('low', 'medium', 'high')
                  END
              AND (
                    r.scope = 'workspace'
                    OR (r.scope = 'project' AND r.project_id = $1)
                    OR (r.scope = 'agent' AND r.project_id = $1 AND r.agent_id = $2)
                  )
        )
        "#,
    )
    .bind(project_id)
    .bind(agent_id)
    .bind(minimum_confidence)
    .fetch_one(pool)
    .await
}

pub async fn retrieve(
    pool: &PgPool,
    project_id: Uuid,
    agent_id: Uuid,
    query_vector: &[f32],
    config: &KnowledgeRetrievalConfig,
) -> Result<Vec<RetrievedKnowledge>, RetrievalError> {
    let vector = vector_literal(query_vector)?;
    let top_k = config.top_k.clamp(1, 20) as i64;
    let rows = sqlx::query(RETRIEVAL_QUERY_SQL)
        .bind(project_id)
        .bind(agent_id)
        .bind(&config.minimum_confidence)
        .bind(vector)
        .bind(config.minimum_similarity as f64)
        .bind(top_k)
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(RetrievedKnowledge {
                item_id: row.try_get("item_id")?,
                revision_id: row.try_get("revision_id")?,
                scope: row.try_get("scope")?,
                knowledge_type: row.try_get("knowledge_type")?,
                title: row.try_get("title")?,
                content: row.try_get("content")?,
                source_type: row.try_get("source_type")?,
                source_id: row.try_get("source_id")?,
                confidence: row.try_get("confidence")?,
                similarity: row.try_get("similarity")?,
                revision_created_at: row.try_get("revision_created_at")?,
            })
        })
        .collect()
}
