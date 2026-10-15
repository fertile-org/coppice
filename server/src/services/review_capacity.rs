//! Review-capacity cap.
//!
//! The Ready check counts tickets in the review columns. It does not read the
//! plan, the ticket description, or any comment, and it does not send a prompt.

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::copy::review::{ready_held, review_entry_held, waiting_on_review};
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::substatus::TicketStatus;
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::TransitionAction;
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::planning_service::PlanningService;
use crate::services::run_service::{RunError, RunService};

/// Counts tickets that occupy review capacity. Status filter is `$2`.
/// This statement does not read a plan or send a prompt.
pub const OCCUPANCY_SQL: &str = r#"
SELECT COUNT(*)::bigint
FROM tickets
WHERE board_id = $1
  AND archived_at IS NULL
  AND status = ANY($2)
"#;

const WORK_ON_TICKET: &str = "work_on_ticket";

#[derive(Debug, thiserror::Error)]
pub enum ReviewCapacityError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Message(String),
}

impl From<CommentError> for ReviewCapacityError {
    fn from(err: CommentError) -> Self {
        match err {
            CommentError::Database(err) => ReviewCapacityError::Database(err),
            other => ReviewCapacityError::Message(other.to_string()),
        }
    }
}

impl From<ReviewCapacityError> for crate::services::ticket_service::TicketError {
    fn from(err: ReviewCapacityError) -> Self {
        use crate::services::ticket_service::TicketError;
        match err {
            ReviewCapacityError::Database(err) => TicketError::Database(err),
            ReviewCapacityError::Message(message) => TicketError::Validation(message),
        }
    }
}

impl From<ReviewCapacityError> for RunError {
    fn from(err: ReviewCapacityError) -> Self {
        match err {
            ReviewCapacityError::Database(err) => RunError::Database(err),
            ReviewCapacityError::Message(message) => RunError::Validation(message),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReviewPolicy {
    pub cap: Option<i32>,
    pub count_in_review: bool,
}

/// Returned when a Ready `work_on_ticket` run must not start.
pub struct ReadyHold {
    pub message: String,
    /// Post `message` once. Further attempts while the hold stands stay quiet.
    pub post: bool,
}

pub struct ReviewCapacityService<'a> {
    pool: &'a PgPool,
}

impl<'a> ReviewCapacityService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn policy(&self, board_id: Uuid) -> Result<ReviewPolicy, ReviewCapacityError> {
        let row = sqlx::query(
            r#"
            SELECT review_cap, review_cap_counts_in_review
            FROM boards
            WHERE id = $1
            "#,
        )
        .bind(board_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| ReviewCapacityError::Message("board not found".into()))?;
        Ok(ReviewPolicy {
            cap: row.get("review_cap"),
            count_in_review: row.get("review_cap_counts_in_review"),
        })
    }

    pub async fn set_policy(
        &self,
        board_id: Uuid,
        policy: ReviewPolicy,
    ) -> Result<(), ReviewCapacityError> {
        let updated = sqlx::query(
            r#"
            UPDATE boards
            SET review_cap = $2,
                review_cap_counts_in_review = $3
            WHERE id = $1
            "#,
        )
        .bind(board_id)
        .bind(policy.cap)
        .bind(policy.count_in_review)
        .execute(self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(ReviewCapacityError::Message("board not found".into()));
        }
        self.release(board_id).await?;
        Ok(())
    }

    /// Tickets currently occupying the cap. Does not read the plan.
    pub async fn occupancy(
        &self,
        board_id: Uuid,
        count_in_review: bool,
    ) -> Result<i64, ReviewCapacityError> {
        let statuses = counted_status_strings(count_in_review);
        let count: i64 = sqlx::query_scalar(OCCUPANCY_SQL)
            .bind(board_id)
            .bind(&statuses)
            .fetch_one(self.pool)
            .await?;
        Ok(count)
    }

    pub async fn waiting_line(
        &self,
        board_id: Uuid,
    ) -> Result<Option<String>, ReviewCapacityError> {
        let policy = self.policy(board_id).await?;
        let count = self.occupancy(board_id, policy.count_in_review).await?;
        if count <= 0 {
            Ok(None)
        } else {
            Ok(Some(waiting_on_review(count)))
        }
    }

    /// `None` when the Ready ticket may start. Does not read the plan.
    pub async fn block_ready_start(
        &self,
        ticket_id: Uuid,
    ) -> Result<Option<ReadyHold>, ReviewCapacityError> {
        let row = sqlx::query(
            r#"
            SELECT board_id, review_hold
            FROM tickets
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| ReviewCapacityError::Message("ticket not found".into()))?;
        let board_id: Uuid = row.get("board_id");
        let already: bool = row.get("review_hold");
        let policy = self.policy(board_id).await?;
        let occupancy = self.occupancy(board_id, policy.count_in_review).await?;
        if ready_may_start(occupancy, policy.cap) {
            if already {
                self.clear_ready_hold(ticket_id).await?;
            }
            return Ok(None);
        }
        let post = if already {
            false
        } else {
            self.mark_ready_hold(ticket_id).await?;
            true
        };
        Ok(Some(ReadyHold {
            message: ready_held(occupancy),
            post,
        }))
    }

    /// Refusal for a direct column move, if this move would break the cap.
    /// Entering Wait for Human Review is queued. Leaving Ready into work is refused.
    pub async fn direct_status_block(
        &self,
        ticket_id: Uuid,
        from: TicketStatus,
        to: TicketStatus,
    ) -> Result<Option<String>, ReviewCapacityError> {
        if from == to {
            return Ok(None);
        }
        if to == TicketStatus::WaitForFinalReview {
            if let Some(denial) = self.entry_denial(ticket_id, from).await? {
                if self.mark_entry_held(ticket_id).await? {
                    self.post_system(ticket_id, &denial).await?;
                }
                return Ok(Some(denial));
            }
            return Ok(None);
        }
        if from == TicketStatus::Ready
            && matches!(to, TicketStatus::InProgress | TicketStatus::InReview)
        {
            let board_id = self.board_id(ticket_id).await?;
            let policy = self.policy(board_id).await?;
            let occupancy = self.occupancy(board_id, policy.count_in_review).await?;
            if !ready_may_start(occupancy, policy.cap) {
                return Ok(Some(ready_held(occupancy)));
            }
        }
        Ok(None)
    }

    /// `Some(message)` when the ticket must not enter Wait for Human Review.
    pub async fn entry_denial(
        &self,
        ticket_id: Uuid,
        from: TicketStatus,
    ) -> Result<Option<String>, ReviewCapacityError> {
        let row = sqlx::query(
            r#"
            SELECT board_id, review_cap_exempt
            FROM tickets
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| ReviewCapacityError::Message("ticket not found".into()))?;
        let board_id: Uuid = row.get("board_id");
        let exempt: bool = row.get("review_cap_exempt");
        let policy = self.policy(board_id).await?;
        let occupancy = self.occupancy(board_id, policy.count_in_review).await?;
        let already = counts_toward_cap(from, policy.count_in_review);
        if entry_allowed(occupancy, policy.cap, exempt, already) {
            Ok(None)
        } else {
            Ok(Some(review_entry_held(occupancy)))
        }
    }

    /// Hold a finished run that would enter Wait for Human Review while the cap is full.
    pub async fn constrain_finished_run(
        &self,
        ticket_id: Uuid,
        current: TicketStatus,
        action: &mut TransitionAction,
    ) -> Result<(), ReviewCapacityError> {
        if action.new_status == Some(TicketStatus::WaitForFinalReview) {
            if let Some(message) = self.entry_denial(ticket_id, current).await? {
                action.new_status = None;
                if matches!(action.new_assignee_id, Some(None)) {
                    action.new_assignee_id = None;
                }
                action.enqueue_jobs.clear();
                if self.mark_entry_held(ticket_id).await? {
                    action.system_comments.push(message);
                }
            }
        }
        Ok(())
    }

    /// A merge-conflict reopen may enter Wait for Human Review while the cap is full.
    pub async fn mark_merge_conflict_exempt(
        &self,
        ticket_id: Uuid,
    ) -> Result<(), ReviewCapacityError> {
        sqlx::query(
            r#"
            UPDATE tickets
            SET review_cap_exempt = true
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// After a status write: clear a used exemption, then let queued work move
    /// when a review slot opened.
    pub async fn after_status_write(
        &self,
        board_id: Uuid,
        ticket_id: Uuid,
        from: TicketStatus,
        to: TicketStatus,
    ) -> Result<(), ReviewCapacityError> {
        if to == TicketStatus::WaitForFinalReview {
            sqlx::query(
                r#"
                UPDATE tickets
                SET review_cap_exempt = false,
                    review_entry_held = false
                WHERE id = $1
                "#,
            )
            .bind(ticket_id)
            .execute(self.pool)
            .await?;
        }
        if from == TicketStatus::Ready && to != TicketStatus::Ready {
            self.clear_ready_hold(ticket_id).await?;
        }
        if frees_review_slot(from, to) {
            self.release(board_id).await?;
        }
        Ok(())
    }

    async fn release(&self, board_id: Uuid) -> Result<(), ReviewCapacityError> {
        for _ in 0..50 {
            let Some(held_id) = self.oldest_entry_held(board_id).await? else {
                break;
            };
            let status = self.ticket_status(held_id).await?;
            if self.entry_denial(held_id, status).await?.is_some() {
                break;
            }
            // Direct update, not `apply_workflow_update`: that path calls back
            // into `after_status_write`, which is what invoked this release.
            let waiting = status_to_str(TicketStatus::WaitForFinalReview);
            sqlx::query(
                r#"
                UPDATE tickets
                SET status = $2,
                    substatus = NULL,
                    substatus_metadata = NULL,
                    assignee_agent_id = NULL,
                    review_cap_exempt = false,
                    review_entry_held = false,
                    updated_at = now()
                WHERE id = $1
                  AND archived_at IS NULL
                  AND status <> $2
                "#,
            )
            .bind(held_id)
            .bind(waiting)
            .execute(self.pool)
            .await?;
        }

        let policy = self.policy(board_id).await?;
        let occupancy = self.occupancy(board_id, policy.count_in_review).await?;
        if !ready_may_start(occupancy, policy.cap) {
            return Ok(());
        }
        let ready = self.held_ready(board_id).await?;
        for (ticket_id, assignee) in ready {
            if let Some(agent_id) = assignee {
                match RunService::new(self.pool)
                    .start_run_for_agent(
                        ticket_id,
                        agent_id,
                        WORK_ON_TICKET,
                        crate::services::run_service::StartRunOptions::default(),
                    )
                    .await
                {
                    Ok(_) | Err(RunError::ActiveRunExists) | Err(RunError::NotFound) => {}
                    Err(RunError::Validation(message)) => {
                        tracing::info!(%message, "queued Ready ticket did not start");
                    }
                    Err(RunError::Database(err)) => return Err(ReviewCapacityError::Database(err)),
                }
            } else if let Err(err) = PlanningService::new(self.pool)
                .on_entered_ready(ticket_id)
                .await
            {
                match err {
                    crate::services::planning_service::PlanError::Database(err) => {
                        return Err(ReviewCapacityError::Database(err));
                    }
                    other => {
                        tracing::warn!(error = %other, "queued Ready ticket did not start");
                    }
                }
            }
        }
        Ok(())
    }

    async fn oldest_entry_held(&self, board_id: Uuid) -> Result<Option<Uuid>, ReviewCapacityError> {
        let waiting = status_to_str(TicketStatus::WaitForFinalReview);
        let id = sqlx::query_scalar(
            r#"
            SELECT id
            FROM tickets
            WHERE board_id = $1
              AND archived_at IS NULL
              AND review_entry_held = true
              AND status <> $2
            ORDER BY updated_at ASC, id ASC
            LIMIT 1
            "#,
        )
        .bind(board_id)
        .bind(waiting)
        .fetch_optional(self.pool)
        .await?;
        Ok(id)
    }

    async fn held_ready(
        &self,
        board_id: Uuid,
    ) -> Result<Vec<(Uuid, Option<Uuid>)>, ReviewCapacityError> {
        let ready = status_to_str(TicketStatus::Ready);
        let rows = sqlx::query(
            r#"
            SELECT id, assignee_agent_id
            FROM tickets
            WHERE board_id = $1
              AND archived_at IS NULL
              AND review_hold = true
              AND status = $2
            ORDER BY updated_at ASC, id ASC
            "#,
        )
        .bind(board_id)
        .bind(ready)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| (row.get("id"), row.get("assignee_agent_id")))
            .collect())
    }

    async fn ticket_status(&self, ticket_id: Uuid) -> Result<TicketStatus, ReviewCapacityError> {
        let status: String = sqlx::query_scalar("SELECT status FROM tickets WHERE id = $1")
            .bind(ticket_id)
            .fetch_one(self.pool)
            .await?;
        Ok(crate::domain::ticket::status_from_str(&status).unwrap_or(TicketStatus::Backlog))
    }

    async fn board_id(&self, ticket_id: Uuid) -> Result<Uuid, ReviewCapacityError> {
        sqlx::query_scalar("SELECT board_id FROM tickets WHERE id = $1")
            .bind(ticket_id)
            .fetch_one(self.pool)
            .await
            .map_err(ReviewCapacityError::from)
    }

    async fn mark_ready_hold(&self, ticket_id: Uuid) -> Result<(), ReviewCapacityError> {
        sqlx::query("UPDATE tickets SET review_hold = true WHERE id = $1")
            .bind(ticket_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    async fn clear_ready_hold(&self, ticket_id: Uuid) -> Result<(), ReviewCapacityError> {
        sqlx::query("UPDATE tickets SET review_hold = false WHERE id = $1")
            .bind(ticket_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    async fn mark_entry_held(&self, ticket_id: Uuid) -> Result<bool, ReviewCapacityError> {
        let updated = sqlx::query(
            r#"
            UPDATE tickets
            SET review_entry_held = true
            WHERE id = $1
              AND review_entry_held = false
            "#,
        )
        .bind(ticket_id)
        .execute(self.pool)
        .await?;
        Ok(updated.rows_affected() > 0)
    }

    async fn post_system(&self, ticket_id: Uuid, body: &str) -> Result<(), ReviewCapacityError> {
        CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::System,
                None,
                body,
                CommentIntent::SystemEvent,
                &[],
                &[],
            )
            .await?;
        Ok(())
    }
}

pub fn ready_may_start(occupancy: i64, cap: Option<i32>) -> bool {
    match cap {
        None => true,
        Some(cap) => occupancy < i64::from(cap),
    }
}

pub fn entry_allowed(
    occupancy: i64,
    cap: Option<i32>,
    exempt: bool,
    already_counted: bool,
) -> bool {
    exempt || already_counted || ready_may_start(occupancy, cap)
}

pub fn counts_toward_cap(status: TicketStatus, count_in_review: bool) -> bool {
    match status {
        TicketStatus::WaitForFinalReview => true,
        TicketStatus::InReview => count_in_review,
        _ => false,
    }
}

fn counted_status_strings(count_in_review: bool) -> Vec<String> {
    let mut statuses = vec![status_to_str(TicketStatus::WaitForFinalReview).to_string()];
    if count_in_review {
        statuses.push(status_to_str(TicketStatus::InReview).to_string());
    }
    statuses
}

/// A counted ticket left the columns the cap watches.
fn frees_review_slot(from: TicketStatus, to: TicketStatus) -> bool {
    if from == to {
        return false;
    }
    // Re-checked with the board flag inside `release`. Either column can be
    // the one that opened a slot, so wake the queue whenever one of them is left.
    let watched = matches!(
        from,
        TicketStatus::WaitForFinalReview | TicketStatus::InReview
    );
    let still_watched = matches!(
        to,
        TicketStatus::WaitForFinalReview | TicketStatus::InReview
    );
    watched && !still_watched
}

/// Empty is no cap. A set cap is a whole number of 1 or more.
pub fn review_cap_limit(value: Option<i32>, invalid: &str) -> Result<Option<i32>, String> {
    if let Some(value) = value {
        if value < 1 {
            return Err(invalid.to_string());
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_check_does_not_read_the_plan_or_send_a_prompt() {
        let occupancy = OCCUPANCY_SQL.to_ascii_lowercase();
        for word in [
            "plan",
            "prompt",
            "description",
            "comment",
            "summary",
            "body",
        ] {
            assert!(!occupancy.contains(word), "occupancy query contains {word}");
        }
    }

    #[test]
    fn cap_holds_ready_work_and_lets_a_merge_conflict_reopen_through() {
        assert!(ready_may_start(0, None));
        assert!(ready_may_start(4, None));
        assert!(ready_may_start(0, Some(1)));
        assert!(!ready_may_start(1, Some(1)));

        assert!(!entry_allowed(1, Some(1), false, false));
        assert!(entry_allowed(1, Some(1), true, false));
        assert!(entry_allowed(1, Some(1), false, true));
        assert!(entry_allowed(0, Some(1), false, false));
        assert!(entry_allowed(9, None, false, false));
    }

    #[test]
    fn review_cap_rejects_zero_and_keeps_empty() {
        let invalid = "invalid";
        assert_eq!(review_cap_limit(None, invalid), Ok(None));
        assert_eq!(review_cap_limit(Some(1), invalid), Ok(Some(1)));
        assert_eq!(review_cap_limit(Some(0), invalid), Err(invalid.to_string()));
        assert_eq!(review_cap_limit(Some(-1), invalid), Err(invalid.to_string()));
    }

    #[test]
    fn in_review_counts_only_when_the_board_asks() {
        assert!(counts_toward_cap(TicketStatus::WaitForFinalReview, false));
        assert!(!counts_toward_cap(TicketStatus::InReview, false));
        assert!(counts_toward_cap(TicketStatus::InReview, true));
        assert!(!counts_toward_cap(TicketStatus::Ready, true));
        assert!(!counts_toward_cap(TicketStatus::InProgress, true));
    }
}
