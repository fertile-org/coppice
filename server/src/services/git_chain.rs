//! The chain after accept: merge into the default branch, clean up the
//! worktree, then push. Each step follows the repo's switches, which default
//! to off. A failed step is recorded on the review row; the ticket stays in
//! Done.

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::domain::git_state::GitOutcome;
use crate::domain::repo::Repo;
use crate::services::git_ops::push_gate;
use crate::services::git_state_service::GitStateService;
use crate::services::human_review_service::HumanReviewService;
use crate::services::repo_git_service::{RepoGitError, RepoGitService};
use crate::services::repo_service::RepoService;
use crate::services::ticket_git_service::TicketGitError;
use crate::services::ticket_merge_flow::TicketMergeFlow;
use crate::services::ticket_service::TicketService;
use crate::AppState;

/// One lock per repo id. A merge and the push after it run under it, so two
/// accepts on the same repo never merge at the same time.
pub type GitChainLocks = Arc<std::sync::Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>>;

/// The lock for `repo_id`, created on first use.
pub fn repo_lock(state: &AppState, repo_id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = state
        .git_chain_locks
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    locks.entry(repo_id).or_default().clone()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainOutcome {
    /// Merge after accept is off, or there was nothing to do.
    Skipped,
    Merged,
    MergedAndPushed,
    /// A step failed and its outcome is recorded on the review row.
    Stopped(GitOutcome),
    /// The merge failed for another reason. The ticket stays `not_merged`
    /// with the reason on the review row.
    MergeFailed,
}

/// Run the chain off the request path.
pub fn spawn_after_accept(state: &Arc<AppState>, ticket_id: Uuid) {
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let outcome = run_after_accept(&state, ticket_id).await;
        tracing::debug!(%ticket_id, ?outcome, "git chain after accept finished");
    });
}

/// Merge the accepted ticket into the repo's default branch when the repo
/// asks for it. The merge pushes afterwards when the repo asks for that too.
/// Safe to run twice: an already merged ticket is skipped.
pub async fn run_after_accept(state: &Arc<AppState>, ticket_id: Uuid) -> ChainOutcome {
    let Some(pool) = state.db.as_ref() else {
        return ChainOutcome::Skipped;
    };
    let repo_id = match TicketService::new(pool).get(ticket_id).await {
        Ok(ticket) => ticket.ticket.repo_id,
        Err(err) => {
            tracing::warn!(%ticket_id, error = %err, "git chain: could not load ticket");
            return ChainOutcome::Skipped;
        }
    };
    let Some(repo_id) = repo_id else {
        return ChainOutcome::Skipped;
    };
    let repo = match RepoService::new(pool).get(repo_id).await {
        Ok(repo) => repo,
        Err(err) => {
            tracing::warn!(%ticket_id, error = %err, "git chain: could not load repo");
            return ChainOutcome::Skipped;
        }
    };
    if !repo.merge_after_accept {
        return ChainOutcome::Skipped;
    }

    let merged = TicketMergeFlow::new(state, pool)
        .merge_after_accept(ticket_id, &repo.default_branch)
        .await;
    match merged {
        Ok(None) => ChainOutcome::Skipped,
        Ok(Some(_)) => after_merge(pool, ticket_id, &repo).await,
        // Merged, cleaned up and pushed; only the activity comment failed.
        Err(TicketGitError::MergeComment(err)) => {
            tracing::warn!(%ticket_id, error = %err, "git chain: merge comment failed");
            after_merge(pool, ticket_id, &repo).await
        }
        Err(TicketGitError::Conflict { .. }) => ChainOutcome::Stopped(GitOutcome::Conflict),
        Err(TicketGitError::MainCheckoutDirty) => ChainOutcome::Stopped(GitOutcome::MainDirty),
        Err(err) => {
            tracing::warn!(%ticket_id, error = %err, "git chain: merge failed");
            let reason = merge_failure_reason(&err);
            match GitStateService::record_not_merged_reason(pool, ticket_id, &reason).await {
                Ok(true) => {
                    crate::events::publish_ticket_state(&state.event_bus, pool, ticket_id).await;
                }
                Ok(false) => {}
                Err(err) => {
                    tracing::warn!(%ticket_id, error = %err, "could not record merge failure");
                }
            }
            ChainOutcome::MergeFailed
        }
    }
}

/// Longest error detail kept on the review row.
const MERGE_FAILURE_DETAIL_MAX: usize = 160;

/// "Auto-merge didn't run: <detail>", with a short detail: the first line of
/// the error, cut to a limit. Internal errors show no detail.
fn merge_failure_reason(err: &TicketGitError) -> String {
    let detail = match err {
        TicketGitError::Io(_) | TicketGitError::Database(_) | TicketGitError::Secret(_) => {
            "internal error".to_string()
        }
        _ => {
            let text = err.to_string();
            let line = text.lines().next().unwrap_or_default().trim();
            match line.char_indices().nth(MERGE_FAILURE_DETAIL_MAX) {
                Some((cut, _)) => format!("{}…", &line[..cut]),
                None => line.to_string(),
            }
        }
    };
    format!("Auto-merge didn't run: {detail}")
}

/// The outcome once the merge succeeded: the push may still have failed.
async fn after_merge(pool: &sqlx::PgPool, ticket_id: Uuid, repo: &Repo) -> ChainOutcome {
    let not_pushed = match HumanReviewService::new(pool).get(ticket_id).await {
        Ok(review) => review.and_then(|r| r.git_outcome) == Some(GitOutcome::NotPushed),
        Err(err) => {
            tracing::warn!(%ticket_id, error = %err, "git chain: could not read outcome");
            false
        }
    };
    if not_pushed {
        ChainOutcome::Stopped(GitOutcome::NotPushed)
    } else if repo.push_after_merge {
        ChainOutcome::MergedAndPushed
    } else {
        ChainOutcome::Merged
    }
}

/// Push the default branch after a successful merge into it, when the repo
/// asks for it. Uses the Settings push and its gates; never forces. A failed
/// push is recorded as `NotPushed` with its reason.
///
/// Callers hold the repo's chain lock.
pub async fn push_after_merge(
    state: &Arc<AppState>,
    ticket_id: Uuid,
    repo: &Repo,
    base_branch: &str,
) {
    if !repo.push_after_merge || base_branch != repo.default_branch {
        return;
    }
    let Some(pool) = state.db.as_ref() else {
        return;
    };
    let repo_git = RepoGitService::new(pool, state.config.git.push_enabled, &state.secret_store);
    // Nothing to push, for example after "Already up to date": the merged
    // work is on the remote already.
    if let Ok(status) = repo_git.default_branch_sync_status(repo.id).await {
        if status.ahead_count == Some(0) {
            return;
        }
    }
    let pushed = repo_git.push_default_branch(repo.id).await;
    match pushed {
        Ok(_) => {
            // The whole default branch went out, so other tickets waiting on
            // a push may be resolved too.
            if let Err(err) = GitStateService::new(state, pool)
                .recheck_repo_not_pushed(repo.id)
                .await
            {
                tracing::warn!(repo_id = %repo.id, error = %err, "git state recheck failed");
            }
        }
        Err(err) => {
            let reason = push_failure_reason(&err, state.config.git.push_enabled, repo);
            tracing::info!(%ticket_id, %reason, "push after merge did not run");
            match GitStateService::set_outcome_and_notify(
                pool,
                ticket_id,
                Some(GitOutcome::NotPushed),
                Some(&reason),
            )
            .await
            {
                Ok(_) => {
                    crate::events::publish_ticket_state(&state.event_bus, pool, ticket_id).await;
                }
                Err(err) => {
                    tracing::warn!(%ticket_id, error = %err, "could not record push outcome");
                }
            }
        }
    }
}

/// A short reason for the review row. Gate reasons come from the same gate
/// Settings uses. Git errors are already free of the forge token.
fn push_failure_reason(err: &RepoGitError, push_enabled: bool, repo: &Repo) -> String {
    match err {
        RepoGitError::PushDisabled | RepoGitError::NoRemoteUrl => {
            push_gate(push_enabled, repo.remote_url.as_deref())
                .1
                .unwrap_or_else(|| err.to_string())
        }
        RepoGitError::Git(msg) => msg.clone(),
        RepoGitError::RepoNotFound | RepoGitError::RepoNotReady | RepoGitError::NoForgeToken => {
            err.to_string()
        }
        RepoGitError::Io(_) | RepoGitError::Database(_) | RepoGitError::Secret(_) => {
            "Push failed".to_string()
        }
    }
}
