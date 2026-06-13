use std::path::{Path, PathBuf};

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePaths {
    pub worktree_dir: PathBuf,
    pub branch_name: String,
}

/// One worktree and branch per ticket — shared by all agents working sequentially on it.
pub fn compute_paths(
    worktrees_root: &Path,
    repo_name: &str,
    ticket_id: Uuid,
) -> WorktreePaths {
    let repo_slug = crate::domain::slug::slugify(repo_name);
    let ticket_id_str = ticket_id.to_string();
    let ticket_short = ticket_id_str.split('-').next().unwrap_or("ticket");
    WorktreePaths {
        worktree_dir: worktrees_root.join(format!("TICKET-{ticket_short}-{repo_slug}")),
        branch_name: format!("agent/TICKET-{ticket_short}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeGitState {
    pub branch: String,
    pub head_sha: String,
    pub newly_committed: bool,
}

/// Fast-forward the worktree to the branch tip in the main repo (if behind).
/// Removes `.agent/` first so injected runtime context cannot block the merge.
pub async fn sync_worktree_to_branch_tip(
    git_dir: &Path,
    worktree: &Path,
    branch: &str,
) -> Result<(), WorktreeError> {
    let branch_tip = git_ref_sha(git_dir, branch).await?;
    let head = git_head_sha(worktree).await?;
    if branch_tip == head {
        return Ok(());
    }

    let agent_dir = worktree.join(".agent");
    if agent_dir.exists() {
        tokio::fs::remove_dir_all(&agent_dir).await?;
    }

    run_git_in(worktree, &["merge", "--ff-only", &branch_tip]).await
}

/// Stage and commit any uncommitted changes (excluding `.agent/`), then return branch + HEAD.
pub async fn finalize_worktree_git(
    worktree: &Path,
    branch: &str,
    commit_message: &str,
) -> Result<WorktreeGitState, WorktreeError> {
    let dirty = worktree_dirty_excluding_agent(worktree).await?;
    let newly_committed = if dirty {
        // Never commit Coppice-injected runtime context under .agent/
        run_git_in(
            worktree,
            &["add", "-A", "--", ".", ":!.agent"],
        )
        .await?;
        run_git_in(worktree, &["commit", "-m", commit_message]).await?;
        true
    } else {
        false
    };
    let head_sha = git_head_sha(worktree).await?;
    Ok(WorktreeGitState {
        branch: branch.to_string(),
        head_sha,
        newly_committed,
    })
}

pub fn format_git_comment_footer(state: &WorktreeGitState) -> String {
    let short_sha = state
        .head_sha
        .get(..7)
        .unwrap_or(state.head_sha.as_str());
    let action = if state.newly_committed {
        "committed"
    } else {
        "no new changes (HEAD"
    };
    if state.newly_committed {
        format!("\n\n---\n**Git:** branch `{branch}` · {action} `{short_sha}`", branch = state.branch)
    } else {
        format!(
            "\n\n---\n**Git:** branch `{branch}` · {action} `{short_sha}`)",
            branch = state.branch
        )
    }
}

async fn worktree_dirty_excluding_agent(worktree: &Path) -> Result<bool, WorktreeError> {
    let output = tokio::process::Command::new("git")
        .current_dir(worktree)
        .args(["status", "--porcelain", "--", ".", ":!.agent"])
        .output()
        .await
        .map_err(WorktreeError::from)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::GitCommandFailed {
            command: "git status --porcelain -- . :!.agent".into(),
            stderr,
        });
    }

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

async fn git_ref_sha(git_dir: &Path, ref_name: &str) -> Result<String, WorktreeError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["rev-parse", ref_name])
        .output()
        .await
        .map_err(WorktreeError::from)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::GitCommandFailed {
            command: format!("git rev-parse {ref_name}"),
            stderr,
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn git_head_sha(worktree: &Path) -> Result<String, WorktreeError> {
    let output = tokio::process::Command::new("git")
        .current_dir(worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .await
        .map_err(WorktreeError::from)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::GitCommandFailed {
            command: "git rev-parse HEAD".into(),
            stderr,
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
        if worktree_dir.join(".git").exists() {
            return Ok(());
        }

        // Directory may have been deleted manually while git still lists a prunable
        // worktree and/or the agent branch still exists from a prior run.
        let _ = run_git_in(git_dir, &["worktree", "prune"]).await;

        if worktree_dir.exists() {
            tokio::fs::remove_dir_all(worktree_dir).await?;
        }

        if let Some(parent) = worktree_dir.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let path = path_to_string(worktree_dir)?;
        match run_git_in(git_dir, &["worktree", "add", "-b", branch, &path]).await {
            Ok(()) => Ok(()),
            Err(WorktreeError::GitCommandFailed { stderr, .. }) if branch_already_exists(&stderr) => {
                run_git_in(git_dir, &["worktree", "add", &path, branch]).await
            }
            Err(err) => Err(err),
        }
    }
}

fn branch_already_exists(stderr: &str) -> bool {
    stderr.contains("already exists")
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
        .await
        .map_err(|err| {
            WorktreeError::Io(std::io::Error::new(
                err.kind(),
                format!(
                    "repository not accessible at {}: {err}",
                    git_dir.display()
                ),
            ))
        })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let combined = match (stderr.is_empty(), stdout.is_empty()) {
            (false, false) => format!("{stderr}\n{stdout}"),
            (false, true) => stderr,
            (true, false) => stdout,
            (true, true) => format!("exit code {}", output.status),
        };
        Err(WorktreeError::GitCommandFailed {
            command: format!("git {}", args.join(" ")),
            stderr: combined,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::uuid;

    #[test]
    fn compute_paths_builds_per_ticket_strings() {
        let worktrees_root = Path::new("/data/worktrees");
        let ticket_id = uuid!("550e8400-e29b-41d4-a716-446655440000");

        let paths = compute_paths(worktrees_root, "My Repo", ticket_id);

        assert_eq!(
            paths.worktree_dir,
            PathBuf::from("/data/worktrees/TICKET-550e8400-my-repo")
        );
        assert_eq!(paths.branch_name, "agent/TICKET-550e8400");
    }

    #[test]
    fn format_git_comment_footer_notes_branch_and_commit() {
        let footer = format_git_comment_footer(&WorktreeGitState {
            branch: "agent/TICKET-abc".into(),
            head_sha: "deadbeef1234".into(),
            newly_committed: true,
        });
        assert!(footer.contains("agent/TICKET-abc"));
        assert!(footer.contains("deadbee"));
        assert!(footer.contains("committed"));
    }

    #[tokio::test]
    async fn finalize_worktree_git_commits_dirty_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo.git");
        std::process::Command::new("git")
            .args(["init", repo.to_str().unwrap()])
            .status()
            .expect("git init");
        std::process::Command::new("git")
            .current_dir(&repo)
            .args(["config", "user.email", "test@example.com"])
            .status()
            .expect("git config email");
        std::process::Command::new("git")
            .current_dir(&repo)
            .args(["config", "user.name", "Test"])
            .status()
            .expect("git config name");
        std::process::Command::new("git")
            .current_dir(&repo)
            .args(["commit", "--allow-empty", "-m", "initial"])
            .status()
            .expect("initial commit");

        let worktree = tmp.path().join("wt");
        std::process::Command::new("git")
            .current_dir(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "agent/TICKET-test",
                worktree.to_str().unwrap(),
            ])
            .status()
            .expect("worktree add");

        std::fs::write(worktree.join("change.txt"), "hello").expect("write file");

        let state = finalize_worktree_git(
            &worktree,
            "agent/TICKET-test",
            "[coppice] test: sample",
        )
        .await
        .expect("finalize git");

        assert!(state.newly_committed);
        assert!(!state.head_sha.is_empty());
        assert_eq!(state.branch, "agent/TICKET-test");
    }

    #[test]
    fn worktree_service_stores_worktrees_root() {
        let service = WorktreeService::new(PathBuf::from("/data/worktrees"));
        assert_eq!(service.worktrees_root(), Path::new("/data/worktrees"));
    }

    #[test]
    fn branch_already_exists_detects_git_stderr() {
        assert!(branch_already_exists(
            "fatal: a branch named 'agent/TICKET-5681de33-researcher' already exists"
        ));
        assert!(!branch_already_exists("fatal: not a git repository"));
    }

    #[test]
    fn git_command_failed_display_includes_stderr() {
        let err = WorktreeError::GitCommandFailed {
            command: "git worktree add".into(),
            stderr: "fatal: not a git repository".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("fatal: not a git repository"));
    }
}
