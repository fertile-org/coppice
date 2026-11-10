//! Removes a ticket's worktree and local branch once the ticket is archived
//! or merged. Runs off the request path and is safe to repeat.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::git_state::GitOutcome;
use crate::events::bus::EventBus;
use crate::services::comment_service::CommentService;
use crate::services::git_state_service::GitStateService;
use crate::services::human_review_service::HumanReviewService;
use crate::services::ticket_git_service::{TicketGitService, WorktreeCleanup};
use crate::services::worktree_service::compute_paths;

/// Posted on the ticket when cleanup keeps a worktree with uncommitted work.
pub const KEPT_UNCOMMITTED_WORKTREE: &str =
    "Coppice kept this ticket's worktree because it has uncommitted changes.";

const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Clean up one ticket. `announce` posts a ticket comment when the worktree
/// is kept; the hourly sweep only logs.
pub async fn cleanup_ticket(
    pool: &PgPool,
    bus: &EventBus,
    worktrees_root: PathBuf,
    ticket_id: Uuid,
    announce: bool,
) -> WorktreeCleanup {
    let outcome = match TicketGitService::new(pool, worktrees_root)
        .cleanup_finished_worktree(ticket_id)
        .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::warn!(%ticket_id, error = %err, "ticket worktree cleanup failed");
            crate::events::publish_ticket_state(bus, pool, ticket_id).await;
            return WorktreeCleanup::NothingToDo;
        }
    };
    match &outcome {
        WorktreeCleanup::Removed => {
            tracing::info!(%ticket_id, "removed finished ticket worktree and branch");
            if let Err(err) = HumanReviewService::new(pool)
                .clear_git_outcome_if(ticket_id, GitOutcome::WorktreeKept)
                .await
            {
                tracing::warn!(%ticket_id, error = %err, "could not clear worktree cleanup outcome");
            }
        }
        WorktreeCleanup::SkippedUncommitted { worktree_path } => {
            tracing::warn!(%ticket_id, worktree_path, "kept ticket worktree with uncommitted changes");
            set_outcome(
                pool,
                ticket_id,
                Some(GitOutcome::WorktreeKept),
                Some("It has uncommitted changes"),
            )
            .await;
            if announce {
                if let Err(err) = CommentService::new(pool)
                    .create(
                        ticket_id,
                        AuthorType::System,
                        None,
                        KEPT_UNCOMMITTED_WORKTREE,
                        CommentIntent::SystemEvent,
                        &[],
                        &[],
                    )
                    .await
                {
                    tracing::warn!(%ticket_id, error = %err, "could not post worktree cleanup note");
                }
            }
        }
        WorktreeCleanup::SkippedActiveRun => {
            tracing::info!(%ticket_id, "kept ticket worktree: an agent run is active");
        }
        WorktreeCleanup::NothingToDo => {}
    }
    // Always: the panel shows the git state, which this may have changed.
    crate::events::publish_ticket_state(bus, pool, ticket_id).await;
    outcome
}

async fn set_outcome(
    pool: &PgPool,
    ticket_id: Uuid,
    outcome: Option<GitOutcome>,
    reason: Option<&str>,
) {
    if let Err(err) =
        GitStateService::set_outcome_and_notify(pool, ticket_id, outcome, reason).await
    {
        tracing::warn!(%ticket_id, error = %err, "could not record worktree cleanup outcome");
    }
}

/// Clean up in the background, for example after archive or merge.
pub fn spawn_cleanup(state: &Arc<crate::AppState>, ticket_id: Uuid) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    let root = PathBuf::from(&state.config.agent.worktrees_path);
    let state = state.clone();
    tokio::spawn(async move {
        cleanup_ticket(&pool, &state.event_bus, root, ticket_id, true).await;
    });
}

/// Hourly: archived or merged tickets whose worktree is still on disk.
pub fn spawn_sweep(state: Arc<crate::AppState>) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_INTERVAL).await;
            let root = PathBuf::from(&state.config.agent.worktrees_path);
            if let Err(err) = sweep(&pool, &state.event_bus, root).await {
                tracing::warn!(error = %err, "ticket worktree sweep failed");
            }
            if let Err(err) = recheck_pending(&state, &pool).await {
                tracing::warn!(error = %err, "ticket git state recheck failed");
            }
        }
    });
}

pub async fn sweep(
    pool: &PgPool,
    bus: &EventBus,
    worktrees_root: PathBuf,
) -> Result<usize, sqlx::Error> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT ticket.id, repo.name
        FROM tickets ticket
        JOIN repos repo ON repo.id = ticket.repo_id
        LEFT JOIN ticket_human_reviews review ON review.ticket_id = ticket.id
        WHERE ticket.archived_at IS NOT NULL OR review.merged_at IS NOT NULL
        "#,
    )
    .fetch_all(pool)
    .await?;
    let mut removed = 0;
    for (ticket_id, repo_name) in rows {
        if !compute_paths(&worktrees_root, &repo_name, ticket_id)
            .worktree_dir
            .exists()
        {
            continue;
        }
        if cleanup_ticket(pool, bus, worktrees_root.clone(), ticket_id, false).await
            == WorktreeCleanup::Removed
        {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Tickets waiting on a push or a kept worktree may have been resolved by
/// hand since. Local git only.
async fn recheck_pending(state: &crate::AppState, pool: &PgPool) -> Result<(), sqlx::Error> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT ticket_id FROM ticket_human_reviews WHERE git_outcome IN ('not_pushed', 'worktree_kept')",
    )
    .fetch_all(pool)
    .await?;
    let service = GitStateService::new(state, pool);
    for ticket_id in ids {
        if let Err(err) = service.recheck(ticket_id).await {
            tracing::warn!(%ticket_id, error = %err, "git state recheck failed");
        }
    }
    Ok(())
}
