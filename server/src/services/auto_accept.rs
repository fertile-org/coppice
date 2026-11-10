//! Auto-accept at final review for tickets flagged to skip human review.
//!
//! Coppice accepts only when every safety check passes. Anything else leaves
//! the ticket waiting for a person.

use std::sync::Arc;

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::substatus::TicketStatus;
use crate::services::accept_service::{AcceptError, AcceptService};
use crate::services::comment_service::CommentService;
use crate::services::git_flow::Actor;
use crate::services::ticket_service::{TicketError, TicketService, TicketWithDisplay};
use crate::AppState;

/// Why a flagged ticket still waits for a person. Checked in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReviewBlock {
    NotFlagged,
    NoAgentReview,
    ChangesRequested,
    Split,
}

impl SkipReviewBlock {
    /// The ticket comment for this block. `NotFlagged` stays silent.
    pub fn notice(self) -> Option<&'static str> {
        use crate::copy::review::{
            SKIP_REVIEW_CHANGES_REQUESTED, SKIP_REVIEW_NO_AGENT_REVIEW, SKIP_REVIEW_SPLIT,
        };
        match self {
            SkipReviewBlock::NotFlagged => None,
            SkipReviewBlock::NoAgentReview => Some(SKIP_REVIEW_NO_AGENT_REVIEW),
            SkipReviewBlock::ChangesRequested => Some(SKIP_REVIEW_CHANGES_REQUESTED),
            SkipReviewBlock::Split => Some(SKIP_REVIEW_SPLIT),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkipReviewFacts {
    pub flagged: bool,
    pub agent_review_passed: bool,
    pub changes_requested: bool,
    pub split: bool,
}

/// Pure gate: the first block in [`SkipReviewBlock`] order, or `Ok` when
/// Coppice may accept without a person.
pub fn skip_review_eligibility(facts: &SkipReviewFacts) -> Result<(), SkipReviewBlock> {
    if !facts.flagged {
        return Err(SkipReviewBlock::NotFlagged);
    }
    if !facts.agent_review_passed {
        return Err(SkipReviewBlock::NoAgentReview);
    }
    if facts.changes_requested {
        return Err(SkipReviewBlock::ChangesRequested);
    }
    if facts.split {
        return Err(SkipReviewBlock::Split);
    }
    Ok(())
}

/// Record that a reviewer or Tech Lead passed the ticket out of In Review.
/// `at` is the approving comment's time, so that comment is not read as a
/// change request. With no comment the database clock is used.
pub async fn record_agent_review_pass(
    pool: &PgPool,
    ticket_id: Uuid,
    at: Option<OffsetDateTime>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE tickets SET agent_review_passed_at = COALESCE($2, now()) WHERE id = $1")
        .bind(ticket_id)
        .bind(at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Forget an earlier pass: the code or the review changed since.
pub async fn clear_agent_review_pass(pool: &PgPool, ticket_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tickets SET agent_review_passed_at = NULL \
         WHERE id = $1 AND agent_review_passed_at IS NOT NULL",
    )
    .bind(ticket_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Accept a ticket waiting in final review when it is flagged to skip human
/// review and every check passes. Returns `true` when Coppice accepted it.
/// A flagged ticket that is blocked gets one comment that says why.
pub async fn try_auto_accept(state: &Arc<AppState>, ticket_id: Uuid) -> Result<bool, AcceptError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(false);
    };
    let ticket = TicketService::new(pool).get(ticket_id).await?;
    if ticket.ticket.status != TicketStatus::WaitForFinalReview {
        return Ok(false);
    }
    let facts = load_facts(pool, &ticket)
        .await
        .map_err(TicketError::Database)?;
    match skip_review_eligibility(&facts) {
        Ok(()) => {
            // AcceptService also starts the merge and push chain.
            AcceptService::new(state, pool)
                .accept(ticket_id, Actor::Coppice)
                .await?;
            tracing::info!(%ticket_id, "accepted by Coppice (human review skipped)");
            Ok(true)
        }
        Err(block) => {
            if let Some(notice) = block.notice() {
                post_notice_once(state, pool, ticket_id, notice).await?;
            }
            Ok(false)
        }
    }
}

async fn load_facts(
    pool: &PgPool,
    ticket: &TicketWithDisplay,
) -> Result<SkipReviewFacts, sqlx::Error> {
    let ticket_id = ticket.ticket.id;
    let passed_at: Option<OffsetDateTime> =
        sqlx::query_scalar("SELECT agent_review_passed_at FROM tickets WHERE id = $1")
            .bind(ticket_id)
            .fetch_one(pool)
            .await?;
    // Every reviewer comment is `review_feedback`, approvals included. The
    // approval that set `agent_review_passed_at` shares its timestamp, so only
    // earlier feedback counts as a change request. Any earlier review comment
    // counts, even an earlier approval: the ticket came back for more work.
    let changes_requested: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM ticket_comments
            WHERE ticket_id = $1
              AND author_type = 'agent'
              AND (
                  intent = 'qa_failed'
                  OR (intent = 'review_feedback' AND ($2::timestamptz IS NULL OR created_at < $2))
              )
        )
        "#,
    )
    .bind(ticket_id)
    .bind(passed_at)
    .fetch_one(pool)
    .await?;
    let has_children: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tickets WHERE parent_ticket_id = $1)")
            .bind(ticket_id)
            .fetch_one(pool)
            .await?;
    Ok(SkipReviewFacts {
        flagged: ticket.ticket.skip_human_review,
        agent_review_passed: passed_at.is_some(),
        changes_requested,
        split: ticket.split_parent || has_children,
    })
}

/// One notice per entry into final review: skip it when the latest comment
/// is already this notice.
async fn post_notice_once(
    state: &AppState,
    pool: &PgPool,
    ticket_id: Uuid,
    notice: &str,
) -> Result<(), AcceptError> {
    let latest: Option<(String, String)> = sqlx::query_as(
        "SELECT author_type, body FROM ticket_comments WHERE ticket_id = $1 \
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(ticket_id)
    .fetch_optional(pool)
    .await
    .map_err(TicketError::Database)?;
    if latest.is_some_and(|(author, body)| author == "system" && body == notice) {
        return Ok(());
    }
    let comment = CommentService::new(pool)
        .create(
            ticket_id,
            AuthorType::System,
            None,
            notice,
            CommentIntent::SystemEvent,
            &[],
            &[],
        )
        .await?;
    state
        .event_bus
        .publish(crate::events::AppEvent::CommentCreated {
            comment_id: comment.id,
            ticket_id,
            author_type: "system".into(),
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean() -> SkipReviewFacts {
        SkipReviewFacts {
            flagged: true,
            agent_review_passed: true,
            changes_requested: false,
            split: false,
        }
    }

    #[test]
    fn clean_flagged_ticket_is_eligible() {
        assert_eq!(skip_review_eligibility(&clean()), Ok(()));
    }

    #[test]
    fn unflagged_ticket_is_blocked_first() {
        let facts = SkipReviewFacts {
            flagged: false,
            agent_review_passed: false,
            changes_requested: true,
            split: true,
        };
        assert_eq!(
            skip_review_eligibility(&facts),
            Err(SkipReviewBlock::NotFlagged)
        );
    }

    #[test]
    fn missing_agent_review_blocks() {
        let facts = SkipReviewFacts {
            agent_review_passed: false,
            changes_requested: true,
            split: true,
            ..clean()
        };
        assert_eq!(
            skip_review_eligibility(&facts),
            Err(SkipReviewBlock::NoAgentReview)
        );
    }

    #[test]
    fn requested_changes_block() {
        let facts = SkipReviewFacts {
            changes_requested: true,
            split: true,
            ..clean()
        };
        assert_eq!(
            skip_review_eligibility(&facts),
            Err(SkipReviewBlock::ChangesRequested)
        );
    }

    #[test]
    fn split_ticket_is_blocked() {
        let facts = SkipReviewFacts {
            split: true,
            ..clean()
        };
        assert_eq!(skip_review_eligibility(&facts), Err(SkipReviewBlock::Split));
    }

    #[test]
    fn only_not_flagged_is_silent() {
        assert_eq!(SkipReviewBlock::NotFlagged.notice(), None);
        for block in [
            SkipReviewBlock::NoAgentReview,
            SkipReviewBlock::ChangesRequested,
            SkipReviewBlock::Split,
        ] {
            assert!(block
                .notice()
                .is_some_and(|text| text.starts_with("Skip human review didn't apply: ")));
        }
    }
}
