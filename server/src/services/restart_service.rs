//! A crashed or restarted run waits for a person. The worktree stays until
//! they resume, discard the changed files, or cancel.

use std::path::Path;

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::copy::restart::{
    comment_for, confirm_discard, ACTION_CANCEL, ACTION_DISCARD, ACTION_RESUME,
};
use crate::domain::substatus::{Substatus, TicketStatus};
use crate::domain::ticket::{status_from_str, status_to_str};
use crate::services::run_service::{RunError, RunService};
use crate::services::ticket_service::{TicketError, TicketService};

#[derive(Debug, thiserror::Error)]
pub enum RestartError {
    #[error("nothing is waiting after a restart")]
    NothingPending,
    #[error("discard is for a worktree with changes")]
    UseDiscard,
    #[error("cancel is for a worktree with no changes")]
    UseCancel,
    #[error(transparent)]
    Run(#[from] RunError),
    #[error(transparent)]
    Ticket(#[from] TicketError),
    #[error("{0}")]
    Git(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub struct RestartChoice {
    pub run_id: Uuid,
    pub changed_files: i32,
    pub previous_status: TicketStatus,
    pub resume_label: &'static str,
    pub other_label: &'static str,
    pub confirm: Option<String>,
}

pub struct RestartService<'a> {
    pool: &'a PgPool,
}

impl<'a> RestartService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn record(
        &self,
        run_id: Uuid,
        ticket_id: Uuid,
        changed_files: u32,
        previous_status: TicketStatus,
    ) -> Result<(), RestartError> {
        sqlx::query(
            r#"
            INSERT INTO run_restart_decisions (
                run_id, ticket_id, changed_files, previous_status
            )
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (run_id) DO NOTHING
            "#,
        )
        .bind(run_id)
        .bind(ticket_id)
        .bind(i32::try_from(changed_files).unwrap_or(i32::MAX))
        .bind(status_to_str(previous_status))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn open(&self, ticket_id: Uuid) -> Result<Option<RestartChoice>, RestartError> {
        let row = sqlx::query(
            r#"
            SELECT run_id, changed_files, previous_status
            FROM run_restart_decisions
            WHERE ticket_id = $1 AND resolved_at IS NULL
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let changed_files: i32 = row.get("changed_files");
        let previous_status: String = row.get("previous_status");
        let previous_status = status_from_str(&previous_status).unwrap_or(TicketStatus::Blocked);
        let changed = u32::try_from(changed_files).unwrap_or(0);
        let (other_label, confirm) = if changed == 0 {
            (ACTION_CANCEL, None)
        } else {
            (ACTION_DISCARD, Some(confirm_discard(changed)))
        };
        Ok(Some(RestartChoice {
            run_id: row.get("run_id"),
            changed_files,
            previous_status,
            resume_label: ACTION_RESUME,
            other_label,
            confirm,
        }))
    }

    pub async fn resume(
        &self,
        ticket_id: Uuid,
    ) -> Result<crate::domain::run::AgentRun, RestartError> {
        let choice = self.require_open(ticket_id).await?;
        let tickets = TicketService::new(self.pool);
        let restored = choice.previous_status != TicketStatus::Blocked;
        if restored {
            tickets
                .restore_interrupted_column(ticket_id, choice.previous_status)
                .await?;
        }
        let started = RunService::new(self.pool).start_run(ticket_id).await;
        let run = match started {
            Ok(run) => run,
            Err(err) => {
                if restored {
                    if let Err(rollback_err) = tickets
                        .update_status(
                            ticket_id,
                            TicketStatus::Blocked,
                            Some(Some(Substatus::BlockedByError)),
                            Some(None),
                        )
                        .await
                    {
                        tracing::warn!(
                            error = %rollback_err,
                            %ticket_id,
                            "failed to put the ticket back in Blocked after resume failed"
                        );
                    }
                }
                return Err(err.into());
            }
        };
        self.resolve(choice.run_id).await?;
        Ok(run)
    }

    pub async fn discard(&self, ticket_id: Uuid) -> Result<(), RestartError> {
        let choice = self.require_open(ticket_id).await?;
        if choice.changed_files == 0 {
            return Err(RestartError::UseCancel);
        }
        if let Some(path) = self.worktree_path(choice.run_id).await? {
            let start = self
                .start_sha(choice.run_id)
                .await?
                .filter(|sha| is_commit_sha(sha));
            let Some(start) = start else {
                return Err(RestartError::Git(
                    "run has no commit to discard back to".into(),
                ));
            };
            discard_to_start(Path::new(&path), &start).await?;
        }
        self.resolve(choice.run_id).await?;
        Ok(())
    }

    pub async fn cancel(&self, ticket_id: Uuid) -> Result<(), RestartError> {
        let choice = self.require_open(ticket_id).await?;
        if choice.changed_files > 0 {
            return Err(RestartError::UseDiscard);
        }
        if let Some(path) = self.worktree_path(choice.run_id).await? {
            remove_linked_worktree(Path::new(&path)).await?;
        }
        self.resolve(choice.run_id).await?;
        Ok(())
    }

    async fn require_open(&self, ticket_id: Uuid) -> Result<RestartChoice, RestartError> {
        self.open(ticket_id)
            .await?
            .ok_or(RestartError::NothingPending)
    }

    async fn resolve(&self, run_id: Uuid) -> Result<(), RestartError> {
        sqlx::query(
            r#"
            UPDATE run_restart_decisions
            SET resolved_at = now()
            WHERE run_id = $1 AND resolved_at IS NULL
            "#,
        )
        .bind(run_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    async fn worktree_path(&self, run_id: Uuid) -> Result<Option<String>, RestartError> {
        Ok(RunService::new(self.pool).get(run_id).await?.worktree_path)
    }

    async fn start_sha(&self, run_id: Uuid) -> Result<Option<String>, RestartError> {
        let sha = sqlx::query_scalar::<_, Option<String>>(
            "SELECT start_sha FROM agent_runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(sha.flatten())
    }
}

/// Files the run changed relative to the commit it started from.
/// A missing start commit or an unreadable worktree counts as zero so the run still waits.
pub async fn changed_file_count(pool: &PgPool, run: &crate::domain::run::AgentRun) -> u32 {
    let sha = sqlx::query_scalar::<_, Option<String>>(
        "SELECT start_sha FROM agent_runs WHERE id = $1",
    )
    .bind(run.id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten();
    count_changed_files(
        run.worktree_path.as_deref().map(Path::new),
        sha.as_deref(),
    )
    .await
}

/// Tracked edits, deletions, later commits, and untracked files that are not
/// ignored and not under `.agent/`. Compared with the commit the run started from.
pub async fn count_changed_files(worktree: Option<&Path>, start_sha: Option<&str>) -> u32 {
    let (Some(worktree), Some(start_sha)) = (worktree, start_sha) else {
        return 0;
    };
    if !worktree.exists() || !is_commit_sha(start_sha) {
        return 0;
    }
    let Some(tracked) = git_name_list(
        worktree,
        &["diff", "--name-only", start_sha, "--", ".", ":!.agent"],
    )
    .await
    else {
        return 0;
    };
    let Some(untracked) = git_name_list(
        worktree,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--",
            ".",
            ":!.agent",
        ],
    )
    .await
    else {
        return 0;
    };
    let mut names = std::collections::HashSet::new();
    for line in tracked.lines().chain(untracked.lines()) {
        let line = line.trim();
        if !line.is_empty() {
            names.insert(line.to_string());
        }
    }
    u32::try_from(names.len()).unwrap_or(u32::MAX)
}

fn is_commit_sha(sha: &str) -> bool {
    (7..=64).contains(&sha.len()) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn restart_comment(changed_files: u32) -> String {
    comment_for(changed_files)
}

async fn discard_to_start(worktree: &Path, start_sha: &str) -> Result<(), RestartError> {
    if !worktree.exists() {
        return Ok(());
    }
    git_ok(worktree, &["reset", "--hard", start_sha]).await?;
    // Untracked files only. `-fd` leaves ignored files. `.agent/` stays.
    git_ok(worktree, &["clean", "-fd", "--", ".", ":!.agent"]).await?;
    Ok(())
}

async fn git_name_list(worktree: &Path, args: &[&str]) -> Option<String> {
    match crate::services::git_cmd::output(Some(worktree), args).await {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) => {
            tracing::warn!(
                stderr = %String::from_utf8_lossy(&output.stderr),
                path = %worktree.display(),
                "could not list worktree changes after restart"
            );
            None
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                path = %worktree.display(),
                "could not list worktree changes after restart"
            );
            None
        }
    }
}

async fn remove_linked_worktree(worktree: &Path) -> Result<(), RestartError> {
    if !worktree.exists() {
        return Ok(());
    }
    if !worktree.join(".git").is_file() {
        return Ok(());
    }
    let common = git_stdout(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await?;
    let common = Path::new(common.trim());
    let path = path_string(worktree)?;
    git_ok(common, &["worktree", "remove", "--force", &path]).await?;
    if let Err(err) = git_ok(common, &["worktree", "prune"]).await {
        tracing::debug!(error = %err, "worktree prune after cancel");
    }
    if worktree.exists() {
        tokio::fs::remove_dir_all(worktree)
            .await
            .map_err(|err| RestartError::Git(err.to_string()))?;
    }
    Ok(())
}

async fn git_ok(cwd: &Path, args: &[&str]) -> Result<(), RestartError> {
    let output = crate::services::git_cmd::output(Some(cwd), args)
        .await
        .map_err(|err| RestartError::Git(err.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RestartError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

async fn git_stdout(cwd: &Path, args: &[&str]) -> Result<String, RestartError> {
    let output = crate::services::git_cmd::output(Some(cwd), args)
        .await
        .map_err(|err| RestartError::Git(err.to_string()))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(RestartError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

fn path_string(path: &Path) -> Result<String, RestartError> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| RestartError::Git("worktree path is not utf-8".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    fn head(dir: &Path) -> String {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .expect("rev-parse");
        assert!(output.status.success(), "rev-parse failed");
        String::from_utf8(output.stdout)
            .expect("utf-8")
            .trim()
            .to_string()
    }

    fn worktree_with_changes(dirty: bool) -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().expect("temp");
        let main = root.path().join("main");
        let wt = root.path().join("wt");
        std::fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-b", "main"]);
        std::fs::write(main.join("README.md"), "hello\n").unwrap();
        std::fs::write(main.join("tracked.txt"), "keep\n").unwrap();
        git(&main, &["add", "README.md", "tracked.txt"]);
        git(&main, &["commit", "-m", "init"]);
        git(
            &main,
            &[
                "worktree",
                "add",
                "-b",
                "agent/ticket",
                wt.to_str().unwrap(),
            ],
        );
        std::fs::create_dir_all(wt.join(".agent")).unwrap();
        std::fs::write(wt.join(".agent").join("context.md"), "runtime\n").unwrap();
        if dirty {
            std::fs::write(wt.join("README.md"), "changed\n").unwrap();
            std::fs::write(wt.join("notes.txt"), "new\n").unwrap();
            std::fs::remove_file(wt.join("tracked.txt")).unwrap();
        }
        (root, wt)
    }

    #[tokio::test]
    async fn count_ignores_agent_context_and_keeps_real_files() {
        let (_root, wt) = worktree_with_changes(true);
        let start = head(&wt);
        assert_eq!(count_changed_files(Some(&wt), Some(&start)).await, 3);
    }

    #[tokio::test]
    async fn count_is_zero_when_only_agent_context_changed() {
        let (_root, wt) = worktree_with_changes(false);
        let start = head(&wt);
        assert_eq!(count_changed_files(Some(&wt), Some(&start)).await, 0);
        assert_eq!(count_changed_files(None, Some(&start)).await, 0);
        assert_eq!(count_changed_files(Some(&wt), None).await, 0);
    }

    #[tokio::test]
    async fn discard_removes_counted_files_and_keeps_agent_context() {
        let (_root, wt) = worktree_with_changes(true);
        let start = head(&wt);
        discard_to_start(&wt, &start).await.expect("discard");
        assert_eq!(
            std::fs::read_to_string(wt.join("README.md")).unwrap(),
            "hello\n"
        );
        assert!(!wt.join("notes.txt").exists());
        assert_eq!(
            std::fs::read_to_string(wt.join("tracked.txt")).unwrap(),
            "keep\n"
        );
        assert!(wt.join(".agent").join("context.md").exists());
        assert_eq!(count_changed_files(Some(&wt), Some(&start)).await, 0);
    }

    #[tokio::test]
    async fn untracked_file_is_counted_and_discard_removes_it() {
        let (_root, wt) = worktree_with_changes(false);
        std::fs::write(wt.join(".gitignore"), "*.log\n").unwrap();
        git(&wt, &["add", ".gitignore"]);
        git(&wt, &["commit", "-m", "ignore logs"]);
        let start = head(&wt);
        std::fs::write(wt.join("notes.txt"), "new\n").unwrap();
        std::fs::write(wt.join("secret.log"), "hidden\n").unwrap();
        assert_eq!(count_changed_files(Some(&wt), Some(&start)).await, 1);
        discard_to_start(&wt, &start).await.expect("discard");
        assert!(!wt.join("notes.txt").exists());
        assert!(wt.join("secret.log").exists());
        assert!(wt.join(".agent").join("context.md").exists());
        assert_eq!(head(&wt), start);
    }

    #[tokio::test]
    async fn committed_change_is_counted_and_discard_returns_to_start() {
        let (_root, wt) = worktree_with_changes(false);
        let start = head(&wt);
        std::fs::write(wt.join("README.md"), "during the run\n").unwrap();
        git(&wt, &["add", "README.md"]);
        git(&wt, &["commit", "-m", "before the crash"]);
        assert_ne!(head(&wt), start);
        assert_eq!(count_changed_files(Some(&wt), Some(&start)).await, 1);
        discard_to_start(&wt, &start).await.expect("discard");
        assert_eq!(head(&wt), start);
        assert_eq!(
            std::fs::read_to_string(wt.join("README.md")).unwrap(),
            "hello\n"
        );
        assert!(wt.join(".agent").join("context.md").exists());
    }

    #[tokio::test]
    async fn cancel_removes_a_clean_linked_worktree() {
        let (_root, wt) = worktree_with_changes(false);
        remove_linked_worktree(&wt).await.expect("remove");
        assert!(!wt.exists());
    }
}
