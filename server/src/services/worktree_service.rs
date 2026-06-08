use std::path::{Path, PathBuf};

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePaths {
    pub repo_dir: PathBuf,
    pub worktree_dir: PathBuf,
    pub branch_name: String,
}

pub fn compute_paths(
    repos_root: &Path,
    worktrees_root: &Path,
    repo_id: Uuid,
    repo_name: &str,
    ticket_id: Uuid,
    agent_name: &str,
) -> WorktreePaths {
    let agent_slug = crate::domain::slug::slugify(agent_name);
    let repo_slug = crate::domain::slug::slugify(repo_name);
    let ticket_id_str = ticket_id.to_string();
    let ticket_short = ticket_id_str.split('-').next().unwrap_or("ticket");
    WorktreePaths {
        repo_dir: repos_root.join(repo_id.to_string()),
        worktree_dir: worktrees_root.join(format!(
            "TICKET-{ticket_short}-{agent_slug}-{repo_slug}"
        )),
        branch_name: format!("agent/TICKET-{ticket_short}-{agent_slug}"),
    }
}

pub struct WorktreeService {
    repos_root: PathBuf,
    worktrees_root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("remote URL is empty")]
    EmptyRemoteUrl,
    #[error("git command failed: {command}: {stderr}")]
    GitCommandFailed { command: String, stderr: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl WorktreeService {
    pub fn new(repos_root: PathBuf, worktrees_root: PathBuf) -> Self {
        Self {
            repos_root,
            worktrees_root,
        }
    }

    pub fn repos_root(&self) -> &Path {
        &self.repos_root
    }

    pub fn worktrees_root(&self) -> &Path {
        &self.worktrees_root
    }

    pub async fn ensure_repo_clone(
        &self,
        remote_url: &str,
        repo_dir: &Path,
    ) -> Result<(), WorktreeError> {
        if remote_url.trim().is_empty() {
            return Err(WorktreeError::EmptyRemoteUrl);
        }

        if repo_dir.exists() {
            return Ok(());
        }

        if let Some(parent) = repo_dir.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        run_git(&["clone", remote_url, &path_to_string(repo_dir)?]).await
    }

    pub async fn ensure_worktree(
        &self,
        repo_dir: &Path,
        worktree_dir: &Path,
        branch: &str,
    ) -> Result<(), WorktreeError> {
        if worktree_dir.exists() {
            return Ok(());
        }

        if let Some(parent) = worktree_dir.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        run_git_in(
            repo_dir,
            &[
                "worktree",
                "add",
                "-b",
                branch,
                &path_to_string(worktree_dir)?,
            ],
        )
        .await
    }
}

fn path_to_string(path: &Path) -> Result<String, WorktreeError> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| WorktreeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("path is not valid UTF-8: {}", path.display()),
        )))
}

async fn run_git(args: &[&str]) -> Result<(), WorktreeError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .output()
        .await?;

    if output.status.success() {
        Ok(())
    } else {
        Err(WorktreeError::GitCommandFailed {
            command: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

async fn run_git_in(repo_dir: &Path, args: &[&str]) -> Result<(), WorktreeError> {
    let output = tokio::process::Command::new("git")
        .current_dir(repo_dir)
        .args(args)
        .output()
        .await?;

    if output.status.success() {
        Ok(())
    } else {
        Err(WorktreeError::GitCommandFailed {
            command: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use uuid::uuid;

    #[test]
    fn compute_paths_builds_expected_strings() {
        let repos_root = Path::new("/data/repos");
        let worktrees_root = Path::new("/data/worktrees");
        let repo_id = uuid!("660e8400-e29b-41d4-a716-446655440001");
        let ticket_id = uuid!("550e8400-e29b-41d4-a716-446655440000");

        let paths = compute_paths(
            repos_root,
            worktrees_root,
            repo_id,
            "My Repo",
            ticket_id,
            "Frontend Engineer",
        );

        assert_eq!(
            paths.repo_dir,
            PathBuf::from("/data/repos/660e8400-e29b-41d4-a716-446655440001")
        );
        assert_eq!(
            paths.worktree_dir,
            PathBuf::from("/data/worktrees/TICKET-550e8400-frontend-engineer-my-repo")
        );
        assert_eq!(paths.branch_name, "agent/TICKET-550e8400-frontend-engineer");
    }

    #[test]
    fn worktree_service_stores_config_paths() {
        let service = WorktreeService::new(
            PathBuf::from("/data/repos"),
            PathBuf::from("/data/worktrees"),
        );
        assert_eq!(service.repos_root(), Path::new("/data/repos"));
        assert_eq!(service.worktrees_root(), Path::new("/data/worktrees"));
    }

    #[tokio::test]
    async fn ensure_repo_clone_rejects_empty_remote_url() {
        let service = WorktreeService::new(
            PathBuf::from("/tmp/repos"),
            PathBuf::from("/tmp/worktrees"),
        );
        let err = service
            .ensure_repo_clone("", Path::new("/tmp/repos/test"))
            .await
            .unwrap_err();
        assert!(matches!(err, WorktreeError::EmptyRemoteUrl));
    }

    #[tokio::test]
    #[ignore = "requires git CLI"]
    async fn ensure_repo_clone_and_worktree_integration() {
        let base = std::env::temp_dir().join(format!(
            "coppice-worktree-test-{}",
            Uuid::new_v4()
        ));
        let bare = base.join("bare.git");
        let repos_root = base.join("repos");
        let worktrees_root = base.join("worktrees");

        std::fs::create_dir_all(&bare).unwrap();
        assert!(Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare)
            .status()
            .unwrap()
            .success());

        let remote_url = format!("file://{}", bare.display());
        let repo_id = Uuid::new_v4();
        let ticket_id = Uuid::new_v4();
        let paths = compute_paths(
            &repos_root,
            &worktrees_root,
            repo_id,
            "test-repo",
            ticket_id,
            "Test Agent",
        );

        let service = WorktreeService::new(repos_root, worktrees_root);
        service
            .ensure_repo_clone(&remote_url, &paths.repo_dir)
            .await
            .unwrap();
        service
            .ensure_worktree(&paths.repo_dir, &paths.worktree_dir, &paths.branch_name)
            .await
            .unwrap();

        assert!(paths.repo_dir.join(".git").exists());
        assert!(paths.worktree_dir.join(".git").exists());

        std::fs::remove_dir_all(base).ok();
    }
}
