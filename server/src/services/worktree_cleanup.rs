//! Removes a ticket's worktree and local branch once the ticket is archived
//! or merged. Runs off the request path and is safe to repeat.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::comment::{AuthorType, CommentIntent};
use crate::services::comment_service::CommentService;
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
            return WorktreeCleanup::NothingToDo;
        }
    };
    match &outcome {
        WorktreeCleanup::Removed => {
            tracing::info!(%ticket_id, "removed finished ticket worktree and branch");
        }
        WorktreeCleanup::SkippedUncommitted { worktree_path } => {
            tracing::warn!(%ticket_id, worktree_path, "kept ticket worktree with uncommitted changes");
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
    outcome
}

/// Clean up in the background, for example after archive or merge.
pub fn spawn_cleanup(state: &Arc<crate::AppState>, ticket_id: Uuid) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    let root = PathBuf::from(&state.config.agent.worktrees_path);
    tokio::spawn(async move {
        cleanup_ticket(&pool, root, ticket_id, true).await;
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
            if let Err(err) = sweep(&pool, root).await {
                tracing::warn!(error = %err, "ticket worktree sweep failed");
            }
        }
    });
}

pub async fn sweep(pool: &PgPool, worktrees_root: PathBuf) -> Result<usize, sqlx::Error> {
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
        if cleanup_ticket(pool, worktrees_root.clone(), ticket_id, false).await
            == WorktreeCleanup::Removed
        {
            removed += 1;
        }
    }
    Ok(removed)
}
