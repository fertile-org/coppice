//! Final acceptance: Human Review → Done, by a person or by Coppice.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::comment::{AuthorType, CommentIntent};
use crate::events::bus::AppEvent;
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::human_review_service::{HumanReviewError, HumanReviewService};
use crate::services::ticket_git_service::{TicketGitError, TicketGitService};
use crate::services::ticket_service::{TicketError, TicketService, TicketWithDisplay};
use crate::services::workflow_service::WorkflowService;
use crate::AppState;

/// Who triggered a git-flow step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    User(Uuid),
    /// The server itself, for example a skipped human review.
    Coppice,
}

impl Actor {
    fn author(self) -> (AuthorType, Option<Uuid>, &'static str) {
        match self {
            Actor::User(id) => (AuthorType::Human, Some(id), "human"),
            Actor::Coppice => (AuthorType::System, None, "system"),
        }
    }

    fn accepted_by(self) -> Option<Uuid> {
        match self {
            Actor::User(id) => Some(id),
            Actor::Coppice => None,
        }
    }
}

/// Post a `SystemEvent` comment for a git-flow step and announce it.
pub(crate) async fn create_git_action_comment(
    state: &AppState,
    pool: &PgPool,
    ticket_id: Uuid,
    actor: Actor,
    body: &str,
) -> Result<(), CommentError> {
    let (author_type, author_id, author_label) = actor.author();
    let comment = CommentService::new(pool)
        .create(
            ticket_id,
            author_type,
            author_id,
            body,
            CommentIntent::SystemEvent,
            &[],
            &[],
        )
        .await?;
    state.event_bus.publish(AppEvent::CommentCreated {
        comment_id: comment.id,
        ticket_id,
        author_type: author_label.into(),
    });
    Ok(())
}

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
        Ok(updated)
    }
}
