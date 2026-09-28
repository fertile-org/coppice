use std::path::Path;
use std::process::Command;

use crate::domain::repo::VerificationStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyResult {
    pub status: VerificationStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectPathResult {
    pub verification: VerifyResult,
    pub suggested_name: Option<String>,
    pub remote_url: Option<String>,
    pub default_branch: Option<String>,
}

pub fn verify_local_path(path: &Path) -> VerifyResult {
    if path.as_os_str().is_empty() || !path.exists() {
        return VerifyResult {
            status: VerificationStatus::PathMissing,
            error: None,
        };
    }

    match Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["rev-parse", "--git-dir"])
        .output()
    {
        Ok(output) if output.status.success() => VerifyResult {
            status: VerificationStatus::Ready,
            error: None,
        },
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = stderr
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("git rev-parse failed")
                .trim()
                .to_string();
            let status = if stderr.contains("not a git repository") {
                VerificationStatus::NotGitRepo
            } else {
                VerificationStatus::Error
            };
            VerifyResult {
                status,
                error: Some(message),
            }
        }
        Err(err) => VerifyResult {
            status: VerificationStatus::Error,
            error: Some(err.to_string()),
        },
    }
}

/// Probe a local path for registration autofill (name, origin URL, default branch).
pub fn inspect_local_path(path: &Path) -> InspectPathResult {
    let verification = verify_local_path(path);
    let suggested_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    if verification.status != VerificationStatus::Ready {
        return InspectPathResult {
            verification,
            suggested_name,
            remote_url: None,
            default_branch: None,
        };
    }

    let remote_url = git_stdout(path, &["remote", "get-url", "origin"]);
    let default_branch = git_stdout(path, &["symbolic-ref", "--short", "HEAD"])
        .or_else(|| git_stdout(path, &["rev-parse", "--abbrev-ref", "HEAD"]))
        .filter(|branch| branch != "HEAD");

    InspectPathResult {
        verification,
        suggested_name,
        remote_url,
        default_branch,
    }
}

fn git_stdout(path: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use uuid::Uuid;

    #[test]
    fn missing_path_returns_path_missing() {
        let path = std::env::temp_dir().join(format!("does-not-exist-{}", Uuid::new_v4()));
        let result = verify_local_path(&path);
        assert_eq!(result.status, VerificationStatus::PathMissing);
        assert!(result.error.is_none());
    }

    #[test]
    fn non_git_dir_returns_not_git_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = verify_local_path(dir.path());
        assert_eq!(result.status, VerificationStatus::NotGitRepo);
    }

    #[test]
    fn git_init_dir_returns_ready() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path();

        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(path)
            .output()
            .expect("git init");
        std::fs::write(path.join("README.md"), "# test\n").expect("write readme");
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(path)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@localhost")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@localhost")
            .current_dir(path)
            .output()
            .expect("git commit");

        let result = verify_local_path(path);
        assert_eq!(result.status, VerificationStatus::Ready);
        assert!(result.error.is_none());
    }
}
