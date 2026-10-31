use super::fts::{build_tsquery, query_terms, terms};
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
/// Lowest [`near_duplicate_score`] at which compaction is told to consider a merge.
pub const MERGE_MIN_SCORE: f64 = 0.5;
/// Most recently updated approved items compared pairwise per call.
pub const MERGE_SCAN_ITEMS: i64 = 500;

/// Base predicates for approved, live knowledge.
macro_rules! live_knowledge_predicates {
    () => {
        r#"i.status = 'approved'
      AND i.superseded_by IS NULL
      AND (i.expires_at IS NULL OR i.expires_at > now())
"#
    };
}

/// What one agent may read: confidence, type and scope filters. Parameters:
/// `$1` board ids (NULL means every board), `$2` agent id, `$3` minimum
/// confidence, `$5` allowed types. Shared by [`retrieve`] and [`fetch_visible`]
/// so search and get can never disagree about visibility.
macro_rules! agent_visible_predicates {
    () => {
        r#"
      AND CASE $3
            WHEN 'high' THEN r.confidence = 'high'
            WHEN 'medium' THEN r.confidence IN ('medium', 'high')
            ELSE r.confidence IN ('low', 'medium', 'high')
          END
      AND (cardinality($5::text[]) = 0 OR r.knowledge_type = ANY($5::text[]))
      AND (
            r.scope = 'workspace'
            OR (r.scope = 'board' AND ($1::uuid[] IS NULL OR r.board_id = ANY($1::uuid[])))
            OR (r.scope = 'agent' AND r.agent_id = $2
                AND ($1::uuid[] IS NULL OR r.board_id = ANY($1::uuid[])))
          )
"#
    };
}

/// Shared join + base predicates for approved, live knowledge.
/// Agent retrieval adds confidence/type/scope filters on top; inbox assist does not.
macro_rules! eligible_knowledge_from {
    () => {
        concat!(
            r#"
    FROM knowledge_items i
    JOIN knowledge_revisions r ON r.id = i.active_revision_id
    WHERE "#,
            live_knowledge_predicates!()
        )
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
    /// Summed IDF of the query terms matched; `0.0` when none matched.
    pub score: f64,
    pub revision_created_at: OffsetDateTime,
    /// Set for [`RetrievalScope::AllBoards`] only.
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

/// One query for every retrieval scope. `$1` is the board ids to cover (NULL
/// means every board), `$2` the agent id, `$3` minimum confidence, `$4` the
/// search terms, `$5` allowed types, `$6` the row limit.
///
/// `score` sums the IDF of each term a note matches, computed over the eligible
/// set so words common across these notes count little; a title match counts
/// double. `matches` is the one scan of notes against terms; document frequency
/// and scores both come from it.
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
        r.search_vector,
        b.name AS board_name
    FROM knowledge_items i
    JOIN knowledge_revisions r ON r.id = i.active_revision_id
    LEFT JOIN boards b ON b.id = r.board_id
    WHERE "#,
    live_knowledge_predicates!(),
    agent_visible_predicates!(),
    r#"
), terms AS MATERIALIZED (
    SELECT
        t,
        to_tsquery('simple'::regconfig, coppice_unaccent(t))
            || to_tsquery('english'::regconfig, coppice_unaccent(t)) AS q
    FROM unnest($4::text[]) AS t
), matches AS MATERIALIZED (
    SELECT e.item_id, terms.t, ts_filter(e.search_vector, '{a}') @@ terms.q AS in_title
    FROM eligible e
    CROSS JOIN terms
    WHERE e.search_vector @@ terms.q
), document_frequency AS (
    SELECT t, count(*)::float8 AS c FROM matches GROUP BY t
), scored AS (
    SELECT
        m.item_id,
        sum(
            ln(1 + ((SELECT count(*)::float8 FROM eligible) - df.c + 0.5) / (df.c + 0.5))
            * CASE WHEN m.in_title THEN 2 ELSE 1 END
        ) AS score
    FROM matches m
    JOIN document_frequency df ON df.t = m.t
    GROUP BY m.item_id
)
SELECT
    item_id, revision_id, scope, knowledge_type, title, content,
    source_type, source_id, confidence, revision_created_at, board_name,
    COALESCE(scored.score, 0.0::float8) AS score
FROM eligible
LEFT JOIN scored USING (item_id)
ORDER BY score DESC, revision_created_at DESC, item_id ASC
LIMIT $6
"#
);

/// Visibility check for `knowledge_get`: the same predicates as retrieval,
/// for the requested items only. `$4` is the item ids; `$1`, `$2`, `$3`, `$5`
/// as in [`RETRIEVAL_QUERY_SQL`].
const FETCH_VISIBLE_SQL: &str = concat!(
    r#"
SELECT
    i.id AS item_id, r.id AS revision_id, r.scope, r.knowledge_type, r.title, r.content,
    r.source_type, r.source_id, r.confidence, r.created_at AS revision_created_at,
    b.name AS board_name
FROM knowledge_items i
JOIN knowledge_revisions r ON r.id = i.active_revision_id
LEFT JOIN boards b ON b.id = r.board_id
WHERE "#,
    live_knowledge_predicates!(),
    agent_visible_predicates!(),
    r#"
  AND i.id = ANY($4::uuid[])
"#
);

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

/// Which boards a retrieval covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalScope {
    /// Board notes for these boards only (an empty list matches none).
    Boards(Vec<Uuid>),
    /// Board notes from every board; results carry `board_name`.
    AllBoards,
}

/// Load the eligible set for a run, full-text matches first. Non-matching
/// entries are included with score 0; callers wanting matches only filter on
/// `score > 0`. Workspace notes and this agent's own notes are always eligible.
pub async fn retrieve(
    pool: &PgPool,
    scope: &RetrievalScope,
    agent_id: Uuid,
    query_title: &str,
    query_body: &str,
    config: &KnowledgeRetrievalConfig,
) -> Result<Vec<RetrievedKnowledge>, sqlx::Error> {
    let terms = query_terms(query_title, query_body);
    let (board_ids, with_board_name) = scope_binding(scope);
    let rows = sqlx::query(RETRIEVAL_QUERY_SQL)
        .bind(board_ids)
        .bind(agent_id)
        .bind(&config.minimum_confidence)
        .bind(terms)
        .bind(&config.allowed_types)
        .bind(MAX_ELIGIBLE_CANDIDATES)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| retrieved_from_row(row, with_board_name, row.try_get("score")?))
        .collect()
}

/// The requested items, only those this agent may read under `scope`: the
/// same predicates as [`retrieve`]. Anything not visible is simply absent, so
/// callers cannot tell "hidden" from "unknown". Rows come back in the order of
/// `item_ids`; `score` is `0.0`.
pub async fn fetch_visible(
    pool: &PgPool,
    scope: &RetrievalScope,
    agent_id: Uuid,
    item_ids: &[Uuid],
    config: &KnowledgeRetrievalConfig,
) -> Result<Vec<RetrievedKnowledge>, sqlx::Error> {
    let (board_ids, with_board_name) = scope_binding(scope);
    let rows = sqlx::query(FETCH_VISIBLE_SQL)
        .bind(board_ids)
        .bind(agent_id)
        .bind(&config.minimum_confidence)
        .bind(item_ids)
        .bind(&config.allowed_types)
        .fetch_all(pool)
        .await?;
    let mut found = rows
        .iter()
        .map(|row| retrieved_from_row(row, with_board_name, 0.0))
        .collect::<Result<Vec<_>, _>>()?;
    found.sort_by_key(|k| item_ids.iter().position(|id| *id == k.item_id));
    Ok(found)
}

fn scope_binding(scope: &RetrievalScope) -> (Option<Vec<Uuid>>, bool) {
    match scope {
        RetrievalScope::Boards(ids) => (Some(ids.clone()), false),
        RetrievalScope::AllBoards => (None, true),
    }
}

fn retrieved_from_row(
    row: &sqlx::postgres::PgRow,
    with_board_name: bool,
    score: f64,
) -> Result<RetrievedKnowledge, sqlx::Error> {
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
        score,
        revision_created_at: row.try_get("revision_created_at")?,
        board_name: if with_board_name {
            row.try_get("board_name")?
        } else {
            None
        },
    })
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

/// One approved item's folded text, for pairwise comparison.
struct FoldedItem {
    id: Uuid,
    board_id: Option<Uuid>,
    title: String,
    content: String,
}

/// Pairs of approved, live items on the same board (or both workspace-wide)
/// that score at least [`MERGE_MIN_SCORE`], best first, at most `limit`.
/// Looks at the [`MERGE_SCAN_ITEMS`] most recently updated items that are
/// workspace-wide or on one of `board_ids`.
pub async fn near_duplicate_pairs(
    pool: &PgPool,
    board_ids: &[Uuid],
    limit: usize,
) -> Result<Vec<(Uuid, Uuid, f64)>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT i.id, r.board_id,
               coppice_unaccent(r.title) AS folded_title,
               coppice_unaccent(r.content) AS folded_content
        FROM knowledge_items i
        JOIN knowledge_revisions r ON r.id = i.active_revision_id
        WHERE i.status = 'approved'
          AND i.superseded_by IS NULL
          AND (i.expires_at IS NULL OR i.expires_at > now())
          AND (r.board_id IS NULL OR r.board_id = ANY($1))
        ORDER BY i.updated_at DESC, i.id
        LIMIT $2
        "#,
    )
    .bind(board_ids)
    .bind(MERGE_SCAN_ITEMS)
    .fetch_all(pool)
    .await?;
    let items = rows
        .iter()
        .map(|row| {
            Ok(FoldedItem {
                id: row.try_get("id")?,
                board_id: row.try_get("board_id")?,
                title: row.try_get("folded_title")?,
                content: row.try_get("folded_content")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(score_pairs(&items, limit))
}

fn score_pairs(items: &[FoldedItem], limit: usize) -> Vec<(Uuid, Uuid, f64)> {
    let mut pairs = Vec::new();
    for (index, first) in items.iter().enumerate() {
        for second in &items[index + 1..] {
            if first.board_id != second.board_id {
                continue;
            }
            let score =
                near_duplicate_score(&first.title, &first.content, &second.title, &second.content);
            if score >= MERGE_MIN_SCORE {
                pairs.push((first.id, second.id, score));
            }
        }
    }
    pairs.sort_by(|a, b| {
        b.2.total_cmp(&a.2)
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });
    pairs.truncate(limit);
    pairs
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
        assert!(INBOX_SIMILAR_QUERY_SQL.contains("ts_rank_cd"));
        assert!(!RETRIEVAL_QUERY_SQL.contains("<=>"));
        assert_eq!(ELIGIBLE_KNOWLEDGE_FROM, eligible_knowledge_from!());
        assert!(RETRIEVAL_QUERY_SQL.contains("r.scope = 'workspace'"));
        assert!(RETRIEVAL_QUERY_SQL.contains("r.board_id = ANY($1::uuid[])"));
        assert!(RETRIEVAL_QUERY_SQL.contains("$1::uuid[] IS NULL"));
        assert!(RETRIEVAL_QUERY_SQL.contains("r.agent_id = $2"));
        assert!(RETRIEVAL_QUERY_SQL.contains("LEFT JOIN boards"));
        assert!(FETCH_VISIBLE_SQL.contains("r.agent_id = $2"));
        assert!(FETCH_VISIBLE_SQL.contains("i.status = 'approved'"));
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
    fn pairs_need_the_same_board_and_a_high_score() {
        let board = Some(Uuid::new_v4());
        let item = |board_id, title: &str, content: &str| FoldedItem {
            id: Uuid::new_v4(),
            board_id,
            title: title.into(),
            content: content.into(),
        };
        let items = [
            item(board, DRAG_TITLE, DRAG_BODY),
            item(board, DRAG_TITLE, DRAG_BODY),
            item(None, DRAG_TITLE, DRAG_BODY),
            item(board, TIMEOUT_TITLE, TIMEOUT_BODY),
        ];
        let pairs = score_pairs(&items, 10);
        assert_eq!(pairs.len(), 1);
        assert_eq!((pairs[0].0, pairs[0].1), (items[0].id, items[1].id));
        assert_eq!(pairs[0].2, 1.0);
        assert!(score_pairs(&items, 0).is_empty());
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
