//! Human Review acceptance bound to the commit that was reviewed.
//!
//! Review comments live on `ticket_comments` (`review_feedback`). Check
//! outcomes live there too (`qa_passed`, `qa_failed`). Runs do not keep a
//! test report after they finish (`submitted_result` is cleared), so the
//! acceptance records the run ids whose stored outcome it covers, plus any
//! run that still has `submitted_result`.

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::domain::substatus::TicketStatus;
use crate::services::workflow_service::WorkflowService;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanReview {
    pub ticket_id: Uuid,
    pub head_sha: String,
    pub comment_ids: Vec<Uuid>,
    pub run_ids: Vec<Uuid>,
    pub stale: bool,
    pub accepted_by: Option<Uuid>,
    pub accepted_at: time::OffsetDateTime,
}

#[derive(Debug, thiserror::Error)]
pub enum HumanReviewError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub struct HumanReviewService<'a> {
    pool: &'a PgPool,
}

impl<'a> HumanReviewService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, ticket_id: Uuid) -> Result<Option<HumanReview>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT ticket_id, head_sha, comment_ids, run_ids, stale, accepted_by, accepted_at
            FROM ticket_human_reviews
            WHERE ticket_id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.as_ref().map(row_to_review))
    }

    /// Record acceptance of `head_sha`, replacing any earlier acceptance.
    pub async fn record_acceptance(
        &self,
        ticket_id: Uuid,
        accepted_by: Uuid,
        head_sha: &str,
    ) -> Result<HumanReview, HumanReviewError> {
        let head_sha = normalize_sha(head_sha);
        let comment_ids = self.evidence_comment_ids(ticket_id).await?;
        let run_ids = self.evidence_run_ids(ticket_id).await?;

        let row = sqlx::query(
            r#"
            INSERT INTO ticket_human_reviews (
                ticket_id, head_sha, comment_ids, run_ids, stale, accepted_by, accepted_at, invalidated_at
            )
            VALUES ($1, $2, $3, $4, false, $5, now(), NULL)
            ON CONFLICT (ticket_id) DO UPDATE SET
                head_sha = EXCLUDED.head_sha,
                comment_ids = EXCLUDED.comment_ids,
                run_ids = EXCLUDED.run_ids,
                stale = false,
                accepted_by = EXCLUDED.accepted_by,
                accepted_at = now(),
                invalidated_at = NULL
            RETURNING ticket_id, head_sha, comment_ids, run_ids, stale, accepted_by, accepted_at
            "#,
        )
        .bind(ticket_id)
        .bind(&head_sha)
        .bind(&comment_ids)
        .bind(&run_ids)
        .bind(accepted_by)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_review(&row))
    }

    /// A commit landed that is not the accepted one. Mark the acceptance stale
    /// and move Done / Wait for Human Review back to In Review.
    ///
    /// Returns whether this call invalidated a live acceptance.
    pub async fn observe_head(
        &self,
        ticket_id: Uuid,
        head_sha: &str,
    ) -> Result<bool, HumanReviewError> {
        let head_sha = normalize_sha(head_sha);
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query(
            r#"
            UPDATE ticket_human_reviews
            SET stale = true, invalidated_at = now()
            WHERE ticket_id = $1
              AND stale = false
              AND lower(head_sha) <> lower($2)
            "#,
        )
        .bind(ticket_id)
        .bind(&head_sha)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(false);
        }

        let row = sqlx::query(
            r#"
            SELECT status, archived_at IS NOT NULL AS archived
            FROM tickets
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = row {
            let status_str: String = row.get("status");
            let archived: bool = row.get("archived");
            let current = crate::domain::ticket::status_from_str(&status_str)
                .unwrap_or(TicketStatus::Backlog);
            let next = WorkflowService::status_after_stale_review(current);
            if !archived && next != current {
                sqlx::query(
                    r#"
                    UPDATE tickets
                    SET status = $2,
                        substatus = NULL,
                        substatus_metadata = NULL,
                        updated_at = now()
                    WHERE id = $1
                    "#,
                )
                .bind(ticket_id)
                .bind(crate::domain::ticket::status_to_str(next))
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query("UPDATE tickets SET updated_at = now() WHERE id = $1")
                    .bind(ticket_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(true)
    }

    async fn evidence_comment_ids(&self, ticket_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"
            SELECT id
            FROM ticket_comments
            WHERE ticket_id = $1
              AND intent IN ('review_feedback', 'qa_passed', 'qa_failed')
            ORDER BY created_at ASC
            "#,
        )
        .bind(ticket_id)
        .fetch_all(self.pool)
        .await
    }

    async fn evidence_run_ids(&self, ticket_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"
            SELECT id
            FROM agent_runs
            WHERE ticket_id = $1
              AND (
                status IN ('succeeded', 'failed', 'blocked')
                OR submitted_result IS NOT NULL
              )
            ORDER BY created_at ASC
            "#,
        )
        .bind(ticket_id)
        .fetch_all(self.pool)
        .await
    }
}

pub fn normalize_sha(sha: &str) -> String {
    sha.trim().to_ascii_lowercase()
}

pub fn reviewed_sha_matches(approved: &str, current: &str) -> bool {
    let approved = approved.trim();
    let current = current.trim();
    !approved.is_empty() && approved.eq_ignore_ascii_case(current)
}

pub fn short_commit_sha(sha: &str) -> &str {
    let sha = sha.trim();
    if sha.len() >= 7 {
        sha.get(..7).unwrap_or(sha)
    } else {
        sha
    }
}

fn row_to_review(row: &sqlx::postgres::PgRow) -> HumanReview {
    HumanReview {
        ticket_id: row.get("ticket_id"),
        head_sha: row.get("head_sha"),
        comment_ids: row.get("comment_ids"),
        run_ids: row.get("run_ids"),
        stale: row.get("stale"),
        accepted_by: row.get("accepted_by"),
        accepted_at: row.get("accepted_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewed_sha_matches_ignores_case_and_whitespace() {
        assert!(reviewed_sha_matches("AbC123", "  abc123"));
        assert!(!reviewed_sha_matches("abc123", "abc124"));
        assert!(!reviewed_sha_matches("  ", "abc"));
    }

    #[test]
    fn short_commit_sha_is_seven_characters() {
        assert_eq!(short_commit_sha("abc1234deadbeef"), "abc1234");
        assert_eq!(short_commit_sha("abc"), "abc");
    }
}
