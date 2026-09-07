//! Human-admin sync of a registered repository's default branch with its remote.

use std::path::PathBuf;

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::crypto::SecretStore;
use crate::services::git_ops::{
    ahead_behind, auth_https_remote, fetch_default_refspec, fetch_gate, git_rev_parse,
    git_status_clean, push_argv, push_gate, push_refspec, run_git, sanitize_token, GitOpsError,
};
use crate::services::pr_create_url::https_remote_url;
use crate::services::secret_service::SecretService;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultBranchSyncStatus {
    pub default_branch: String,
    pub local_sha: Option<String>,
    pub remote_sha: Option<String>,
    pub ahead_count: Option<u64>,
    pub behind_count: Option<u64>,
    pub working_tree_clean: bool,
    pub push_enabled: bool,
    pub forge_token_configured: bool,
    pub can_fetch: bool,
    pub can_push: bool,
    pub can_pull: bool,
    pub fetch_disabled_reason: Option<String>,
    pub push_disabled_reason: Option<String>,
    pub pull_disabled_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushDefaultBranchResult {
    pub default_branch: String,
    pub remote: String,
    pub message: String,
    pub status: DefaultBranchSyncStatus,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullDefaultBranchResult {
    pub default_branch: String,
    pub message: String,
    pub status: DefaultBranchSyncStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum RepoGitError {
    #[error("repository not found")]
    RepoNotFound,
    #[error("repository is not ready")]
    RepoNotReady,
    #[error("git push is disabled (set git.push_enabled = true)")]
    PushDisabled,
    #[error("repository has no remote_url")]
    NoRemoteUrl,
    #[error("repository has no forge token — set one in Settings → Repositories")]
    NoForgeToken,
    #[error("git error: {0}")]
    Git(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Secret(#[from] crate::services::secret_service::SecretError),
}

impl From<GitOpsError> for RepoGitError {
    fn from(err: GitOpsError) -> Self {
        match err {
            GitOpsError::Git(msg) => RepoGitError::Git(msg),
            GitOpsError::Io(err) => RepoGitError::Io(err),
        }
    }
}

struct RepoGitContext {
    git_dir: PathBuf,
    default_branch: String,
    remote_url: Option<String>,
    forge_token_secret_id: Option<Uuid>,
}

pub struct RepoGitService<'a> {
    pool: &'a PgPool,
    push_enabled: bool,
    secret_store: &'a SecretStore,
}

impl<'a> RepoGitService<'a> {
    pub fn new(pool: &'a PgPool, push_enabled: bool, secret_store: &'a SecretStore) -> Self {
        Self {
            pool,
            push_enabled,
            secret_store,
        }
    }

    async fn resolve(&self, repo_id: Uuid) -> Result<RepoGitContext, RepoGitError> {
        let row = sqlx::query(
            r#"
            SELECT local_path, default_branch, verification_status, remote_url,
                   forge_token_secret_id
            FROM repos
            WHERE id = $1
            "#,
        )
        .bind(repo_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(RepoGitError::RepoNotFound)?;

        let verification_status: String = row.get("verification_status");
        if verification_status != "ready" {
            return Err(RepoGitError::RepoNotReady);
        }

        Ok(RepoGitContext {
            git_dir: PathBuf::from(row.get::<String, _>("local_path")),
            default_branch: row.get("default_branch"),
            remote_url: row.get("remote_url"),
            forge_token_secret_id: row.get("forge_token_secret_id"),
        })
    }

    pub async fn default_branch_sync_status(
        &self,
        repo_id: Uuid,
    ) -> Result<DefaultBranchSyncStatus, RepoGitError> {
        let ctx = self.resolve(repo_id).await?;
        self.build_status(&ctx).await
    }

    async fn build_status(&self, ctx: &RepoGitContext) -> Result<DefaultBranchSyncStatus, RepoGitError> {
        let branch = &ctx.default_branch;
        let local_ref = format!("refs/heads/{branch}");
        let remote_ref = format!("refs/remotes/origin/{branch}");

        let local_sha = git_rev_parse(&ctx.git_dir, &local_ref).await?;
        let remote_sha = git_rev_parse(&ctx.git_dir, &remote_ref).await?;
        let working_tree_clean = git_status_clean(&ctx.git_dir).await?;

        let (ahead_count, behind_count, missing_remote_reason) = match (&local_sha, &remote_sha) {
            (Some(_), Some(_)) => {
                let (ahead, behind) = ahead_behind(&ctx.git_dir, &local_ref, &remote_ref).await?;
                (Some(ahead), Some(behind), None)
            }
            (Some(_), None) => (None, None, Some("Fetch remote first".to_string())),
            (None, _) => (
                None,
                None,
                Some(format!("Local branch `{branch}` not found")),
            ),
        };

        let forge_token_configured = ctx.forge_token_secret_id.is_some();
        let (can_fetch, fetch_disabled_reason) =
            fetch_gate(ctx.remote_url.as_deref(), forge_token_configured);

        let (config_ok, config_reason) = push_gate(
            self.push_enabled,
            ctx.remote_url.as_deref(),
            forge_token_configured,
        );

        let (can_push, push_disabled_reason) = default_branch_push_gate(
            config_ok,
            config_reason,
            working_tree_clean,
            ahead_count,
            behind_count,
            missing_remote_reason.as_deref(),
            local_sha.is_some(),
        );

        let (can_pull, pull_disabled_reason) = default_branch_pull_gate(
            can_fetch,
            fetch_disabled_reason.clone(),
            working_tree_clean,
            ahead_count,
            behind_count,
            missing_remote_reason.as_deref(),
            local_sha.is_some(),
        );

        Ok(DefaultBranchSyncStatus {
            default_branch: branch.clone(),
            local_sha,
            remote_sha,
            ahead_count,
            behind_count,
            working_tree_clean,
            push_enabled: self.push_enabled,
            forge_token_configured,
            can_fetch,
            can_push,
            can_pull,
            fetch_disabled_reason,
            push_disabled_reason,
            pull_disabled_reason,
        })
    }

    pub async fn fetch_default_branch(
        &self,
        repo_id: Uuid,
    ) -> Result<DefaultBranchSyncStatus, RepoGitError> {
        let ctx = self.resolve(repo_id).await?;
        let remote_url = ctx
            .remote_url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or(RepoGitError::NoRemoteUrl)?;
        let secret_id = ctx
            .forge_token_secret_id
            .ok_or(RepoGitError::NoForgeToken)?;
        let token = SecretService::new(self.pool, self.secret_store)
            .decrypt_by_id(secret_id)
            .await?;

        let auth_remote = auth_https_remote(remote_url, token.trim())?;
        let refspec = fetch_default_refspec(&ctx.default_branch);
        match run_git(
            &ctx.git_dir,
            &["fetch", &auth_remote, &refspec],
        )
        .await
        {
            Ok(()) => {}
            Err(GitOpsError::Git(msg)) => {
                return Err(RepoGitError::Git(sanitize_token(&msg, token.trim())));
            }
            Err(other) => return Err(other.into()),
        }

        self.build_status(&ctx).await
    }

    pub async fn push_default_branch(
        &self,
        repo_id: Uuid,
    ) -> Result<PushDefaultBranchResult, RepoGitError> {
        if !self.push_enabled {
            return Err(RepoGitError::PushDisabled);
        }
        let ctx = self.resolve(repo_id).await?;
        let remote_url = ctx
            .remote_url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or(RepoGitError::NoRemoteUrl)?;
        let secret_id = ctx
            .forge_token_secret_id
            .ok_or(RepoGitError::NoForgeToken)?;
        let token = SecretService::new(self.pool, self.secret_store)
            .decrypt_by_id(secret_id)
            .await?;
        let https = https_remote_url(remote_url).ok_or(RepoGitError::NoRemoteUrl)?;

        self.enforce_push_gates(&ctx).await?;

        let auth_remote = auth_https_remote(remote_url, token.trim())?;
        let refspec = push_refspec(&ctx.default_branch);
        let args = push_argv(&auth_remote, &refspec);
        debug_assert!(
            !args.iter().any(|a| *a == "--force"
                || *a == "--force-with-lease"
                || *a == "-f"
                || a.starts_with("--force")),
            "default-branch push must never force"
        );

        match run_git(&ctx.git_dir, &args).await {
            Ok(()) => {}
            Err(GitOpsError::Git(msg)) => {
                return Err(RepoGitError::Git(sanitize_token(&msg, token.trim())));
            }
            Err(other) => return Err(other.into()),
        }

        // Refresh remote-tracking ref to match what we just pushed (no network).
        let local_ref = format!("refs/heads/{}", ctx.default_branch);
        let remote_ref = format!("refs/remotes/origin/{}", ctx.default_branch);
        if let Some(sha) = git_rev_parse(&ctx.git_dir, &local_ref).await? {
            let _ = run_git(&ctx.git_dir, &["update-ref", &remote_ref, &sha]).await;
        }

        let status = self.build_status(&ctx).await?;
        Ok(PushDefaultBranchResult {
            default_branch: ctx.default_branch,
            remote: https,
            message: "Default branch pushed".into(),
            status,
        })
    }

    /// Push default branch to an explicit remote URL (used by tests with a bare sibling).
    /// Same refspec and gates as production HTTPS push; does not embed a forge token.
    pub async fn push_default_branch_to_remote(
        &self,
        repo_id: Uuid,
        remote: &str,
    ) -> Result<PushDefaultBranchResult, RepoGitError> {
        if !self.push_enabled {
            return Err(RepoGitError::PushDisabled);
        }
        let ctx = self.resolve(repo_id).await?;
        self.enforce_push_gates(&ctx).await?;

        let refspec = push_refspec(&ctx.default_branch);
        let args = push_argv(remote, &refspec);
        assert!(
            !args.iter().any(|a| a.contains("force")),
            "force flag must not appear in push argv: {args:?}"
        );
        run_git(&ctx.git_dir, &args).await?;

        let local_ref = format!("refs/heads/{}", ctx.default_branch);
        let remote_ref = format!("refs/remotes/origin/{}", ctx.default_branch);
        if let Some(sha) = git_rev_parse(&ctx.git_dir, &local_ref).await? {
            let _ = run_git(&ctx.git_dir, &["update-ref", &remote_ref, &sha]).await;
        }

        let status = self.build_status(&ctx).await?;
        Ok(PushDefaultBranchResult {
            default_branch: ctx.default_branch,
            remote: remote.to_string(),
            message: "Default branch pushed".into(),
            status,
        })
    }

    /// Fetch the default branch from the configured remote, then fast-forward the local
    /// default branch when strictly behind. Never merges diverged history.
    pub async fn pull_default_branch(
        &self,
        repo_id: Uuid,
    ) -> Result<PullDefaultBranchResult, RepoGitError> {
        let ctx = self.resolve(repo_id).await?;
        let remote_url = ctx
            .remote_url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or(RepoGitError::NoRemoteUrl)?;
        let secret_id = ctx
            .forge_token_secret_id
            .ok_or(RepoGitError::NoForgeToken)?;
        let token = SecretService::new(self.pool, self.secret_store)
            .decrypt_by_id(secret_id)
            .await?;
        let auth_remote = auth_https_remote(remote_url, token.trim())?;
        self.fetch_then_ff_pull(&ctx, &auth_remote, Some(token.trim()))
            .await
    }

    /// Fetch from an explicit remote URL then fast-forward (tests with a bare sibling).
    pub async fn pull_default_branch_from_remote(
        &self,
        repo_id: Uuid,
        remote: &str,
    ) -> Result<PullDefaultBranchResult, RepoGitError> {
        let ctx = self.resolve(repo_id).await?;
        self.fetch_then_ff_pull(&ctx, remote, None).await
    }

    async fn fetch_then_ff_pull(
        &self,
        ctx: &RepoGitContext,
        fetch_remote: &str,
        token: Option<&str>,
    ) -> Result<PullDefaultBranchResult, RepoGitError> {
        let refspec = fetch_default_refspec(&ctx.default_branch);
        match run_git(ctx.git_dir.as_path(), &["fetch", fetch_remote, &refspec]).await {
            Ok(()) => {}
            Err(GitOpsError::Git(msg)) => {
                let sanitized = match token {
                    Some(t) => sanitize_token(&msg, t),
                    None => msg,
                };
                return Err(RepoGitError::Git(sanitized));
            }
            Err(other) => return Err(other.into()),
        }

        self.enforce_pull_gates(ctx).await?;
        self.fast_forward_default_branch(ctx).await?;

        let status = self.build_status(ctx).await?;
        Ok(PullDefaultBranchResult {
            default_branch: ctx.default_branch.clone(),
            message: "Default branch pulled".into(),
            status,
        })
    }

    async fn fast_forward_default_branch(
        &self,
        ctx: &RepoGitContext,
    ) -> Result<(), RepoGitError> {
        let local_ref = format!("refs/heads/{}", ctx.default_branch);
        let remote_ref = format!("refs/remotes/origin/{}", ctx.default_branch);
        let remote_sha = git_rev_parse(&ctx.git_dir, &remote_ref)
            .await?
            .ok_or_else(|| {
                RepoGitError::Git(format!("Remote-tracking ref `{remote_ref}` not found"))
            })?;

        // Belt-and-suspenders: refuse unless local is a strict ancestor of remote.
        match run_git(
            &ctx.git_dir,
            &["merge-base", "--is-ancestor", &local_ref, &remote_ref],
        )
        .await
        {
            Ok(()) => {}
            Err(GitOpsError::Git(_)) => {
                return Err(RepoGitError::Git(
                    "Local default branch is not a fast-forward of remote — refuse pull".into(),
                ));
            }
            Err(other) => return Err(other.into()),
        }

        let on_default = head_points_at_ref(&ctx.git_dir, &local_ref).await?;
        if on_default {
            // Update HEAD, index, and working tree together.
            run_git(&ctx.git_dir, &["merge", "--ff-only", &remote_ref]).await?;
        } else {
            // Leave unrelated checked-out branches alone; only move the default branch tip.
            run_git(&ctx.git_dir, &["update-ref", &local_ref, &remote_sha]).await?;
        }
        Ok(())
    }

    async fn enforce_pull_gates(&self, ctx: &RepoGitContext) -> Result<(), RepoGitError> {
        let status = self.build_status(ctx).await?;
        if !status.can_pull {
            let reason = status
                .pull_disabled_reason
                .unwrap_or_else(|| "Pull is not allowed".into());
            if ctx.remote_url.as_deref().map(str::trim).filter(|s| !s.is_empty()).is_none() {
                return Err(RepoGitError::NoRemoteUrl);
            }
            if ctx.forge_token_secret_id.is_none() {
                return Err(RepoGitError::NoForgeToken);
            }
            return Err(RepoGitError::Git(reason));
        }
        Ok(())
    }

    async fn enforce_push_gates(&self, ctx: &RepoGitContext) -> Result<(), RepoGitError> {
        let status = self.build_status(ctx).await?;
        if !status.can_push {
            let reason = status
                .push_disabled_reason
                .unwrap_or_else(|| "Push is not allowed".into());
            if !self.push_enabled {
                return Err(RepoGitError::PushDisabled);
            }
            if ctx.remote_url.as_deref().map(str::trim).filter(|s| !s.is_empty()).is_none() {
                return Err(RepoGitError::NoRemoteUrl);
            }
            if ctx.forge_token_secret_id.is_none() {
                return Err(RepoGitError::NoForgeToken);
            }
            return Err(RepoGitError::Git(reason));
        }
        Ok(())
    }
}

fn default_branch_push_gate(
    config_ok: bool,
    config_reason: Option<String>,
    working_tree_clean: bool,
    ahead_count: Option<u64>,
    behind_count: Option<u64>,
    missing_remote_reason: Option<&str>,
    local_branch_exists: bool,
) -> (bool, Option<String>) {
    if !config_ok {
        return (false, config_reason);
    }
    if !local_branch_exists {
        return (false, Some("Local default branch not found".into()));
    }
    if !working_tree_clean {
        return (
            false,
            Some(
                "Main repository has uncommitted changes — commit or stash before pushing"
                    .into(),
            ),
        );
    }
    if let Some(reason) = missing_remote_reason {
        return (false, Some(reason.to_string()));
    }
    let behind = behind_count.unwrap_or(0);
    let ahead = ahead_count.unwrap_or(0);
    if behind > 0 {
        return (
            false,
            Some(
                "Local default branch is behind or diverged from remote — use Pull when strictly behind, or resolve divergence outside Coppice"
                    .into(),
            ),
        );
    }
    if ahead == 0 {
        return (
            false,
            Some("Local default branch is not ahead of remote".into()),
        );
    }
    (true, None)
}

fn default_branch_pull_gate(
    config_ok: bool,
    config_reason: Option<String>,
    working_tree_clean: bool,
    ahead_count: Option<u64>,
    behind_count: Option<u64>,
    missing_remote_reason: Option<&str>,
    local_branch_exists: bool,
) -> (bool, Option<String>) {
    if !config_ok {
        return (false, config_reason);
    }
    if !local_branch_exists {
        return (false, Some("Local default branch not found".into()));
    }
    if !working_tree_clean {
        return (
            false,
            Some(
                "Main repository has uncommitted changes — commit or stash before pulling"
                    .into(),
            ),
        );
    }
    if let Some(reason) = missing_remote_reason {
        return (false, Some(reason.to_string()));
    }
    let behind = behind_count.unwrap_or(0);
    let ahead = ahead_count.unwrap_or(0);
    if ahead > 0 && behind > 0 {
        return (
            false,
            Some(
                "Local default branch has diverged from remote — refuse pull without merge or rebase"
                    .into(),
            ),
        );
    }
    if ahead > 0 {
        return (
            false,
            Some("Local default branch is ahead of remote — pull is not needed".into()),
        );
    }
    if behind == 0 {
        return (
            false,
            Some("Local default branch is not behind remote".into()),
        );
    }
    (true, None)
}

async fn head_points_at_ref(git_dir: &std::path::Path, local_ref: &str) -> Result<bool, RepoGitError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        .await?;
    if !output.status.success() {
        return Ok(false);
    }
    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(head == local_ref)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit_file(cwd: &Path, name: &str, contents: &str, message: &str) {
        std::fs::write(cwd.join(name), contents).expect("write");
        git(cwd, &["add", name]);
        let out = Command::new("git")
            .current_dir(cwd)
            .args(["commit", "-m", message])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@localhost")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@localhost")
            .output()
            .expect("commit");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn push_gate_blocks_dirty_behind_and_not_ahead() {
        let (ok, reason) = default_branch_push_gate(
            true,
            None,
            false,
            Some(1),
            Some(0),
            None,
            true,
        );
        assert!(!ok);
        assert!(reason.unwrap().contains("uncommitted"));

        let (ok, reason) = default_branch_push_gate(
            true,
            None,
            true,
            Some(1),
            Some(2),
            None,
            true,
        );
        assert!(!ok);
        let reason = reason.unwrap();
        assert!(reason.contains("behind") || reason.contains("diverged"));
        assert!(
            reason.to_lowercase().contains("pull"),
            "behind push reason should point operators at Pull: {reason}"
        );

        let (ok, reason) =
            default_branch_push_gate(true, None, true, Some(0), Some(0), None, true);
        assert!(!ok);
        assert!(reason.unwrap().contains("not ahead"));

        let (ok, reason) =
            default_branch_push_gate(true, None, true, None, None, Some("Fetch remote first"), true);
        assert!(!ok);
        assert_eq!(reason.as_deref(), Some("Fetch remote first"));

        let (ok, _) = default_branch_push_gate(true, None, true, Some(2), Some(0), None, true);
        assert!(ok);
    }

    #[test]
    fn pull_gate_allows_strictly_behind_clean_checkout() {
        let (ok, reason) = default_branch_pull_gate(
            true,
            None,
            true,
            Some(0),
            Some(2),
            None,
            true,
        );
        assert!(ok, "{reason:?}");
        assert!(reason.is_none());
    }

    #[test]
    fn pull_gate_blocks_dirty_diverged_not_behind_and_missing_remote() {
        let (ok, reason) = default_branch_pull_gate(
            true,
            None,
            false,
            Some(0),
            Some(1),
            None,
            true,
        );
        assert!(!ok);
        assert!(reason.unwrap().contains("uncommitted"));

        let (ok, reason) = default_branch_pull_gate(
            true,
            None,
            true,
            Some(1),
            Some(1),
            None,
            true,
        );
        assert!(!ok);
        let reason = reason.unwrap();
        assert!(reason.contains("diverged") || reason.contains("ahead"));

        let (ok, reason) =
            default_branch_pull_gate(true, None, true, Some(0), Some(0), None, true);
        assert!(!ok);
        assert!(reason.unwrap().contains("not behind"));

        let (ok, reason) =
            default_branch_pull_gate(true, None, true, Some(2), Some(0), None, true);
        assert!(!ok);
        let reason = reason.unwrap();
        assert!(reason.contains("ahead") || reason.contains("not behind"));

        let (ok, reason) = default_branch_pull_gate(
            true,
            None,
            true,
            None,
            None,
            Some("Fetch remote first"),
            true,
        );
        assert!(!ok);
        assert_eq!(reason.as_deref(), Some("Fetch remote first"));

        let (ok, reason) =
            default_branch_pull_gate(false, Some("Set a forge token".into()), true, Some(0), Some(1), None, true);
        assert!(!ok);
        assert!(reason.unwrap().contains("forge token"));

        let (ok, reason) =
            default_branch_pull_gate(true, None, true, Some(0), Some(1), None, false);
        assert!(!ok);
        assert!(reason.unwrap().contains("Local default branch not found"));
    }

    #[test]
    fn sanitize_token_on_git_errors() {
        let token = "ghs_abc123token";
        let err = format!("git push failed: Authentication failed for 'https://x-access-token:{token}@github.com/o/r.git'");
        let cleaned = sanitize_token(&err, token);
        assert!(!cleaned.contains(token));
    }

    #[tokio::test]
    async fn status_ahead_behind_from_synthetic_remote_ref() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path();
        git(repo, &["init", "-b", "main"]);
        commit_file(repo, "a.txt", "a\n", "initial");
        let base = Command::new("git")
            .current_dir(repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("sha");
        let base_sha = String::from_utf8_lossy(&base.stdout).trim().to_string();
        commit_file(repo, "b.txt", "b\n", "local ahead");
        git(repo, &["update-ref", "refs/remotes/origin/main", &base_sha]);

        let (ahead, behind) = ahead_behind(
            repo,
            "refs/heads/main",
            "refs/remotes/origin/main",
        )
        .await
        .expect("count");
        assert_eq!((ahead, behind), (1, 0));

        let (can_push, reason) =
            default_branch_push_gate(true, None, true, Some(ahead), Some(behind), None, true);
        assert!(can_push, "{reason:?}");
    }

    #[tokio::test]
    async fn head_points_at_ref_detects_checked_out_branch() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path();
        git(repo, &["init", "-b", "main"]);
        commit_file(repo, "a.txt", "a\n", "initial");
        assert!(head_points_at_ref(repo, "refs/heads/main")
            .await
            .expect("head"));

        git(repo, &["checkout", "-b", "feature"]);
        assert!(!head_points_at_ref(repo, "refs/heads/main")
            .await
            .expect("head"));
        assert!(head_points_at_ref(repo, "refs/heads/feature")
            .await
            .expect("head"));
    }
}
