//! Final acceptance: Human Review → Done, by a person or by Coppice.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::git_state::TicketGitState;
use crate::services::comment_service::CommentError;
use crate::services::git_flow::{create_git_action_comment, Actor};
use crate::services::human_review_service::{HumanReviewError, HumanReviewService};
use crate::services::notification_service::NotificationService;
use crate::services::repo_service::RepoService;
use crate::services::ticket_git_service::{TicketGitError, TicketGitService};
use crate::services::ticket_service::{TicketError, TicketService, TicketWithDisplay};
use crate::services::workflow_service::WorkflowService;
use crate::AppState;

#[derive(Debug, thiserror::Error)]
pub enum AcceptError {
    #[error(transparent)]
    Ticket(#[from] TicketError),
    #[error("final approve requires wait_for_final_review")]
    NotWaitingForFinalReview,
    #[error(transparent)]
    Git(#[from] TicketGitError),
    #[error(transparent)]
    Review(#[from] HumanReviewError),
    #[error(transparent)]
    Comment(#[from] CommentError),
}

pub struct AcceptService<'a> {
    state: &'a Arc<AppState>,
    pool: &'a PgPool,
}

impl<'a> AcceptService<'a> {
    pub fn new(state: &'a Arc<AppState>, pool: &'a PgPool) -> Self {
        Self { state, pool }
    }

    /// Bind the acceptance to the reviewed commit, move the ticket to Done,
    /// post the activity comment and publish the update.
    pub async fn accept(
        &self,
        ticket_id: Uuid,
        actor: Actor,
    ) -> Result<TicketWithDisplay, AcceptError> {
        let ticket_svc = TicketService::new(self.pool);
        let ticket = ticket_svc.get(ticket_id).await?;
        WorkflowService::final_approve(ticket.ticket.status)
            .map_err(|_| AcceptError::NotWaitingForFinalReview)?;

        // A split parent has no branch of its own: its work merged with the
        // sub-tickets, so there is no commit to bind the acceptance to.
        if !ticket.split_parent {
            let reviewed = TicketGitService::from_state(self.state, self.pool)
                .reviewed_head_sha(ticket_id)
                .await?;
            if let Some(head_sha) = reviewed {
                HumanReviewService::new(self.pool)
                    .record_acceptance(ticket_id, actor.accepted_by(), &head_sha)
                    .await?;
            }
        }

        // Staged attachment copies stay until a successful merge or ticket
        // delete. A failed merge leaves the ticket in Done.
        let updated = ticket_svc.accept_to_done(ticket_id).await?;

        let body = match actor {
            Actor::User(_) => "Accepted: ticket moved to **Done**.",
            Actor::Coppice => {
                "Accepted by Coppice (human review skipped): ticket moved to **Done**."
            }
        };
        create_git_action_comment(self.state, self.pool, ticket_id, actor, body).await?;

        crate::events::publish_ticket_updated(&self.state.event_bus, &updated);
        if matches!(actor, Actor::Coppice) {
            self.notify_if_not_merging(&updated).await;
        }
        crate::services::git_chain::spawn_after_accept(self.state, ticket_id);
        Ok(updated)
    }

    /// After an auto-accept with merge-after-accept off, the accepted work
    /// waits for a person, so say so. A person who accepted already knows.
    /// With merge-after-accept on, the chain notifies only if it stops.
    async fn notify_if_not_merging(&self, ticket: &TicketWithDisplay) {
        let Some(repo_id) = ticket.ticket.repo_id.filter(|_| !ticket.split_parent) else {
            return;
        };
        let ticket_id = ticket.ticket.id;
        match RepoService::new(self.pool).get(repo_id).await {
            Ok(repo) if !repo.merge_after_accept => {
                if let Err(err) = NotificationService::new(self.pool)
                    .create_for_git_needs_action(ticket_id, TicketGitState::NotMerged, None)
                    .await
                {
                    tracing::warn!(%ticket_id, error = %err, "could not create needs-action notification");
                }
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(%ticket_id, error = %err, "could not load repo for accept"),
        }
    }
}
