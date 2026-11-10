//! Server-owned git state of a ticket, and rechecks of the outcomes that
//! resolve outside Coppice (a push done by hand, a worktree removed by hand).

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::git_state::{derive_git_state, GitOutcome, GitStateInput, TicketGitState};
use crate::domain::substatus::TicketStatus;
use crate::services::git_ops::{ahead_behind, git_rev_parse};
use crate::services::human_review_service::{HumanReview, HumanReviewService};
use crate::services::ticket_git_service::{TicketGitError, TicketGitService};
use crate::services::ticket_service::TicketService;
use crate::AppState;

/// State from stored facts only. Runs no git command, so the board list can
/// use it for every ticket.
///
/// A ticket that is not Done is in review whatever its review row says: a
/// reopened ticket keeps its old (stale) row.
pub fn state_from_review(review: Option<&HumanReview>, status: TicketStatus) -> TicketGitState {
    let Some(review) = review.filter(|_| status == TicketStatus::Done) else {
        return TicketGitState::InReview;
    };
    derive_git_state(&GitStateInput {
        accepted: true,
        stale: review.stale,
        merged: review.merged_at.is_some(),
        outcome: review.git_outcome,
    })
}

pub struct GitStateService<'a> {
    state: &'a AppState,
    pool: &'a PgPool,
}

impl<'a> GitStateService<'a> {
    pub fn new(state: &'a AppState, pool: &'a PgPool) -> Self {
        Self { state, pool }
    }

    /// Clear a `NotPushed` or `WorktreeKept` outcome when local git says it is
    /// resolved. Never fetches. Returns the state afterwards.
    pub async fn recheck(&self, ticket_id: Uuid) -> Result<TicketGitState, TicketGitError> {
        let reviews = HumanReviewService::new(self.pool);
        let Some(review) = reviews.get(ticket_id).await? else {
            return Ok(TicketGitState::InReview);
        };
        let status = TicketService::new(self.pool)
            .get(ticket_id)
            .await?
            .ticket
            .status;
        let checked = review.git_outcome;
        let resolved = match checked {
            Some(GitOutcome::NotPushed) => self.not_pushed_resolved(ticket_id).await?,
            Some(GitOutcome::WorktreeKept) => {
                let ctx = TicketGitService::from_state(self.state, self.pool)
                    .resolve_context(ticket_id)
                    .await?;
                !ctx.worktree_dir.exists()
            }
            _ => false,
        };
        if !resolved {
            return Ok(state_from_review(Some(&review), status));
        }
        if let Some(checked) = checked {
            if reviews.clear_git_outcome_if(ticket_id, checked).await? {
                crate::events::publish_ticket_state(&self.state.event_bus, self.pool, ticket_id)
                    .await;
            }
        }
        let after = reviews.get(ticket_id).await?;
        Ok(state_from_review(after.as_ref(), status))
    }

    /// Recheck every ticket of a repo that is waiting on a push, for example
    /// after a push from Settings.
    pub async fn recheck_repo_not_pushed(&self, repo_id: Uuid) -> Result<(), sqlx::Error> {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT ticket.id
            FROM tickets ticket
            JOIN ticket_human_reviews review ON review.ticket_id = ticket.id
            WHERE ticket.repo_id = $1 AND review.git_outcome = 'not_pushed'
            "#,
        )
        .bind(repo_id)
        .fetch_all(self.pool)
        .await?;
        for ticket_id in ids {
            if let Err(err) = self.recheck(ticket_id).await {
                tracing::warn!(%ticket_id, error = %err, "git state recheck failed");
            }
        }
        Ok(())
    }

    /// The default branch has nothing the remote-tracking ref lacks.
    async fn not_pushed_resolved(&self, ticket_id: Uuid) -> Result<bool, TicketGitError> {
        let ctx = TicketGitService::from_state(self.state, self.pool)
            .resolve_context(ticket_id)
            .await?;
        let local_ref = format!("refs/heads/{}", ctx.default_branch);
        let remote_ref = format!("refs/remotes/origin/{}", ctx.default_branch);
        if git_rev_parse(&ctx.git_dir, &remote_ref).await?.is_none() {
            return Ok(false);
        }
        let (ahead, _behind) = ahead_behind(&ctx.git_dir, &local_ref, &remote_ref).await?;
        Ok(ahead == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(stale: bool) -> HumanReview {
        HumanReview {
            ticket_id: Uuid::nil(),
            head_sha: "abc".into(),
            comment_ids: vec![],
            run_ids: vec![],
            stale,
            accepted_by: None,
            accepted_at: time::OffsetDateTime::now_utc(),
            merged_at: None,
            accepted_by_coppice: false,
            merged_base_branch: None,
            merged_sha: None,
            git_outcome: None,
            git_outcome_reason: None,
        }
    }

    #[test]
    fn done_ticket_with_review_is_not_merged() {
        assert_eq!(
            state_from_review(Some(&review(false)), TicketStatus::Done),
            TicketGitState::NotMerged
        );
    }

    #[test]
    fn reopened_ticket_with_stale_row_is_in_review() {
        assert_eq!(
            state_from_review(Some(&review(true)), TicketStatus::InProgress),
            TicketGitState::InReview
        );
    }

    #[test]
    fn no_review_row_is_in_review() {
        assert_eq!(
            state_from_review(None, TicketStatus::Done),
            TicketGitState::InReview
        );
    }
}
