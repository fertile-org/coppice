//! Retention for run folders under `<artifacts_dir>/runs/<run_id>/`.
//!
//! A folder is removed when its run's ticket is archived, when the run ended
//! more than `storage.run_artifacts_retention_days` ago, or when no run row
//! exists for it any more. Queued and running runs are never touched.

use sqlx::PgPool;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// A folder with no run row is left alone this long, in case its run row is
/// still being written.
const ORPHAN_GRACE: Duration = Duration::from_secs(60 * 60);
const BATCH: usize = 500;

pub fn spawn_sweep(state: Arc<crate::AppState>) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    tokio::spawn(async move {
        loop {
            let artifacts_dir = state.config.storage.artifacts_dir.clone();
            let days = state.config.storage.effective_run_artifacts_retention_days();
            match sweep(&pool, Path::new(&artifacts_dir), days).await {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "removed old run artifacts"),
                Err(err) => tracing::warn!(error = %err, "run artifact sweep failed"),
            }
            tokio::time::sleep(SWEEP_INTERVAL).await;
        }
    });
}

/// Remove run folders the retention rule no longer keeps. Returns how many.
pub async fn sweep(pool: &PgPool, artifacts_dir: &Path, retention_days: u32) -> anyhow::Result<usize> {
    let runs_dir = artifacts_dir.join("runs");
    let dirs = run_dirs(&runs_dir);
    let mut removed = 0;
    for chunk in dirs.chunks(BATCH) {
        let ids: Vec<Uuid> = chunk.iter().map(|(id, _)| *id).collect();
        let rows: Vec<(Uuid, bool)> = sqlx::query_as(
            r#"
            SELECT run.id,
                   run.status NOT IN ('queued', 'running')
                   AND (
                       ticket.archived_at IS NOT NULL
                       OR COALESCE(run.ended_at, run.created_at)
                          < now() - make_interval(days => $2)
                   ) AS expired
            FROM agent_runs run
            LEFT JOIN tickets ticket ON ticket.id = run.ticket_id
            WHERE run.id = ANY($1)
            "#,
        )
        .bind(&ids)
        .bind(i32::try_from(retention_days).unwrap_or(i32::MAX))
        .fetch_all(pool)
        .await?;
        for (id, path) in chunk {
            let remove = match rows.iter().find(|(run_id, _)| run_id == id) {
                Some((_, expired)) => *expired,
                None => older_than(path, ORPHAN_GRACE),
            };
            if remove && remove_dir(path) {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

/// Remove the folders of a ticket's finished runs, for example right after
/// the ticket is archived. Active runs keep theirs.
pub async fn remove_ticket_run_artifacts(
    pool: &PgPool,
    artifacts_dir: &Path,
    ticket_id: Uuid,
) -> anyhow::Result<usize> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM agent_runs WHERE ticket_id = $1 AND status NOT IN ('queued', 'running')",
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;
    let runs_dir = artifacts_dir.join("runs");
    Ok(ids
        .iter()
        .filter(|id| remove_dir(&runs_dir.join(id.to_string())))
        .count())
}

fn run_dirs(runs_dir: &Path) -> Vec<(Uuid, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(runs_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|entry| {
            let id = Uuid::parse_str(entry.file_name().to_str()?).ok()?;
            Some((id, entry.path()))
        })
        .collect()
}

fn older_than(path: &Path, age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|elapsed| elapsed >= age)
}

fn remove_dir(path: &Path) -> bool {
    match std::fs::remove_dir_all(path) {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "could not remove run artifacts");
            false
        }
    }
}
