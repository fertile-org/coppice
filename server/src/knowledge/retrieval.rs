use super::fts::{build_tsquery, terms};
use crate::config::KnowledgeRetrievalConfig;
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use time::OffsetDateTime;
use uuid::Uuid;

pub const INBOX_SIMILAR_DEFAULT_LIMIT: usize = 5;
pub const INBOX_SIMILAR_MAX_LIMIT: usize = 10;
/// Upper bound on eligible rows loaded for one run; the context budget trims further.
pub const MAX_ELIGIBLE_CANDIDATES: i64 = 200;
/// Full-text matches rescored for inbox near-duplicates. The OR query matches
/// any shared word, so this is a cheap first filter, not the answer.
pub const INBOX_SIMILAR_CANDIDATES: i64 = 50;
/// Lowest [`near_duplicate_score`] shown as a near duplicate.
pub const NEAR_DUPLICATE_MIN_SCORE: f64 = 0.25;

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
    /// Set by [`retrieve_across_boards`]. Board-scoped retrieval leaves this empty.
    pub board_name: Option<String>,
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

/// Approved knowledge on every board, plus workspace notes and this agent's
/// agent-scoped notes. `$1` is the agent id (no board bind).
pub const WORKSPACE_RETRIEVAL_QUERY_SQL: &str = r#"
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
        r.search_vector,
        b.name AS board_name
    FROM knowledge_items i
    JOIN knowledge_revisions r ON r.id = i.active_revision_id
    LEFT JOIN boards b ON b.id = r.board_id
    WHERE i.status = 'approved'
      AND i.superseded_by IS NULL
      AND (i.expires_at IS NULL OR i.expires_at > now())
      AND CASE $2
            WHEN 'high' THEN r.confidence = 'high'
            WHEN 'medium' THEN r.confidence IN ('medium', 'high')
            ELSE r.confidence IN ('low', 'medium', 'high')
          END
      AND (cardinality($4::text[]) = 0 OR r.knowledge_type = ANY($4::text[]))
      AND (
            r.scope = 'workspace'
            OR r.scope = 'board'
            OR (r.scope = 'agent' AND r.agent_id = $1)
          )
), query AS (
    SELECT CASE
        WHEN $3::text IS NULL THEN NULL
        ELSE to_tsquery('simple'::regconfig, coppice_unaccent($3::text))
    END AS q
)
SELECT
    item_id, revision_id, scope, knowledge_type, title, content,
    source_type, source_id, confidence, revision_created_at, board_name,
    CASE
        WHEN query.q IS NOT NULL AND eligible.search_vector @@ query.q
            THEN ts_rank_cd(eligible.search_vector, query.q)::float8
        ELSE 0.0::float8
    END AS score
FROM eligible CROSS JOIN query
ORDER BY score DESC, revision_created_at DESC, item_id ASC
LIMIT $5
"#;

/// Text comes back folded by `coppice_unaccent`, the pending item's too, so
/// both sides of [`near_duplicate_score`] fold accents the way `search_vector`
/// does.
const INBOX_SIMILAR_QUERY_SQL: &str = concat!(
    r#"
WITH eligible AS MATERIALIZED (
    SELECT
        i.id AS item_id,
        r.id AS revision_id,
        r.scope,
        r.knowledge_type,
        r.title,
        r.content,
        r.board_id,
        i.status,
        r.search_vector
"#,
    eligible_knowledge_from!(),
    r#"
      AND i.id <> $1
), query AS (
    SELECT
        to_tsquery('simple'::regconfig, coppice_unaccent($2::text)) AS q,
        coppice_unaccent($3::text) AS pending_title,
        coppice_unaccent($4::text) AS pending_content
)
SELECT
    item_id, revision_id, scope, knowledge_type, title, board_id, status,
    coppice_unaccent(title) AS folded_title,
    coppice_unaccent(content) AS folded_content,
    query.pending_title,
    query.pending_content
FROM eligible CROSS JOIN query
WHERE eligible.search_vector @@ query.q
ORDER BY ts_rank_cd(eligible.search_vector, query.q) DESC, item_id ASC
LIMIT $5
"#
);

/// Load the eligible set for a run, full-text matches first. Non-matching
/// entries are included with score 0; callers wanting matches only filter on
/// `score > 0`.
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
                board_name: None,
            })
        })
        .collect()
}

/// Same eligibility as [`retrieve`], without a single-board filter.
pub async fn retrieve_across_boards(
    pool: &PgPool,
    agent_id: Uuid,
    query_title: &str,
    query_body: &str,
    config: &KnowledgeRetrievalConfig,
) -> Result<Vec<RetrievedKnowledge>, sqlx::Error> {
    let tsquery = build_tsquery(query_title, query_body);
    let rows = sqlx::query(WORKSPACE_RETRIEVAL_QUERY_SQL)
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
                board_name: row.try_get("board_name")?,
            })
        })
        .collect()
}

/// Approved, live notes that are near duplicates of a pending one, best first.
/// Soft-prefers the pending item's board and scope; does not apply agent
/// confidence/type filters. `score` is [`near_duplicate_score`].
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
    let rows = sqlx::query(INBOX_SIMILAR_QUERY_SQL)
        .bind(exclude_item_id)
        .bind(tsquery)
        .bind(query_title)
        .bind(query_body)
        .bind(INBOX_SIMILAR_CANDIDATES)
        .fetch_all(pool)
        .await?;
    let mut neighbors = Vec::new();
    for row in rows {
        let score = near_duplicate_score(
            row.try_get("pending_title")?,
            row.try_get("pending_content")?,
            row.try_get("folded_title")?,
            row.try_get("folded_content")?,
        );
        if score < NEAR_DUPLICATE_MIN_SCORE {
            continue;
        }
        neighbors.push(SimilarKnowledgeNeighbor {
            item_id: row.try_get("item_id")?,
            revision_id: row.try_get("revision_id")?,
            title: row.try_get("title")?,
            knowledge_type: row.try_get("knowledge_type")?,
            scope: row.try_get("scope")?,
            board_id: row.try_get("board_id")?,
            score,
            status: row.try_get("status")?,
        });
    }
    let preference = |n: &SimilarKnowledgeNeighbor| {
        (
            prefer_board_id.is_none() || n.board_id != prefer_board_id,
            n.scope != prefer_scope,
        )
    };
    neighbors.sort_by(|a, b| {
        preference(a)
            .cmp(&preference(b))
            .then(b.score.total_cmp(&a.score))
            .then(a.item_id.cmp(&b.item_id))
    });
    neighbors.truncate(limit.clamp(1, INBOX_SIMILAR_MAX_LIMIT));
    Ok(neighbors)
}

/// How alike two notes are, from 0 (no shared words) to 1 (the same words):
/// `0.4 × Jaccard(title words) + 0.6 × Jaccard(title and content words)`.
/// Jaccard divides shared words by all words, so a long note that happens to
/// contain common words scores low. Text should be folded by
/// `coppice_unaccent` first.
pub fn near_duplicate_score(
    pending_title: &str,
    pending_body: &str,
    other_title: &str,
    other_body: &str,
) -> f64 {
    let pending_title = terms(pending_title);
    let other_title = terms(other_title);
    let pending_all: HashSet<String> = pending_title
        .iter()
        .cloned()
        .chain(terms(pending_body))
        .collect();
    let other_all: HashSet<String> = other_title
        .iter()
        .cloned()
        .chain(terms(other_body))
        .collect();
    (0.4 * jaccard(&pending_title, &other_title) + 0.6 * jaccard(&pending_all, &other_all))
        .clamp(0.0, 1.0)
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f64 / union as f64
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
        assert!(WORKSPACE_RETRIEVAL_QUERY_SQL.contains("r.scope = 'workspace'"));
        assert!(WORKSPACE_RETRIEVAL_QUERY_SQL.contains("r.scope = 'board'"));
        assert!(WORKSPACE_RETRIEVAL_QUERY_SQL.contains("r.agent_id = $1"));
        assert!(WORKSPACE_RETRIEVAL_QUERY_SQL.contains("LEFT JOIN boards"));
        assert!(!WORKSPACE_RETRIEVAL_QUERY_SQL.contains("r.board_id = $1"));
        assert!(WORKSPACE_RETRIEVAL_QUERY_SQL.contains("i.status = 'approved'"));
    }

    // Notes like the ones that showed up as near duplicates of each other.
    const TIMEOUT_TITLE: &str = "Connector run_timeout_secs defaults";
    const TIMEOUT_BODY: &str = "Each connector has a run_timeout_secs setting in config.toml. \
        The default is 3600 seconds; a run that goes past it is cancelled and the ticket \
        moves back to Ready. Raise it for long test runs or long reviews.";
    const CHECKS_TITLE: &str = "Server checks: scoped cargo test before review";
    const CHECKS_BODY: &str = "Before review, run make test-smoke or a scoped cargo test \
        -p coppice-server --features embedded-test-db <filter>. Run make test only for final \
        acceptance, then make clean.";
    const DRAG_TITLE: &str = "Active-run check on drag into review";
    const DRAG_BODY: &str = "Dragging a ticket into In Review is refused while the ticket \
        has an active run. The server checks hasActiveRun and answers 409; cancel the run or \
        wait for it to finish, then make the move again.";

    #[test]
    fn identical_notes_score_one_and_disjoint_notes_zero() {
        assert_eq!(
            near_duplicate_score(DRAG_TITLE, DRAG_BODY, DRAG_TITLE, DRAG_BODY),
            1.0
        );
        assert_eq!(
            near_duplicate_score(
                "Paint storage",
                "Never mix paint with diesel fuel.",
                DRAG_TITLE,
                DRAG_BODY
            ),
            0.0
        );
    }

    #[test]
    fn notes_sharing_only_workflow_words_are_not_near_duplicates() {
        for (title, body) in [(TIMEOUT_TITLE, TIMEOUT_BODY), (CHECKS_TITLE, CHECKS_BODY)] {
            let score = near_duplicate_score(DRAG_TITLE, DRAG_BODY, title, body);
            assert!(
                score > 0.0 && score < NEAR_DUPLICATE_MIN_SCORE,
                "{title}: {score}"
            );
        }
    }

    #[test]
    fn a_light_paraphrase_is_a_near_duplicate() {
        let score = near_duplicate_score(
            "Refuse drag into review during an active run",
            "Moving a ticket to In Review is refused while it has an active run. The server \
             checks hasActiveRun and returns 409; cancel the run or wait until it finishes.",
            DRAG_TITLE,
            DRAG_BODY,
        );
        assert!(score >= NEAR_DUPLICATE_MIN_SCORE, "{score}");
    }

    #[test]
    fn scores_stay_between_zero_and_one() {
        for (a, b) in [
            ((DRAG_TITLE, DRAG_BODY), (TIMEOUT_TITLE, TIMEOUT_BODY)),
            ((CHECKS_TITLE, CHECKS_BODY), (DRAG_TITLE, DRAG_BODY)),
            (("", ""), (DRAG_TITLE, DRAG_BODY)),
            (("", ""), ("", "")),
        ] {
            let score = near_duplicate_score(a.0, a.1, b.0, b.1);
            assert!((0.0..=1.0).contains(&score), "{score}");
        }
    }
}
