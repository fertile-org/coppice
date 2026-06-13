use std::path::{Path, PathBuf};

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::services::ticket_service::{TicketError, TicketService};
use crate::services::worktree_service::{
    compute_paths, finalize_worktree_git, sync_worktree_to_branch_tip, WorktreeError,
};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketGitInfo {
    pub ticket_branch: String,
    pub worktree_path: String,
    pub worktree_exists: bool,
    pub default_branch: String,
    pub branches: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeBranchResult {
    pub base_branch: String,
    pub ticket_branch: String,
    pub head_sha: String,
    pub message: String,
}

pub struct TicketGitContext {
    pub git_dir: PathBuf,
    pub worktree_dir: PathBuf,
    pub ticket_branch: String,
    pub default_branch: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TicketGitError {
    #[error("ticket not found")]
    TicketNotFound,
    #[error("ticket has no linked repository")]
    NoRepo,
    #[error("repository not found")]
    RepoNotFound,
    #[error("repository is not ready")]
    RepoNotReady,
    #[error("ticket branch not found: {0}")]
    TicketBranchMissing(String),
    #[error("worktree already removed")]
    WorktreeAlreadyRemoved,
    #[error("invalid branch name")]
    InvalidBranchName,
    #[error("git error: {0}")]
    Git(String),
    #[error(transparent)]
    Ticket(#[from] TicketError),
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub struct TicketGitService<'a> {
    pool: &'a PgPool,
    worktrees_root: PathBuf,
}

impl<'a> TicketGitService<'a> {
    pub fn new(pool: &'a PgPool, worktrees_root: PathBuf) -> Self {
        Self {
            pool,
            worktrees_root,
        }
    }

    pub async fn resolve_context(&self, ticket_id: Uuid) -> Result<TicketGitContext, TicketGitError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        let repo_id = ticket.ticket.repo_id.ok_or(TicketGitError::NoRepo)?;

        let row = sqlx::query(
            r#"
            SELECT local_path, name, default_branch, verification_status
            FROM repos
            WHERE id = $1
            "#,
        )
        .bind(repo_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketGitError::RepoNotFound)?;

        let verification_status: String = row.get("verification_status");
        if verification_status != "ready" {
            return Err(TicketGitError::RepoNotReady);
        }

        let local_path: String = row.get("local_path");
        let repo_name: String = row.get("name");
        let default_branch: String = row.get("default_branch");

        let paths = compute_paths(&self.worktrees_root, &repo_name, ticket_id);

        Ok(TicketGitContext {
            git_dir: PathBuf::from(local_path),
            worktree_dir: paths.worktree_dir,
            ticket_branch: paths.branch_name,
            default_branch,
        })
    }

    pub async fn git_info(&self, ticket_id: Uuid) -> Result<TicketGitInfo, TicketGitError> {
        let ctx = self.resolve_context(ticket_id).await?;
        let branches = list_local_branches(&ctx.git_dir).await?;
        Ok(TicketGitInfo {
            ticket_branch: ctx.ticket_branch.clone(),
            worktree_path: ctx.worktree_dir.to_string_lossy().into_owned(),
            worktree_exists: worktree_exists(&ctx.worktree_dir),
            default_branch: ctx.default_branch,
            branches,
        })
    }

    pub async fn merge_ticket_branch(
        &self,
        ticket_id: Uuid,
        base_branch: &str,
    ) -> Result<MergeBranchResult, TicketGitError> {
        validate_branch_name(base_branch)?;
        let ctx = self.resolve_context(ticket_id).await?;

        if worktree_exists(&ctx.worktree_dir) {
            sync_worktree_to_branch_tip(
                &ctx.git_dir,
                &ctx.worktree_dir,
                &ctx.ticket_branch,
            )
            .await?;
            let _ = finalize_worktree_git(
                &ctx.worktree_dir,
                &ctx.ticket_branch,
                "[coppice] pre-merge checkpoint",
            )
            .await;
        }

        if git_ref_exists(&ctx.git_dir, &ctx.ticket_branch).await? {
            // branch exists
        } else {
            return Err(TicketGitError::TicketBranchMissing(ctx.ticket_branch));
        }

        if !git_status_clean(&ctx.git_dir).await? {
            return Err(TicketGitError::Git(
                "main repository has uncommitted changes — commit or stash before merging"
                    .into(),
            ));
        }

        run_git(&ctx.git_dir, &["checkout", base_branch]).await?;

        let merge_msg = format!(
            "Merge {} into {} (Coppice ticket {})",
            ctx.ticket_branch, base_branch, ticket_id
        );
        let output = run_git_capture(
            &ctx.git_dir,
            &["merge", &ctx.ticket_branch, "-m", &merge_msg],
        )
        .await;

        let message = match output {
            Ok(stdout) => {
                if stdout.contains("Already up to date") {
                    "Already up to date".to_string()
                } else {
                    format!("Merged `{}` into `{}`", ctx.ticket_branch, base_branch)
                }
            }
            Err(msg) => {
                let _ = run_git(&ctx.git_dir, &["merge", "--abort"]).await;
                return Err(TicketGitError::Git(msg));
            }
        };

        let head_sha = git_head_sha(&ctx.git_dir).await?;

        Ok(MergeBranchResult {
            base_branch: base_branch.to_string(),
            ticket_branch: ctx.ticket_branch,
            head_sha,
            message,
        })
    }

    pub async fn remove_worktree(&self, ticket_id: Uuid) -> Result<(), TicketGitError> {
        let ctx = self.resolve_context(ticket_id).await?;
        if !worktree_exists(&ctx.worktree_dir) {
            return Err(TicketGitError::WorktreeAlreadyRemoved);
        }

        let path = path_to_string(&ctx.worktree_dir)?;
        run_git(&ctx.git_dir, &["worktree", "remove", "--force", &path]).await?;
        let _ = run_git(&ctx.git_dir, &["worktree", "prune"]).await;

        if ctx.worktree_dir.exists() {
            tokio::fs::remove_dir_all(&ctx.worktree_dir).await?;
        }

        Ok(())
    }
}

pub fn worktree_exists(worktree_dir: &Path) -> bool {
    worktree_dir.join(".git").exists()
}

fn validate_branch_name(branch: &str) -> Result<(), TicketGitError> {
    let trimmed = branch.trim();
    if trimmed.is_empty()
        || trimmed.len() > 200
        || trimmed.contains("..")
        || trimmed.starts_with('-')
    {
        return Err(TicketGitError::InvalidBranchName);
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
    {
        return Err(TicketGitError::InvalidBranchName);
    }
    Ok(())
}

fn path_to_string(path: &Path) -> Result<String, TicketGitError> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| TicketGitError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("path is not valid UTF-8: {}", path.display()),
        )))
}

async fn list_local_branches(git_dir: &Path) -> Result<Vec<String>, TicketGitError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["branch", "--format=%(refname:short)"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(TicketGitError::Git(git_stderr(&output)));
    }

    let mut branches: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    branches.sort_unstable();
    branches.dedup();
    Ok(branches)
}

async fn git_ref_exists(git_dir: &Path, ref_name: &str) -> Result<bool, TicketGitError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["rev-parse", "--verify", ref_name])
        .output()
        .await?;
    Ok(output.status.success())
}

async fn git_status_clean(git_dir: &Path) -> Result<bool, TicketGitError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["status", "--porcelain"])
        .output()
        .await?;
    if !output.status.success() {
        return Err(TicketGitError::Git(git_stderr(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

async fn git_head_sha(git_dir: &Path) -> Result<String, TicketGitError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .await?;
    if !output.status.success() {
        return Err(TicketGitError::Git(git_stderr(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn run_git(git_dir: &Path, args: &[&str]) -> Result<(), TicketGitError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(args)
        .output()
        .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(TicketGitError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            git_stderr(&output)
        )))
    }
}

async fn run_git_capture(git_dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(args)
        .output()
        .await
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(combine_git_output(&output))
    } else {
        Err(combine_git_output(&output))
    }
}

fn git_stderr(output: &std::process::Output) -> String {
    combine_git_output(output)
}

fn combine_git_output(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match (stderr.is_empty(), stdout.is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stderr,
        (true, false) => stdout,
        (true, true) => format!("exit code {}", output.status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_branch_name_accepts_common_names() {
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("agent/TICKET-abc").is_ok());
        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("bad branch").is_err());
    }

    #[test]
    fn worktree_exists_checks_git_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(!worktree_exists(tmp.path()));
        std::fs::write(tmp.path().join(".git"), "gitdir: /path").expect("write");
        assert!(worktree_exists(tmp.path()));
    }
}
