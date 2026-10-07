use std::path::{Path, PathBuf};

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePaths {
    pub worktree_dir: PathBuf,
    pub branch_name: String,
}

/// One worktree and branch per ticket — shared by all agents working sequentially on it.
pub fn compute_paths(worktrees_root: &Path, repo_name: &str, ticket_id: Uuid) -> WorktreePaths {
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
#[derive(Debug, Clone)]
pub struct GitAuthor {
    pub name: String,
    pub email: String,
}

pub async fn finalize_worktree_git(
    worktree: &Path,
    branch: &str,
    commit_message: &str,
    author: Option<&GitAuthor>,
) -> Result<WorktreeGitState, WorktreeError> {
    let dirty = worktree_dirty_excluding_agent(worktree).await?;
    let newly_committed = if dirty {
        // Never commit Coppice-injected runtime context under .agent/
        run_git_in(worktree, &["add", "-A", "--", ".", ":!.agent"]).await?;
        match author {
            Some(author) => {
                let name_cfg = format!("user.name={}", author.name);
                let email_cfg = format!("user.email={}", author.email);
                run_git_in(
                    worktree,
                    &[
                        "-c",
                        &name_cfg,
                        "-c",
                        &email_cfg,
                        "commit",
                        "-m",
                        commit_message,
                    ],
                )
                .await?;
            }
            None => {
                run_git_in(worktree, &["commit", "-m", commit_message]).await?;
            }
        }
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
    let short_sha = state.head_sha.get(..7).unwrap_or(state.head_sha.as_str());
    let action = if state.newly_committed {
        "committed"
    } else {
        "no new changes (HEAD"
    };
    if state.newly_committed {
        format!(
            "\n\n---\n**Git:** branch `{branch}` · {action} `{short_sha}`",
            branch = state.branch
        )
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
            Err(WorktreeError::GitCommandFailed { stderr, .. })
                if branch_already_exists(&stderr) =>
            {
                run_git_in(git_dir, &["worktree", "add", &path, branch]).await
            }
            Err(err) => Err(err),
        }
    }
}

fn branch_already_exists(stderr: &str) -> bool {
    stderr.contains("already exists")
}

/// Where a plan run works. Never the ticket worktree.
pub fn plan_scratch_dir(worktrees_root: &Path, run_id: Uuid) -> PathBuf {
    worktrees_root.join("plan-scratch").join(run_id.to_string())
}

/// The repo a plan scratch is detached from, when the ticket has one.
pub struct PlanScratchSource {
    pub git_dir: PathBuf,
    pub ticket_branch: String,
    /// Used when `main` does not exist. Usually the repo's default branch.
    pub fallback_branch: String,
}

/// A detached git worktree for one plan run. Dropping it does not commit or push.
pub struct PlanScratch {
    path: PathBuf,
    source_git_dir: Option<PathBuf>,
}

impl PlanScratch {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Detach at the ticket branch head, or `main` when that branch does not exist yet.
    /// With no source repo, init a private detached repo that is not a ticket worktree.
    pub async fn create(
        worktrees_root: &Path,
        run_id: Uuid,
        source: Option<PlanScratchSource>,
    ) -> Result<Self, WorktreeError> {
        let path = plan_scratch_dir(worktrees_root, run_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let source_git_dir = if let Some(source) = source {
            let rev = plan_base_rev(
                &source.git_dir,
                &source.ticket_branch,
                &source.fallback_branch,
            )
            .await?;
            if path.exists() {
                let _ = remove_linked_worktree(&source.git_dir, &path).await;
            }
            add_detached_worktree(&source.git_dir, &path, &rev).await?;
            Some(source.git_dir)
        } else {
            init_detached_scratch(&path).await?;
            None
        };
        Ok(Self {
            path,
            source_git_dir,
        })
    }

    /// True when `git status --porcelain` shows anything besides the injected `.agent` context.
    /// Removes the worktree either way. Does not commit or push.
    pub async fn discard(self) -> bool {
        let dirty = scratch_has_changes(&self.path).await;
        self.remove().await;
        dirty
    }

    async fn remove(self) {
        if let Some(git_dir) = &self.source_git_dir {
            if let Err(err) = remove_linked_worktree(git_dir, &self.path).await {
                tracing::warn!(
                    error = %err,
                    path = %self.path.display(),
                    "failed to remove plan scratch worktree"
                );
            }
        }
        if self.path.exists() {
            if let Err(err) = tokio::fs::remove_dir_all(&self.path).await {
                tracing::warn!(
                    error = %err,
                    path = %self.path.display(),
                    "failed to delete plan scratch directory"
                );
            }
        }
    }
}

async fn plan_base_rev(
    git_dir: &Path,
    ticket_branch: &str,
    fallback_branch: &str,
) -> Result<String, WorktreeError> {
    if local_branch_exists(git_dir, ticket_branch).await? {
        return git_ref_sha(git_dir, ticket_branch).await;
    }
    if local_branch_exists(git_dir, "main").await? {
        return git_ref_sha(git_dir, "main").await;
    }
    if fallback_branch != "main" && local_branch_exists(git_dir, fallback_branch).await? {
        return git_ref_sha(git_dir, fallback_branch).await;
    }
    Err(WorktreeError::GitCommandFailed {
        command: "git rev-parse".into(),
        stderr: format!("no plan base: missing `{ticket_branch}` and `main`"),
    })
}

async fn local_branch_exists(git_dir: &Path, branch: &str) -> Result<bool, WorktreeError> {
    let spec = format!("refs/heads/{branch}");
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(["show-ref", "--verify", "--quiet", &spec])
        .output()
        .await
        .map_err(WorktreeError::from)?;
    Ok(output.status.success())
}

async fn add_detached_worktree(
    git_dir: &Path,
    worktree: &Path,
    rev: &str,
) -> Result<(), WorktreeError> {
    let path = path_to_string(worktree)?;
    run_git_in(git_dir, &["worktree", "add", "--detach", &path, rev]).await
}

async fn remove_linked_worktree(git_dir: &Path, worktree: &Path) -> Result<(), WorktreeError> {
    let path = path_to_string(worktree)?;
    let removed = run_git_in(git_dir, &["worktree", "remove", "--force", &path]).await;
    if worktree.exists() {
        tokio::fs::remove_dir_all(worktree).await?;
    }
    let _ = run_git_in(git_dir, &["worktree", "prune"]).await;
    removed
}

async fn init_detached_scratch(path: &Path) -> Result<(), WorktreeError> {
    if path.exists() {
        tokio::fs::remove_dir_all(path).await?;
    }
    tokio::fs::create_dir_all(path).await?;
    run_git_in(path, &["init", "-b", "main"]).await?;
    run_git_in(
        path,
        &[
            "-c",
            "user.name=Coppice",
            "-c",
            "user.email=coppice@localhost",
            "commit",
            "--allow-empty",
            "-m",
            "plan scratch",
        ],
    )
    .await?;
    run_git_in(path, &["checkout", "--detach"]).await
}

async fn scratch_has_changes(worktree: &Path) -> bool {
    let output = match tokio::process::Command::new("git")
        .current_dir(worktree)
        .args(["status", "--porcelain"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => output,
        _ => return false,
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| !porcelain_line_is_agent_context(line))
}

fn porcelain_line_is_agent_context(line: &str) -> bool {
    let path = line.get(3..).unwrap_or("").trim();
    let path = path.rsplit(" -> ").next().unwrap_or(path).trim_matches('"');
    path == ".agent" || path.starts_with(".agent/")
}

fn path_to_string(path: &Path) -> Result<String, WorktreeError> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        WorktreeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("path is not valid UTF-8: {}", path.display()),
        ))
    })
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
                format!("repository not accessible at {}: {err}", git_dir.display()),
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
            Some(&GitAuthor {
                name: "Test".into(),
                email: "test@example.com".into(),
            }),
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

    fn git_sha(dir: &Path, rev: &str) -> String {
        let output = std::process::Command::new("git")
            .current_dir(dir)
            .args(["rev-parse", rev])
            .output()
            .expect("rev-parse");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn init_repo(path: &Path) {
        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(path)
            .status()
            .expect("git init");
        std::process::Command::new("git")
            .current_dir(path)
            .args(["config", "user.email", "test@example.com"])
            .status()
            .expect("email");
        std::process::Command::new("git")
            .current_dir(path)
            .args(["config", "user.name", "Test"])
            .status()
            .expect("name");
        std::fs::write(path.join("README.md"), "keep\n").expect("readme");
        std::process::Command::new("git")
            .current_dir(path)
            .args(["add", "README.md"])
            .status()
            .expect("add");
        std::process::Command::new("git")
            .current_dir(path)
            .args(["commit", "-m", "initial"])
            .status()
            .expect("commit");
    }

    #[tokio::test]
    async fn plan_scratch_detaches_at_the_ticket_branch_and_discards_writes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).expect("repo");
        init_repo(&repo);
        let main_sha = git_sha(&repo, "main");
        std::process::Command::new("git")
            .current_dir(&repo)
            .args(["checkout", "-b", "agent/TICKET-abc"])
            .status()
            .expect("branch");
        std::fs::write(repo.join("feature.txt"), "feature\n").expect("feature");
        std::process::Command::new("git")
            .current_dir(&repo)
            .args(["add", "feature.txt"])
            .status()
            .expect("add feature");
        std::process::Command::new("git")
            .current_dir(&repo)
            .args(["commit", "-m", "feature"])
            .status()
            .expect("commit feature");
        let branch_sha = git_sha(&repo, "agent/TICKET-abc");
        assert_ne!(branch_sha, main_sha);

        let worktrees = tmp.path().join("worktrees");
        let run_id = Uuid::from_u128(7);
        let scratch = PlanScratch::create(
            &worktrees,
            run_id,
            Some(PlanScratchSource {
                git_dir: repo.clone(),
                ticket_branch: "agent/TICKET-abc".into(),
                fallback_branch: "main".into(),
            }),
        )
        .await
        .expect("scratch");
        let scratch_path = scratch.path().to_path_buf();
        assert_ne!(scratch_path, repo);
        assert!(!scratch_path.starts_with(&repo));
        assert_eq!(git_sha(&scratch_path, "HEAD"), branch_sha);
        let head_name = std::process::Command::new("git")
            .current_dir(&scratch_path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .expect("abbrev-ref");
        assert_eq!(String::from_utf8_lossy(&head_name.stdout).trim(), "HEAD");

        std::fs::create_dir_all(scratch_path.join(".agent")).expect("agent dir");
        std::fs::write(scratch_path.join(".agent").join("context.md"), "ctx").expect("ctx");
        assert!(!scratch_has_changes(&scratch_path).await);
        std::fs::write(scratch_path.join("leak.txt"), "nope\n").expect("leak");
        assert!(scratch_has_changes(&scratch_path).await);

        assert!(scratch.discard().await);
        assert!(!scratch_path.exists());
        assert_eq!(git_sha(&repo, "agent/TICKET-abc"), branch_sha);
        assert_eq!(git_sha(&repo, "main"), main_sha);
        assert!(!repo.join("leak.txt").exists());
        let listed = std::process::Command::new("git")
            .current_dir(&repo)
            .args(["worktree", "list"])
            .output()
            .expect("worktree list");
        assert!(!String::from_utf8_lossy(&listed.stdout).contains("plan-scratch"));
    }

    #[tokio::test]
    async fn plan_scratch_detaches_at_main_when_the_ticket_branch_is_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).expect("repo");
        init_repo(&repo);
        let main_sha = git_sha(&repo, "main");
        let worktrees = tmp.path().join("worktrees");
        let scratch = PlanScratch::create(
            &worktrees,
            Uuid::from_u128(8),
            Some(PlanScratchSource {
                git_dir: repo.clone(),
                ticket_branch: "agent/TICKET-missing".into(),
                fallback_branch: "main".into(),
            }),
        )
        .await
        .expect("scratch");
        assert_eq!(git_sha(scratch.path(), "HEAD"), main_sha);
        let path = scratch.path().to_path_buf();
        assert!(!scratch.discard().await);
        assert!(!path.exists());
        assert_eq!(git_sha(&repo, "main"), main_sha);
    }

    #[test]
    fn porcelain_ignores_only_the_injected_agent_context() {
        assert!(porcelain_line_is_agent_context("?? .agent/context.md"));
        assert!(!porcelain_line_is_agent_context("?? leak.txt"));
        assert!(!porcelain_line_is_agent_context(" M README.md"));
    }
}
