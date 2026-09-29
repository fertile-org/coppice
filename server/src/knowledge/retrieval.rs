use super::fts::build_tsquery;
use crate::config::KnowledgeRetrievalConfig;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

pub const INBOX_SIMILAR_DEFAULT_LIMIT: usize = 5;
pub const INBOX_SIMILAR_MAX_LIMIT: usize = 10;
/// Upper bound on eligible rows loaded for one run; the context budget trims further.
pub const MAX_ELIGIBLE_CANDIDATES: i64 = 200;

/// Shared join + base predicates for approved, live knowledge.
/// Agent retrieval adds confidence/type/scope filters on top; inbox assist does not.
macro_rules! eligible_knowledge_from {
    () => {
        r#"
    FROM knowledge_items i
    JOIN knowledge_revisions r ON r.id = i.active_revision_id
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
    /// Full-text rank; `0.0` when the entry did not match the query.
    pub score: f64,
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
    pub score: f64,
    pub status: String,
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
        r.search_vector
"#,
    eligible_knowledge_from!(),
    r#"
      AND CASE $3
            WHEN 'high' THEN r.confidence = 'high'
            WHEN 'medium' THEN r.confidence IN ('medium', 'high')
            ELSE r.confidence IN ('low', 'medium', 'high')
          END
      AND (cardinality($5::text[]) = 0 OR r.knowledge_type = ANY($5::text[]))
      AND (
            r.scope = 'workspace'
            OR (r.scope = 'board' AND r.board_id = $1)
            OR (r.scope = 'agent' AND r.board_id = $1 AND r.agent_id = $2)
          )
), query AS (
    SELECT CASE
        WHEN $4::text IS NULL THEN NULL
        ELSE to_tsquery('simple'::regconfig, coppice_unaccent($4::text))
    END AS q
)
SELECT
    item_id, revision_id, scope, knowledge_type, title, content,
    source_type, source_id, confidence, revision_created_at,
    CASE
        WHEN query.q IS NOT NULL AND eligible.search_vector @@ query.q
            THEN ts_rank_cd(eligible.search_vector, query.q)::float8
        ELSE 0.0::float8
    END AS score
FROM eligible CROSS JOIN query
ORDER BY score DESC, revision_created_at DESC, item_id ASC
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
        r.search_vector
"#,
    eligible_knowledge_from!(),
    r#"
      AND i.id <> $1
), query AS (
    SELECT to_tsquery('simple'::regconfig, coppice_unaccent($2::text)) AS q
)
SELECT
    item_id, revision_id, scope, knowledge_type, title, board_id, status,
    ts_rank_cd(eligible.search_vector, query.q)::float8 AS score
FROM eligible CROSS JOIN query
WHERE eligible.search_vector @@ query.q
ORDER BY
    CASE
        WHEN $3::uuid IS NOT NULL AND board_id IS NOT DISTINCT FROM $3 THEN 0
        ELSE 1
    END ASC,
    CASE WHEN scope = $4 THEN 0 ELSE 1 END ASC,
    score DESC,
    item_id ASC
LIMIT $5
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

/// Load the eligible set for a run, full-text matches first. Callers pick
/// between "include everything" and "top matches" once the budget is known
/// (see [`crate::services::context_budget::select_within_budget`]).
pub async fn retrieve(
    pool: &PgPool,
    board_id: Uuid,
    agent_id: Uuid,
    query_title: &str,
    query_body: &str,
    config: &KnowledgeRetrievalConfig,
) -> Result<Vec<RetrievedKnowledge>, sqlx::Error> {
    let tsquery = build_tsquery(query_title, query_body);
    let rows = sqlx::query(RETRIEVAL_QUERY_SQL)
        .bind(board_id)
        .bind(agent_id)
        .bind(&config.minimum_confidence)
        .bind(tsquery)
        .bind(&config.allowed_types)
        .bind(MAX_ELIGIBLE_CANDIDATES)
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
                score: row.try_get("score")?,
                revision_created_at: row.try_get("revision_created_at")?,
            })
        })
        .collect()
}

/// Rank approved, live neighbors for inbox review assist by full-text match.
/// Soft-prefers the pending item's board and scope; does not apply agent confidence/type filters.
pub async fn find_similar_inbox(
    pool: &PgPool,
    exclude_item_id: Uuid,
    query_title: &str,
    query_body: &str,
    prefer_board_id: Option<Uuid>,
    prefer_scope: &str,
    limit: usize,
) -> Result<Vec<SimilarKnowledgeNeighbor>, sqlx::Error> {
    let Some(tsquery) = build_tsquery(query_title, query_body) else {
        return Ok(Vec::new());
    };
    let limit = limit.clamp(1, INBOX_SIMILAR_MAX_LIMIT) as i64;
    let rows = sqlx::query(INBOX_SIMILAR_QUERY_SQL)
        .bind(exclude_item_id)
        .bind(tsquery)
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
                score: row.try_get("score")?,
                status: row.try_get("status")?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_limits_and_shared_predicates() {
        assert_eq!(INBOX_SIMILAR_DEFAULT_LIMIT, 5);
        assert_eq!(INBOX_SIMILAR_MAX_LIMIT, 10);
        assert!(INBOX_SIMILAR_QUERY_SQL.contains("i.id <> $1"));
        assert!(INBOX_SIMILAR_QUERY_SQL.contains("i.status = 'approved'"));
        assert!(RETRIEVAL_QUERY_SQL.contains("i.status = 'approved'"));
        assert!(RETRIEVAL_QUERY_SQL.contains("ts_rank_cd"));
        assert!(!RETRIEVAL_QUERY_SQL.contains("<=>"));
        assert_eq!(ELIGIBLE_KNOWLEDGE_FROM, eligible_knowledge_from!());
    }
}
