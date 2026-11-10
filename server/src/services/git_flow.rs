//! Shared pieces of the ticket git flow (accept, merge, push).

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::comment::{AuthorType, CommentIntent};
use crate::events::bus::AppEvent;
use crate::services::comment_service::{CommentError, CommentService};
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

    pub(crate) fn accepted_by(self) -> Option<Uuid> {
        match self {
            Actor::User(id) => Some(id),
            Actor::Coppice => None,
        }
    }
}

/// Post a `SystemEvent` comment for a git-flow step and announce it.
pub async fn create_git_action_comment(
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
