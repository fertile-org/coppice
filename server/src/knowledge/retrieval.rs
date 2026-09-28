use super::embedder::{vector_literal, EmbeddingError};
use crate::config::KnowledgeRetrievalConfig;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

/// Inbox near-duplicate assist floor — separate from agent `minimum_similarity` (default 0.0).
pub const INBOX_SIMILAR_MINIMUM_SIMILARITY: f64 = 0.75;
pub const INBOX_SIMILAR_DEFAULT_LIMIT: usize = 5;
pub const INBOX_SIMILAR_MAX_LIMIT: usize = 10;

/// Shared join + base predicates for approved, live, embedding-ready knowledge.
/// Agent retrieval adds confidence/type/scope filters on top; inbox assist does not.
macro_rules! eligible_knowledge_from {
    () => {
        r#"
    FROM knowledge_items i
    JOIN knowledge_revisions r ON r.id = i.active_revision_id
    JOIN knowledge_embeddings e ON e.revision_id = r.id
    WHERE i.status = 'approved'
      AND i.superseded_by IS NULL
      AND (i.expires_at IS NULL OR i.expires_at > now())
"#
    };
}

pub const ELIGIBLE_KNOWLEDGE_FROM: &str = eligible_knowledge_from!();

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

#[derive(Debug, Clone)]
pub struct SimilarKnowledgeNeighbor {
    pub item_id: Uuid,
    pub revision_id: Uuid,
    pub title: String,
    pub knowledge_type: String,
    pub scope: String,
    pub board_id: Option<Uuid>,
    pub similarity: f64,
    pub status: String,
    pub embedding_status: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RetrievalError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Embedding(#[from] EmbeddingError),
}

pub const RETRIEVAL_QUERY_SQL: &str = concat!(
    r#"
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
"#,
    eligible_knowledge_from!(),
    r#"
      AND CASE $3
            WHEN 'high' THEN r.confidence = 'high'
            WHEN 'medium' THEN r.confidence IN ('medium', 'high')
            ELSE r.confidence IN ('low', 'medium', 'high')
          END
      AND (cardinality($7::text[]) = 0 OR r.knowledge_type = ANY($7::text[]))
      AND (
            r.scope = 'workspace'
            OR (r.scope = 'board' AND r.board_id = $1)
            OR (r.scope = 'agent' AND r.board_id = $1 AND r.agent_id = $2)
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
"#
);

const INBOX_SIMILAR_QUERY_SQL: &str = concat!(
    r#"
WITH eligible AS MATERIALIZED (
    SELECT
        i.id AS item_id,
        r.id AS revision_id,
        r.scope,
        r.knowledge_type,
        r.title,
        r.board_id,
        i.status,
        e.embedding
"#,
    eligible_knowledge_from!(),
    r#"
      AND i.id <> $1
), ranked AS (
    SELECT eligible.*, (eligible.embedding <=> $2::vector) AS distance
    FROM eligible
)
SELECT
    item_id,
    revision_id,
    scope,
    knowledge_type,
    title,
    board_id,
    status,
    1.0 - distance AS similarity
FROM ranked
WHERE 1.0 - distance >= $3
ORDER BY
    CASE
        WHEN $4::uuid IS NOT NULL AND board_id IS NOT DISTINCT FROM $4 THEN 0
        ELSE 1
    END ASC,
    CASE WHEN scope = $5 THEN 0 ELSE 1 END ASC,
    distance ASC,
    item_id ASC
LIMIT $6
"#
);

pub async fn has_eligible(
    pool: &PgPool,
    board_id: Uuid,
    agent_id: Uuid,
    config: &KnowledgeRetrievalConfig,
) -> Result<bool, sqlx::Error> {
    let sql = concat!(
        r#"
        SELECT EXISTS (
            SELECT 1
"#,
        eligible_knowledge_from!(),
        r#"
              AND CASE $3
                    WHEN 'high' THEN r.confidence = 'high'
                    WHEN 'medium' THEN r.confidence IN ('medium', 'high')
                    ELSE r.confidence IN ('low', 'medium', 'high')
                  END
              AND (cardinality($4::text[]) = 0 OR r.knowledge_type = ANY($4::text[]))
              AND (
                    r.scope = 'workspace'
                    OR (r.scope = 'board' AND r.board_id = $1)
                    OR (r.scope = 'agent' AND r.board_id = $1 AND r.agent_id = $2)
                  )
        )
        "#
    );
    sqlx::query_scalar(sql)
        .bind(board_id)
        .bind(agent_id)
        .bind(&config.minimum_confidence)
        .bind(&config.allowed_types)
        .fetch_one(pool)
        .await
}

pub async fn retrieve(
    pool: &PgPool,
    board_id: Uuid,
    agent_id: Uuid,
    query_vector: &[f32],
    config: &KnowledgeRetrievalConfig,
) -> Result<Vec<RetrievedKnowledge>, RetrievalError> {
    let vector = vector_literal(query_vector)?;
    let top_k = config.top_k.clamp(1, 20) as i64;
    let rows = sqlx::query(RETRIEVAL_QUERY_SQL)
        .bind(board_id)
        .bind(agent_id)
        .bind(&config.minimum_confidence)
        .bind(vector)
        .bind(config.minimum_similarity as f64)
        .bind(top_k)
        .bind(&config.allowed_types)
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

/// Rank approved + embedding-ready neighbors for inbox review assist.
/// Soft-prefers the pending item's board and scope; does not apply agent confidence/type filters.
pub async fn find_similar_inbox(
    pool: &PgPool,
    exclude_item_id: Uuid,
    query_vector: &[f32],
    prefer_board_id: Option<Uuid>,
    prefer_scope: &str,
    limit: usize,
) -> Result<Vec<SimilarKnowledgeNeighbor>, RetrievalError> {
    let vector = vector_literal(query_vector)?;
    let limit = limit.clamp(1, INBOX_SIMILAR_MAX_LIMIT) as i64;
    let rows = sqlx::query(INBOX_SIMILAR_QUERY_SQL)
        .bind(exclude_item_id)
        .bind(vector)
        .bind(INBOX_SIMILAR_MINIMUM_SIMILARITY)
        .bind(prefer_board_id)
        .bind(prefer_scope)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SimilarKnowledgeNeighbor {
                item_id: row.try_get("item_id")?,
                revision_id: row.try_get("revision_id")?,
                title: row.try_get("title")?,
                knowledge_type: row.try_get("knowledge_type")?,
                scope: row.try_get("scope")?,
                board_id: row.try_get("board_id")?,
                similarity: row.try_get("similarity")?,
                status: row.try_get("status")?,
                embedding_status: "ready".into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_limits_are_independent_of_agent_defaults() {
        assert_eq!(INBOX_SIMILAR_MINIMUM_SIMILARITY, 0.75);
        assert_eq!(INBOX_SIMILAR_DEFAULT_LIMIT, 5);
        assert_eq!(INBOX_SIMILAR_MAX_LIMIT, 10);
        assert!(INBOX_SIMILAR_QUERY_SQL.contains("i.id <> $1"));
        assert!(INBOX_SIMILAR_QUERY_SQL.contains("i.status = 'approved'"));
        assert!(RETRIEVAL_QUERY_SQL.contains("i.status = 'approved'"));
        assert_eq!(ELIGIBLE_KNOWLEDGE_FROM, eligible_knowledge_from!());
    }
}
