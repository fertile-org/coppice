//! Merge an accepted ticket branch into a base branch, then record the
//! outcome, post the activity comment and clean up.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::git_state::GitOutcome;
use crate::services::accept_service::{create_git_action_comment, Actor};
use crate::services::human_review_service::HumanReviewService;
use crate::services::ticket_attachments::{self, AttachmentFolderEnd};
use crate::services::ticket_git_service::{MergeBranchResult, TicketGitError, TicketGitService};
use crate::services::ticket_service::TicketService;
use crate::AppState;

pub const MAIN_DIRTY_REASON: &str = "Your main checkout has uncommitted changes";

/// Reason line for a merge conflict, such as "2 files conflict with main".
pub fn conflict_reason(file_count: usize, base_branch: &str) -> String {
    if file_count == 1 {
        format!("1 file conflicts with {base_branch}")
    } else {
        format!("{file_count} files conflict with {base_branch}")
    }
}

pub struct TicketMergeFlow<'a> {
    state: &'a Arc<AppState>,
    pool: &'a PgPool,
}

impl<'a> TicketMergeFlow<'a> {
    pub fn new(state: &'a Arc<AppState>, pool: &'a PgPool) -> Self {
        Self { state, pool }
    }

    pub async fn merge(
        &self,
        ticket_id: Uuid,
        base_branch: &str,
        actor: Actor,
    ) -> Result<MergeBranchResult, TicketGitError> {
        let merge = TicketGitService::from_state(self.state, self.pool)
            .merge_ticket_branch(ticket_id, base_branch)
            .await;
        let outcome = match &merge {
            Ok(_) => Some((None, None)),
            Err(TicketGitError::Conflict {
                base_branch, paths, ..
            }) => Some((
                Some(GitOutcome::Conflict),
                Some(conflict_reason(paths.len(), base_branch)),
            )),
            Err(TicketGitError::MainCheckoutDirty) => {
                Some((Some(GitOutcome::MainDirty), Some(MAIN_DIRTY_REASON.into())))
            }
            Err(_) => None,
        };
        if let Some((outcome, reason)) = outcome {
            if let Err(err) = HumanReviewService::new(self.pool)
                .set_git_outcome(ticket_id, outcome, reason.as_deref())
                .await
            {
                tracing::warn!(error = %err, %ticket_id, "could not record merge outcome");
            }
        }
        self.publish_ticket(ticket_id).await;
        let merge = merge?;

        // Accept does not remove these copies. A failed merge leaves them in
        // place.
        if let Err(err) = ticket_attachments::clear_if_ended(
            &self.state.config.storage.artifacts_dir,
            ticket_id,
            AttachmentFolderEnd::MergeSucceeded,
        ) {
            tracing::warn!(
                error = %err,
                %ticket_id,
                "could not remove staged ticket attachments after merge"
            );
        }

        let short_sha = merge.head_sha.get(..7).unwrap_or(&merge.head_sha);
        let comment_body = format!(
            "**Merge:** {} (`{short_sha}` on `{}`)",
            merge.message, merge.base_branch
        );
        create_git_action_comment(self.state, self.pool, ticket_id, actor, &comment_body)
            .await
            .map_err(TicketGitError::MergeComment)?;
        // The merged work is on the base branch now.
        crate::services::worktree_cleanup::spawn_cleanup(self.state, ticket_id);

        Ok(merge)
    }

    /// `ticket.updated` plus `agent_run.started` for each active run.
    async fn publish_ticket(&self, ticket_id: Uuid) {
        if let Ok(ticket) = TicketService::new(self.pool).get(ticket_id).await {
            crate::events::publish_ticket_updated(&self.state.event_bus, &ticket);
            crate::events::publish_ticket_active_runs(&self.state.event_bus, self.pool, ticket_id)
                .await;
        }
    }
}
