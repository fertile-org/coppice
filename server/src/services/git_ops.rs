//! Shared git helpers for ticket and repository forge operations.

use std::path::Path;
use std::process::Output;

use crate::services::pr_create_url::https_remote_url;

#[derive(Debug, thiserror::Error)]
pub enum GitOpsError {
    #[error("git error: {0}")]
    Git(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Config / forge gate shared by ticket push and default-branch push.
pub fn push_gate(
    push_enabled: bool,
    remote_url: Option<&str>,
    forge_token_configured: bool,
) -> (bool, Option<String>) {
    if !push_enabled {
        return (
            false,
            Some("git.push_enabled is false in server config".into()),
        );
    }
    if remote_url.map(str::trim).filter(|s| !s.is_empty()).is_none() {
        return (
            false,
            Some("Set repository remote URL in Settings → Repositories".into()),
        );
    }
    if !forge_token_configured {
        return (
            false,
            Some("Set a forge token in Settings → Repositories".into()),
        );
    }
    (true, None)
}

/// Fetch needs remote + token only (not `git.push_enabled`).
pub fn fetch_gate(
    remote_url: Option<&str>,
    forge_token_configured: bool,
) -> (bool, Option<String>) {
    if remote_url.map(str::trim).filter(|s| !s.is_empty()).is_none() {
        return (
            false,
            Some("Set repository remote URL in Settings → Repositories".into()),
        );
    }
    if !forge_token_configured {
        return (
            false,
            Some("Set a forge token in Settings → Repositories".into()),
        );
    }
    (true, None)
}

pub fn sanitize_token(message: &str, token: &str) -> String {
    if token.is_empty() {
        return message.to_string();
    }
    message.replace(token, "***")
}

/// `https://x-access-token:{token}@host/owner/repo.git`
pub fn auth_https_remote(remote_url: &str, token: &str) -> Result<String, GitOpsError> {
    let https = https_remote_url(remote_url).ok_or_else(|| {
        GitOpsError::Git("repository remote_url is not a valid HTTPS/SSH git remote".into())
    })?;
    Ok(format!(
        "https://x-access-token:{}@{}",
        token.trim(),
        https.trim_start_matches("https://")
    ))
}

pub fn push_refspec(branch: &str) -> String {
    format!("refs/heads/{branch}:refs/heads/{branch}")
}

pub fn fetch_default_refspec(branch: &str) -> String {
    format!("+refs/heads/{branch}:refs/remotes/origin/{branch}")
}

/// Args for a non-force push of `branch` to `remote`. Never includes force flags.
pub fn push_argv<'a>(remote: &'a str, refspec: &'a str) -> [&'a str; 3] {
    ["push", remote, refspec]
}

pub async fn run_git(cwd: &Path, args: &[&str]) -> Result<(), GitOpsError> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(GitOpsError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            git_stderr(&output)
        )))
    }
}

pub async fn run_git_capture(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
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

pub async fn git_status_clean(cwd: &Path) -> Result<bool, GitOpsError> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["status", "--porcelain"])
        .output()
        .await?;
    if !output.status.success() {
        return Err(GitOpsError::Git(git_stderr(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

pub async fn git_rev_parse(cwd: &Path, rev: &str) -> Result<Option<String>, GitOpsError> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "--verify", rev])
        .output()
        .await?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

pub async fn git_ref_exists(cwd: &Path, ref_name: &str) -> Result<bool, GitOpsError> {
    Ok(git_rev_parse(cwd, ref_name).await?.is_some())
}

pub async fn git_head_sha(cwd: &Path) -> Result<String, GitOpsError> {
    git_rev_parse(cwd, "HEAD")
        .await?
        .ok_or_else(|| GitOpsError::Git("unable to resolve HEAD".into()))
}

/// Count commits unique to left vs right of `left...right` (`rev-list --left-right --count`).
pub async fn ahead_behind(
    cwd: &Path,
    local_ref: &str,
    remote_ref: &str,
) -> Result<(u64, u64), GitOpsError> {
    let range = format!("{local_ref}...{remote_ref}");
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["rev-list", "--left-right", "--count", &range])
        .output()
        .await?;
    if !output.status.success() {
        return Err(GitOpsError::Git(git_stderr(&output)));
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let mut parts = line.split_whitespace();
    let ahead = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| GitOpsError::Git(format!("unexpected rev-list count: {line}")))?;
    let behind = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| GitOpsError::Git(format!("unexpected rev-list count: {line}")))?;
    Ok((ahead, behind))
}

pub async fn list_local_branches(git_dir: &Path) -> Result<Vec<String>, GitOpsError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["branch", "--format=%(refname:short)"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(GitOpsError::Git(git_stderr(&output)));
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

fn git_stderr(output: &Output) -> String {
    combine_git_output(output)
}

fn combine_git_output(output: &Output) -> String {
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
    fn push_gate_reasons() {
        assert!(!push_gate(false, Some("https://github.com/o/r"), true).0);
        assert!(!push_gate(true, None, true).0);
        assert!(!push_gate(true, Some("https://github.com/o/r"), false).0);
        assert!(push_gate(true, Some("https://github.com/o/r"), true).0);
    }

    #[test]
    fn fetch_gate_ignores_push_enabled() {
        assert!(fetch_gate(Some("https://github.com/o/r"), true).0);
        assert!(!fetch_gate(None, true).0);
        assert!(!fetch_gate(Some("https://github.com/o/r"), false).0);
    }

    #[test]
    fn sanitize_token_redacts() {
        let token = "ghs_supersecret";
        let msg = format!("fatal: could not read Password for 'https://x-access-token:{token}@github.com'");
        let cleaned = sanitize_token(&msg, token);
        assert!(!cleaned.contains(token));
        assert!(cleaned.contains("***"));
    }

    #[test]
    fn push_argv_never_includes_force() {
        let remote = "https://x-access-token:***@github.com/o/r.git";
        let refspec = push_refspec("main");
        let args = push_argv(remote, &refspec);
        assert_eq!(args, ["push", remote, "refs/heads/main:refs/heads/main"]);
        for a in args {
            assert!(!a.contains("force"), "unexpected force in {a}");
            assert_ne!(a, "--force");
            assert_ne!(a, "--force-with-lease");
            assert!(!a.starts_with("-f"));
        }
    }

    #[tokio::test]
    async fn ahead_behind_with_synthetic_remote_tracking() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path();
        git(repo, &["init", "-b", "main"]);
        commit_file(repo, "a.txt", "a\n", "initial");
        let base = git_head_sha(repo).await.expect("sha");

        commit_file(repo, "b.txt", "b\n", "ahead");
        // Point origin/main at the first commit (local is 1 ahead).
        git(repo, &["update-ref", "refs/remotes/origin/main", &base]);

        let (ahead, behind) = ahead_behind(repo, "refs/heads/main", "refs/remotes/origin/main")
            .await
            .expect("count");
        assert_eq!((ahead, behind), (1, 0));

        // Make remote ahead of local: reset main to base, leave origin at tip.
        let tip = git_head_sha(repo).await.expect("tip");
        git(repo, &["update-ref", "refs/remotes/origin/main", &tip]);
        git(repo, &["reset", "--hard", &base]);
        let (ahead, behind) = ahead_behind(repo, "refs/heads/main", "refs/remotes/origin/main")
            .await
            .expect("count");
        assert_eq!((ahead, behind), (0, 1));
    }

    #[tokio::test]
    async fn push_to_bare_sibling_without_force() {
        let root = tempfile::tempdir().expect("tempdir");
        let bare = root.path().join("remote.git");
        let local = root.path().join("local");
        std::fs::create_dir_all(&bare).expect("mkdir bare");
        std::fs::create_dir_all(&local).expect("mkdir local");

        git(&bare, &["init", "--bare", "-b", "main"]);
        git(&local, &["init", "-b", "main"]);
        commit_file(&local, "README.md", "# hi\n", "initial");

        let refspec = push_refspec("main");
        let remote = bare.to_str().expect("utf8");
        let args = push_argv(remote, &refspec);
        assert!(!args.iter().any(|a| a.contains("force")));
        run_git(&local, &args).await.expect("push");

        let remote_sha = Command::new("git")
            .args(["--git-dir", bare.to_str().unwrap(), "rev-parse", "refs/heads/main"])
            .output()
            .expect("rev-parse");
        assert!(remote_sha.status.success());
        let local_sha = git_head_sha(&local).await.expect("local sha");
        assert_eq!(
            String::from_utf8_lossy(&remote_sha.stdout).trim(),
            local_sha
        );
    }

    #[tokio::test]
    async fn git_status_clean_detects_dirt() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path();
        git(repo, &["init", "-b", "main"]);
        commit_file(repo, "a.txt", "a\n", "initial");
        assert!(git_status_clean(repo).await.expect("clean"));
        std::fs::write(repo.join("dirty.txt"), "x").expect("write");
        assert!(!git_status_clean(repo).await.expect("dirty"));
    }
}
