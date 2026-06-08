use std::path::Path;
use std::process::Command;

use crate::domain::repo::VerificationStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyResult {
    pub status: VerificationStatus,
    pub error: Option<String>,
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
