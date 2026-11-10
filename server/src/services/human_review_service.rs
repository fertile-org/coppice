//! Human Review acceptance bound to the commit that was reviewed.
//!
//! Review comments live on `ticket_comments` (`review_feedback`). Check
//! outcomes live there too (`qa_passed`, `qa_failed`). Runs do not keep a
//! test report after they finish (`submitted_result` is cleared when the run
//! finishes), so the acceptance records the run ids whose stored outcome it
//! covers, plus any run that still holds a submitted result.

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::domain::git_state::GitOutcome;
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
    pub merged_at: Option<time::OffsetDateTime>,
    pub accepted_by_coppice: bool,
    pub merged_base_branch: Option<String>,
    pub merged_sha: Option<String>,
    pub git_outcome: Option<GitOutcome>,
    pub git_outcome_reason: Option<String>,
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
            SELECT ticket_id, head_sha, comment_ids, run_ids, stale, accepted_by, accepted_at, merged_at,
                   accepted_by_coppice, merged_base_branch, merged_sha, git_outcome, git_outcome_reason
            FROM ticket_human_reviews
            WHERE ticket_id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.as_ref().map(row_to_review))
    }

    /// Reviews for many tickets in one query, keyed by ticket.
    pub async fn get_many(
        &self,
        ticket_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, HumanReview>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT ticket_id, head_sha, comment_ids, run_ids, stale, accepted_by, accepted_at, merged_at,
                   accepted_by_coppice, merged_base_branch, merged_sha, git_outcome, git_outcome_reason
            FROM ticket_human_reviews
            WHERE ticket_id = ANY($1)
            "#,
        )
        .bind(ticket_ids)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(row_to_review)
            .map(|review| (review.ticket_id, review))
            .collect())
    }

    /// Record acceptance of `head_sha`, replacing any earlier acceptance.
    pub async fn record_acceptance(
        &self,
        ticket_id: Uuid,
        accepted_by: Option<Uuid>,
        head_sha: &str,
    ) -> Result<HumanReview, HumanReviewError> {
        let head_sha = normalize_sha(head_sha);
        let comment_ids = self.evidence_comment_ids(ticket_id).await?;
        let run_ids = self.evidence_run_ids(ticket_id).await?;

        let row = sqlx::query(
            r#"
            INSERT INTO ticket_human_reviews (
                ticket_id, head_sha, comment_ids, run_ids, stale, accepted_by, accepted_at, invalidated_at,
                accepted_by_coppice
            )
            VALUES ($1, $2, $3, $4, false, $5, now(), NULL, $6)
            ON CONFLICT (ticket_id) DO UPDATE SET
                head_sha = EXCLUDED.head_sha,
                comment_ids = EXCLUDED.comment_ids,
                run_ids = EXCLUDED.run_ids,
                stale = false,
                accepted_by = EXCLUDED.accepted_by,
                accepted_at = now(),
                invalidated_at = NULL,
                accepted_by_coppice = EXCLUDED.accepted_by_coppice,
                merged_sha = NULL,
                merged_at = NULL,
                merged_base_branch = NULL,
                git_outcome = NULL,
                git_outcome_reason = NULL,
                git_outcome_at = NULL
            RETURNING ticket_id, head_sha, comment_ids, run_ids, stale, accepted_by, accepted_at, merged_at,
                   accepted_by_coppice, merged_base_branch, merged_sha, git_outcome, git_outcome_reason
            "#,
        )
        .bind(ticket_id)
        .bind(&head_sha)
        .bind(&comment_ids)
        .bind(&run_ids)
        .bind(accepted_by)
        .bind(accepted_by.is_none())
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_review(&row))
    }

    /// Record that the accepted commit was merged. System-written by the merge
    /// path only. A later acceptance clears this.
    pub async fn record_merge(
        &self,
        ticket_id: Uuid,
        merged_sha: &str,
        base_branch: &str,
    ) -> Result<(), HumanReviewError> {
        let merged_sha = normalize_sha(merged_sha);
        sqlx::query(
            r#"
            UPDATE ticket_human_reviews
            SET merged_sha = $2, merged_at = now(), merged_base_branch = $3
            WHERE ticket_id = $1
            RETURNING ticket_id
            "#,
        )
        .bind(ticket_id)
        .bind(&merged_sha)
        .bind(base_branch)
        .fetch_one(self.pool)
        .await?;
        self.advance_split_after_merge(ticket_id).await;
        Ok(())
    }

    /// Store (or clear, with `None`) the outcome of a failed git step.
    ///
    /// The reason is refreshed even when the outcome is unchanged, so a second
    /// conflict with other files shows the new count. Returns `true` only when
    /// the outcome itself changed, so callers notify only then.
    pub async fn set_git_outcome(
        &self,
        ticket_id: Uuid,
        outcome: Option<GitOutcome>,
        reason: Option<&str>,
    ) -> Result<bool, HumanReviewError> {
        let outcome_str = outcome.map(GitOutcome::as_str);
        let changed: Option<bool> = sqlx::query_scalar(
            r#"
            WITH prev AS (
                SELECT ticket_id, git_outcome
                FROM ticket_human_reviews
                WHERE ticket_id = $1
                FOR UPDATE
            )
            UPDATE ticket_human_reviews r
            SET git_outcome = $2,
                git_outcome_reason = $3,
                git_outcome_at = CASE WHEN $2::text IS NULL THEN NULL ELSE now() END
            FROM prev
            WHERE r.ticket_id = prev.ticket_id
              AND (r.git_outcome IS DISTINCT FROM $2::text
                   OR r.git_outcome_reason IS DISTINCT FROM $3::text)
            RETURNING prev.git_outcome IS DISTINCT FROM $2::text
            "#,
        )
        .bind(ticket_id)
        .bind(outcome_str)
        .bind(reason)
        .fetch_optional(self.pool)
        .await?;
        Ok(changed.unwrap_or(false))
    }

    /// Record a remote merge only while this acceptance is still the live one.
    ///
    /// Returns `true` when a row changed. A racing local merge, a re-accept,
    /// or a stale mark between listing and this write returns `false`.
    pub async fn record_remote_merge_if_current(
        &self,
        ticket_id: Uuid,
        accepted_sha: &str,
        merged_sha: &str,
    ) -> Result<bool, HumanReviewError> {
        let accepted_sha = normalize_sha(accepted_sha);
        let merged_sha = normalize_sha(merged_sha);
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query(
            r#"
            UPDATE ticket_human_reviews
            SET merged_sha = $2, merged_at = now()
            WHERE ticket_id = $1
              AND stale = false
              AND merged_at IS NULL
              AND lower(head_sha) = lower($3)
            "#,
        )
        .bind(ticket_id)
        .bind(&merged_sha)
        .bind(&accepted_sha)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("UPDATE tickets SET updated_at = now() WHERE id = $1")
            .bind(ticket_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.advance_split_after_merge(ticket_id).await;
        Ok(true)
    }

    /// Both merge writers land here: the Merge button and the GitHub sweep.
    /// A failed advance does not undo the merge that was just recorded.
    async fn advance_split_after_merge(&self, ticket_id: Uuid) {
        if let Err(err) = crate::services::split_autopilot::SplitAutopilot::new(self.pool)
            .on_child_merged(ticket_id)
            .await
        {
            tracing::warn!(%ticket_id, error = %err, "split autopilot did not advance");
        }
    }

    /// Keep the acceptance row and mark it stale. Does not change ticket status,
    /// the accepted commit, or who accepted it.
    /// Takes an executor so a run start can do this in its own transaction.
    pub async fn retain_stale_acceptance(
        executor: impl sqlx::PgExecutor<'_>,
        ticket_id: Uuid,
    ) -> Result<(), HumanReviewError> {
        sqlx::query(
            r#"
            UPDATE ticket_human_reviews
            SET stale = true,
                invalidated_at = COALESCE(invalidated_at, now())
            WHERE ticket_id = $1
            "#,
        )
        .bind(ticket_id)
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Move a ticket that has no acceptance row back to Wait for Human Review
    /// so Accept can record one. Does not insert a review row. A ticket already
    /// in that column is left unchanged.
    pub async fn return_for_acceptance(&self, ticket_id: Uuid) -> Result<(), HumanReviewError> {
        let status = crate::domain::ticket::status_to_str(TicketStatus::WaitForFinalReview);
        sqlx::query(
            r#"
            UPDATE tickets
            SET status = $2,
                substatus = NULL,
                substatus_metadata = NULL,
                updated_at = now()
            WHERE id = $1
              AND archived_at IS NULL
              AND status <> $2
            "#,
        )
        .bind(ticket_id)
        .bind(status)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// A commit landed that is not the accepted one. Mark the acceptance stale
    /// and move Done / Wait for Human Review back to In Review.
    ///
    /// No acceptance row: no-op. Nothing is inserted and the status is unchanged.
    /// A ticket whose acceptance has `merged_at` set is also left alone.
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
              AND merged_at IS NULL
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
        merged_at: row.get("merged_at"),
        accepted_by_coppice: row.get("accepted_by_coppice"),
        merged_base_branch: row.get("merged_base_branch"),
        merged_sha: row.get("merged_sha"),
        git_outcome: row
            .get::<Option<String>, _>("git_outcome")
            .and_then(|s| GitOutcome::parse_str(&s)),
        git_outcome_reason: row.get("git_outcome_reason"),
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
