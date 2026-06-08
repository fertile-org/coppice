use std::path::{Path, PathBuf};

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePaths {
    pub worktree_dir: PathBuf,
    pub branch_name: String,
}

pub fn compute_paths(
    worktrees_root: &Path,
    repo_name: &str,
    ticket_id: Uuid,
    agent_name: &str,
) -> WorktreePaths {
    let agent_slug = crate::domain::slug::slugify(agent_name);
    let repo_slug = crate::domain::slug::slugify(repo_name);
    let ticket_id_str = ticket_id.to_string();
    let ticket_short = ticket_id_str.split('-').next().unwrap_or("ticket");
    WorktreePaths {
        worktree_dir: worktrees_root.join(format!(
            "TICKET-{ticket_short}-{agent_slug}-{repo_slug}"
        )),
        branch_name: format!("agent/TICKET-{ticket_short}-{agent_slug}"),
    }
}

pub struct WorktreeService {
    worktrees_root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("git command failed: {command}: {stderr}")]
    GitCommandFailed { command: String, stderr: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl WorktreeService {
    pub fn new(worktrees_root: PathBuf) -> Self {
        Self { worktrees_root }
    }

    pub fn worktrees_root(&self) -> &Path {
        &self.worktrees_root
    }

    pub async fn ensure_worktree(
        &self,
        git_dir: &Path,
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
            git_dir,
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

async fn run_git_in(git_dir: &Path, args: &[&str]) -> Result<(), WorktreeError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
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
    use uuid::uuid;

    #[test]
    fn compute_paths_builds_expected_strings() {
        let worktrees_root = Path::new("/data/worktrees");
        let ticket_id = uuid!("550e8400-e29b-41d4-a716-446655440000");

        let paths = compute_paths(
            worktrees_root,
            "My Repo",
            ticket_id,
            "Frontend Engineer",
        );

        assert_eq!(
            paths.worktree_dir,
            PathBuf::from("/data/worktrees/TICKET-550e8400-frontend-engineer-my-repo")
        );
        assert_eq!(paths.branch_name, "agent/TICKET-550e8400-frontend-engineer");
    }

    #[test]
    fn worktree_service_stores_worktrees_root() {
        let service = WorktreeService::new(PathBuf::from("/data/worktrees"));
        assert_eq!(service.worktrees_root(), Path::new("/data/worktrees"));
    }
}
