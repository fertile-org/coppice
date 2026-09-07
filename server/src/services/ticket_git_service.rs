use std::path::{Path, PathBuf};

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::crypto::SecretStore;
use crate::services::git_ops::{
    auth_https_remote, git_head_sha, git_ref_exists, git_status_clean, list_local_branches,
    push_argv, push_gate, push_refspec, run_git, run_git_capture, sanitize_token, GitOpsError,
};
use crate::services::pr_create_url::{
    build_pr_create_url, github_owner_repo, https_remote_url,
};
use crate::services::secret_service::SecretService;
use crate::services::ticket_service::{TicketError, TicketService};
use crate::services::worktree_service::{
    compute_paths, finalize_worktree_git, sync_worktree_to_branch_tip, GitAuthor, WorktreeError,
};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketGitInfo {
    pub ticket_branch: String,
    pub worktree_path: String,
    pub worktree_exists: bool,
    pub default_branch: String,
    pub branches: Vec<String>,
    pub remote_url: Option<String>,
    pub pr_create_url: Option<String>,
    pub pr_url: Option<String>,
    pub forge_token_configured: bool,
    pub push_enabled: bool,
    pub can_push: bool,
    pub can_create_pr: bool,
    pub push_disabled_reason: Option<String>,
    pub create_pr_disabled_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeBranchResult {
    pub base_branch: String,
    pub ticket_branch: String,
    pub head_sha: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseBranchResult {
    pub base_branch: String,
    pub onto_ref: String,
    pub ticket_branch: String,
    pub head_sha: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushBranchResult {
    pub ticket_branch: String,
    pub remote: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrResult {
    pub pr_url: String,
    pub number: i64,
    pub title: String,
}

pub struct TicketGitContext {
    pub git_dir: PathBuf,
    pub worktree_dir: PathBuf,
    pub ticket_branch: String,
    pub default_branch: String,
    pub remote_url: Option<String>,
    pub forge_token_secret_id: Option<Uuid>,
    pub pr_url: Option<String>,
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
    #[error("ticket worktree does not exist")]
    WorktreeMissing,
    #[error("rebase conflict: {detail}")]
    RebaseConflict {
        paths: Vec<String>,
        detail: String,
    },
    #[error("invalid branch name")]
    InvalidBranchName,
    #[error("git push is disabled (set git.push_enabled = true)")]
    PushDisabled,
    #[error("repository has no remote_url")]
    NoRemoteUrl,
    #[error("repository has no forge token — set one in Settings → Repositories")]
    NoForgeToken,
    #[error("remote is not a GitHub repository")]
    NotGitHub,
    #[error("git error: {0}")]
    Git(String),
    #[error("github api error: {0}")]
    GitHubApi(String),
    #[error(transparent)]
    Ticket(#[from] TicketError),
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Secret(#[from] crate::services::secret_service::SecretError),
}

impl From<GitOpsError> for TicketGitError {
    fn from(err: GitOpsError) -> Self {
        match err {
            GitOpsError::Git(msg) => TicketGitError::Git(msg),
            GitOpsError::Io(err) => TicketGitError::Io(err),
        }
    }
}

pub struct TicketGitService<'a> {
    pool: &'a PgPool,
    worktrees_root: PathBuf,
    push_enabled: bool,
    secret_store: Option<&'a SecretStore>,
    git_author: Option<GitAuthor>,
}

impl<'a> TicketGitService<'a> {
    pub fn new(pool: &'a PgPool, worktrees_root: PathBuf) -> Self {
        Self {
            pool,
            worktrees_root,
            push_enabled: false,
            secret_store: None,
            git_author: None,
        }
    }

    pub fn with_forge(
        pool: &'a PgPool,
        worktrees_root: PathBuf,
        push_enabled: bool,
        secret_store: &'a SecretStore,
        git_author: GitAuthor,
    ) -> Self {
        Self {
            pool,
            worktrees_root,
            push_enabled,
            secret_store: Some(secret_store),
            git_author: Some(git_author),
        }
    }

    pub async fn resolve_context(&self, ticket_id: Uuid) -> Result<TicketGitContext, TicketGitError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        let repo_id = ticket.ticket.repo_id.ok_or(TicketGitError::NoRepo)?;

        let row = sqlx::query(
            r#"
            SELECT local_path, name, default_branch, verification_status, remote_url,
                   forge_token_secret_id
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
        let remote_url: Option<String> = row.get("remote_url");
        let forge_token_secret_id: Option<Uuid> = row.get("forge_token_secret_id");

        let pr_url: Option<String> =
            sqlx::query_scalar("SELECT pr_url FROM tickets WHERE id = $1")
                .bind(ticket_id)
                .fetch_one(self.pool)
                .await?;

        let paths = compute_paths(&self.worktrees_root, &repo_name, ticket_id);

        Ok(TicketGitContext {
            git_dir: PathBuf::from(local_path),
            worktree_dir: paths.worktree_dir,
            ticket_branch: paths.branch_name,
            default_branch,
            remote_url,
            forge_token_secret_id,
            pr_url,
        })
    }

    pub async fn git_info(&self, ticket_id: Uuid) -> Result<TicketGitInfo, TicketGitError> {
        let ctx = self.resolve_context(ticket_id).await?;
        let branches = list_local_branches(&ctx.git_dir).await?;
        let pr_create_url = build_pr_create_url(
            ctx.remote_url.as_deref(),
            &ctx.default_branch,
            &ctx.ticket_branch,
        );
        let forge_token_configured = ctx.forge_token_secret_id.is_some();
        let (can_push, push_disabled_reason) = push_gate(
            self.push_enabled,
            ctx.remote_url.as_deref(),
            forge_token_configured,
        );
        let (can_create_pr, create_pr_disabled_reason) = create_pr_gate(
            self.push_enabled,
            ctx.remote_url.as_deref(),
            forge_token_configured,
        );

        Ok(TicketGitInfo {
            ticket_branch: ctx.ticket_branch.clone(),
            worktree_path: ctx.worktree_dir.to_string_lossy().into_owned(),
            worktree_exists: worktree_exists(&ctx.worktree_dir),
            default_branch: ctx.default_branch,
            branches,
            remote_url: ctx.remote_url,
            pr_create_url,
            pr_url: ctx.pr_url,
            forge_token_configured,
            push_enabled: self.push_enabled,
            can_push,
            can_create_pr,
            push_disabled_reason,
            create_pr_disabled_reason,
        })
    }

    pub async fn push_branch(&self, ticket_id: Uuid) -> Result<PushBranchResult, TicketGitError> {
        if !self.push_enabled {
            return Err(TicketGitError::PushDisabled);
        }
        let store = self.secret_store.ok_or(TicketGitError::NoForgeToken)?;
        let ctx = self.resolve_context(ticket_id).await?;
        let remote_url = ctx.remote_url.as_deref().ok_or(TicketGitError::NoRemoteUrl)?;
        let secret_id = ctx
            .forge_token_secret_id
            .ok_or(TicketGitError::NoForgeToken)?;
        let token = SecretService::new(self.pool, store)
            .decrypt_by_id(secret_id)
            .await?;
        let https = https_remote_url(remote_url).ok_or(TicketGitError::NoRemoteUrl)?;

        if !git_ref_exists(&ctx.git_dir, &ctx.ticket_branch).await? {
            return Err(TicketGitError::TicketBranchMissing(ctx.ticket_branch));
        }

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
                "[coppice] pre-push checkpoint",
                self.git_author.as_ref(),
            )
            .await;
        }

        let auth_remote = auth_https_remote(remote_url, token.trim())?;

        let cwd = if worktree_exists(&ctx.worktree_dir) {
            ctx.worktree_dir.as_path()
        } else {
            ctx.git_dir.as_path()
        };

        let refspec = push_refspec(&ctx.ticket_branch);
        let args = push_argv(&auth_remote, &refspec);
        match run_git(cwd, &args).await {
            Ok(()) => {}
            Err(GitOpsError::Git(msg)) => {
                return Err(TicketGitError::Git(sanitize_token(&msg, token.trim())));
            }
            Err(other) => return Err(other.into()),
        }

        Ok(PushBranchResult {
            ticket_branch: ctx.ticket_branch,
            remote: https,
            message: "Branch pushed".into(),
        })
    }

    pub async fn create_pr(
        &self,
        ticket_id: Uuid,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<CreatePrResult, TicketGitError> {
        if !self.push_enabled {
            return Err(TicketGitError::PushDisabled);
        }
        let store = self.secret_store.ok_or(TicketGitError::NoForgeToken)?;
        let ctx = self.resolve_context(ticket_id).await?;
        let remote_url = ctx.remote_url.as_deref().ok_or(TicketGitError::NoRemoteUrl)?;
        let secret_id = ctx
            .forge_token_secret_id
            .ok_or(TicketGitError::NoForgeToken)?;
        let (owner, repo) = github_owner_repo(remote_url).ok_or(TicketGitError::NotGitHub)?;
        let token = SecretService::new(self.pool, store)
            .decrypt_by_id(secret_id)
            .await?;

        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        let pr_title = title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(ticket.ticket.title.as_str())
            .to_string();
        let pr_body = body.unwrap_or("").to_string();

        let client = reqwest::Client::new();
        let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls");
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token.trim()))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "coppice")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&serde_json::json!({
                "title": pr_title,
                "head": ctx.ticket_branch,
                "base": ctx.default_branch,
                "body": pr_body,
            }))
            .send()
            .await
            .map_err(|e| TicketGitError::GitHubApi(e.to_string()))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| TicketGitError::GitHubApi(e.to_string()))?;
        if !status.is_success() {
            return Err(TicketGitError::GitHubApi(format!(
                "{status}: {}",
                truncate_err(&text)
            )));
        }

        let parsed: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| TicketGitError::GitHubApi(e.to_string()))?;
        let pr_url = parsed["html_url"]
            .as_str()
            .ok_or_else(|| TicketGitError::GitHubApi("missing html_url".into()))?
            .to_string();
        let number = parsed["number"].as_i64().unwrap_or(0);

        sqlx::query("UPDATE tickets SET pr_url = $2, updated_at = now() WHERE id = $1")
            .bind(ticket_id)
            .bind(&pr_url)
            .execute(self.pool)
            .await?;

        Ok(CreatePrResult {
            pr_url,
            number,
            title: pr_title,
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
                self.git_author.as_ref(),
            )
            .await;
        }

        if !git_ref_exists(&ctx.git_dir, &ctx.ticket_branch).await? {
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

    /// Rebase the ticket branch onto `base_branch` (or the repo default) **in the
    /// ticket worktree**. Does not auto-commit; dirty trees fail hard. Fetch is
    /// best-effort and never blocks a local rebase.
    pub async fn rebase_ticket_branch(
        &self,
        ticket_id: Uuid,
        base_branch: Option<&str>,
    ) -> Result<RebaseBranchResult, TicketGitError> {
        let ctx = self.resolve_context(ticket_id).await?;
        let base = base_branch
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(ctx.default_branch.as_str());
        validate_branch_name(base)?;

        if !git_ref_exists(&ctx.git_dir, &ctx.ticket_branch).await? {
            return Err(TicketGitError::TicketBranchMissing(ctx.ticket_branch));
        }

        if !worktree_exists(&ctx.worktree_dir) {
            return Err(TicketGitError::WorktreeMissing);
        }

        if !git_status_clean(&ctx.worktree_dir).await? {
            return Err(TicketGitError::Git(
                "worktree has uncommitted changes — commit or stash before rebasing".into(),
            ));
        }

        run_git(&ctx.worktree_dir, &["checkout", &ctx.ticket_branch]).await?;

        let fetch_succeeded = self.soft_fetch_for_rebase(&ctx, base).await;
        let onto_ref = resolve_rebase_onto(&ctx.worktree_dir, base, fetch_succeeded).await?;

        let rebase_output =
            run_git_capture(&ctx.worktree_dir, &["rebase", &onto_ref]).await;

        match rebase_output {
            Ok(_) => {
                let head_sha = git_head_sha(&ctx.worktree_dir).await?;
                let message = format!(
                    "Rebased `{}` onto `{}`",
                    ctx.ticket_branch, onto_ref
                );
                Ok(RebaseBranchResult {
                    base_branch: base.to_string(),
                    onto_ref,
                    ticket_branch: ctx.ticket_branch,
                    head_sha,
                    message,
                })
            }
            Err(detail) => {
                let paths = unmerged_paths(&ctx.worktree_dir).await;
                let paths = if paths.is_empty() {
                    parse_conflict_paths_from_output(&detail)
                } else {
                    paths
                };
                let _ = run_git(&ctx.worktree_dir, &["rebase", "--abort"]).await;
                let clean_after = git_status_clean(&ctx.worktree_dir).await.unwrap_or(false);
                if !paths.is_empty() {
                    let mut detail = detail;
                    if !clean_after {
                        detail.push_str(" (worktree may still be dirty after rebase --abort)");
                    }
                    return Err(TicketGitError::RebaseConflict { paths, detail });
                }
                Err(TicketGitError::Git(detail))
            }
        }
    }

    /// Best-effort fetch so rebase can prefer `origin/<base>`. Failures never
    /// fail the rebase itself.
    async fn soft_fetch_for_rebase(&self, ctx: &TicketGitContext, base: &str) -> bool {
        if let Some(token) = self.decrypt_forge_token(ctx).await {
            if let Some(remote_url) = ctx.remote_url.as_deref() {
                if let Some(https) = https_remote_url(remote_url) {
                    let auth_remote = format!(
                        "https://x-access-token:{}@{}",
                        token.trim(),
                        https.trim_start_matches("https://")
                    );
                    let refspec =
                        format!("+refs/heads/{base}:refs/remotes/origin/{base}");
                    match run_git(
                        &ctx.worktree_dir,
                        &["fetch", &auth_remote, &refspec],
                    )
                    .await
                    {
                        Ok(()) => return true,
                        Err(TicketGitError::Git(msg)) => {
                            tracing::warn!(
                                "authenticated fetch for rebase failed: {}",
                                sanitize_token(&msg, token.trim())
                            );
                        }
                        Err(err) => {
                            tracing::warn!("authenticated fetch for rebase failed: {err}");
                        }
                    }
                }
            }
        }

        if !has_git_remote(&ctx.worktree_dir).await {
            return false;
        }

        match run_git(&ctx.worktree_dir, &["fetch"]).await {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!("plain fetch for rebase failed: {err}");
                false
            }
        }
    }

    async fn decrypt_forge_token(&self, ctx: &TicketGitContext) -> Option<String> {
        let store = self.secret_store?;
        let secret_id = ctx.forge_token_secret_id?;
        SecretService::new(self.pool, store)
            .decrypt_by_id(secret_id)
            .await
            .ok()
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

fn create_pr_gate(
    push_enabled: bool,
    remote_url: Option<&str>,
    forge_token_configured: bool,
) -> (bool, Option<String>) {
    let (ok, reason) = push_gate(push_enabled, remote_url, forge_token_configured);
    if !ok {
        return (false, reason);
    }
    if remote_url.and_then(github_owner_repo).is_none() {
        return (
            false,
            Some("Create PR via API requires a GitHub remote_url".into()),
        );
    }
    (true, None)
}

fn truncate_err(text: &str) -> String {
    const MAX: usize = 400;
    let trimmed = text.trim();
    if trimmed.len() <= MAX {
        trimmed.to_string()
    } else {
        format!("{}…", &trimmed[..MAX])
    }
}

pub fn worktree_exists(worktree_dir: &Path) -> bool {
    worktree_dir.join(".git").exists()
}

pub(crate) fn validate_branch_name(branch: &str) -> Result<(), TicketGitError> {
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

<<<<<<< HEAD
pub(crate) async fn list_local_branches(git_dir: &Path) -> Result<Vec<String>, TicketGitError> {
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

async fn resolve_rebase_onto(
    worktree: &Path,
    base: &str,
    prefer_origin: bool,
) -> Result<String, TicketGitError> {
    let origin_ref = format!("origin/{base}");
    if prefer_origin && git_ref_exists(worktree, &origin_ref).await? {
        return Ok(origin_ref);
    }
    if git_ref_exists(worktree, base).await? {
        return Ok(base.to_string());
    }
    if git_ref_exists(worktree, &origin_ref).await? {
        return Ok(origin_ref);
    }
    Err(TicketGitError::Git(format!(
        "base branch `{base}` was not found locally (and origin/{base} is unavailable)"
    )))
}

async fn has_git_remote(git_dir: &Path) -> bool {
    let Ok(output) = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["remote"])
        .output()
        .await
    else {
        return false;
    };
    output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

async fn unmerged_paths(git_dir: &Path) -> Vec<String> {
    let Ok(output) = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["diff", "--name-only", "--diff-filter=U"])
        .output()
        .await
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let mut paths: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn parse_conflict_paths_from_output(output: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        // e.g. "CONFLICT (content): Merge conflict in path/to/file"
        if let Some(idx) = trimmed.find("Merge conflict in ") {
            let path = trimmed[idx + "Merge conflict in ".len()..].trim();
            if !path.is_empty() {
                paths.push(path.to_string());
            }
            continue;
        }
        // e.g. "CONFLICT (add/add): Merge conflict in path"
        if let Some(rest) = trimmed.strip_prefix("CONFLICT") {
            if let Some(idx) = rest.find(" in ") {
                let path = rest[idx + " in ".len()..].trim();
                if !path.is_empty() && !path.contains(' ') {
                    paths.push(path.to_string());
                }
            }
        }
    }
    paths.sort_unstable();
    paths.dedup();
    paths
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

=======
>>>>>>> agent/TICKET-d4e76ac5
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

    #[test]
    fn parse_conflict_paths_from_rebase_output() {
        let output = "\
Rebasing (1/1)\n\
Auto-merging README.md\n\
CONFLICT (content): Merge conflict in README.md\n\
error: could not apply abc123... feature\n\
hint: Resolve all conflicts manually\n";
        assert_eq!(
            parse_conflict_paths_from_output(output),
            vec!["README.md".to_string()]
        );
    }
}
