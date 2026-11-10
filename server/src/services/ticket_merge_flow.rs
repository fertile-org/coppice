//! Merge an accepted ticket branch into a base branch, then record the
//! outcome, post the activity comment and clean up.

use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::OwnedMutexGuard;
use uuid::Uuid;

use crate::domain::git_state::GitOutcome;
use crate::domain::substatus::TicketStatus;
use crate::services::git_chain;
use crate::services::git_flow::{create_git_action_comment, Actor};
use crate::services::git_state_service::GitStateService;
use crate::services::human_review_service::HumanReviewService;
use crate::services::repo_service::RepoService;
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

    /// Merge, then push when the repo asks for it, under the repo's chain
    /// lock.
    pub async fn merge(
        &self,
        ticket_id: Uuid,
        base_branch: &str,
        actor: Actor,
    ) -> Result<MergeBranchResult, TicketGitError> {
        let repo_id = self.repo_id(ticket_id).await;
        let _guard = self.lock_repo(repo_id).await;
        self.merge_locked(ticket_id, base_branch, actor, repo_id)
            .await
    }

    /// The chain's merge, as Coppice. Returns `None` without merging when the
    /// ticket is already merged or no longer Done. Checked under the lock, so
    /// two runs for one ticket merge once.
    pub async fn merge_after_accept(
        &self,
        ticket_id: Uuid,
        base_branch: &str,
    ) -> Result<Option<MergeBranchResult>, TicketGitError> {
        let repo_id = self.repo_id(ticket_id).await;
        let _guard = self.lock_repo(repo_id).await;
        let status = TicketService::new(self.pool)
            .get(ticket_id)
            .await?
            .ticket
            .status;
        if status != TicketStatus::Done {
            return Ok(None);
        }
        let review = HumanReviewService::new(self.pool).get(ticket_id).await?;
        if review.is_none_or(|review| review.merged_at.is_some()) {
            return Ok(None);
        }
        self.merge_locked(ticket_id, base_branch, Actor::Coppice, repo_id)
            .await
            .map(Some)
    }

    async fn lock_repo(&self, repo_id: Option<Uuid>) -> Option<OwnedMutexGuard<()>> {
        match repo_id {
            Some(id) => Some(git_chain::repo_lock(self.state, id).lock_owned().await),
            None => None,
        }
    }

    async fn repo_id(&self, ticket_id: Uuid) -> Option<Uuid> {
        match TicketService::new(self.pool).get(ticket_id).await {
            Ok(ticket) => ticket.ticket.repo_id,
            // The merge reports the error itself.
            Err(_) => None,
        }
    }

    /// Callers hold the repo's chain lock.
    async fn merge_locked(
        &self,
        ticket_id: Uuid,
        base_branch: &str,
        actor: Actor,
        repo_id: Option<Uuid>,
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
            if let Err(err) = GitStateService::set_outcome_and_notify(
                self.pool,
                ticket_id,
                outcome,
                reason.as_deref(),
            )
            .await
            {
                tracing::warn!(error = %err, %ticket_id, "could not record merge outcome");
            }
        }
        crate::events::publish_ticket_state(&self.state.event_bus, self.pool, ticket_id).await;
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
        // The merge commit exists and is recorded: a failed comment must not
        // skip the cleanup or the push. Its error is returned at the end.
        let comment =
            create_git_action_comment(self.state, self.pool, ticket_id, actor, &comment_body).await;
        // The merged work is on the base branch now.
        crate::services::worktree_cleanup::spawn_cleanup(self.state, ticket_id);

        // The push does not wait for the cleanup.
        if let Some(repo_id) = repo_id {
            match RepoService::new(self.pool).get(repo_id).await {
                Ok(repo) => {
                    git_chain::push_after_merge(self.state, ticket_id, &repo, &merge.base_branch)
                        .await;
                }
                Err(err) => {
                    tracing::warn!(error = %err, %ticket_id, "could not load repo to push");
                }
            }
        }

        comment.map_err(TicketGitError::MergeComment)?;
        Ok(merge)
    }
}
