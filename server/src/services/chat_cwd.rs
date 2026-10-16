use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

/// Resolve the working directory for a chat turn.
///
/// - Verified read-only connector with a bound repo → registered `local_path`
/// - Any other connector with a bound repo → `git clone --local --no-hardlinks`
///   of that repo at the current HEAD, with every remote removed
/// - Unbound → `$WORKTREES_PATH/chat/{session_id}/` (created if missing)
///
/// Never returns `/` and never attaches to `TICKET-*` worktrees. An isolated
/// clone is never the registered checkout.
pub fn resolve_chat_cwd(
    worktrees_root: &Path,
    session_id: Uuid,
    bound_repo_local_path: Option<&str>,
    isolate: bool,
) -> std::io::Result<PathBuf> {
    if let Some(local_path) = bound_repo_local_path {
        let path = PathBuf::from(local_path);
        validate_cwd(&path)?;
        if isolate {
            return prepare_isolated_clone(worktrees_root, session_id, &path);
        }
        return Ok(path);
    }

    let path = worktrees_root.join("chat").join(session_id.to_string());
    validate_cwd(&path)?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Directory of the throwaway clone for one chat, under the worktrees root.
pub fn chat_clone_dir(worktrees_root: &Path, session_id: Uuid) -> PathBuf {
    worktrees_root
        .join("chat-clones")
        .join(session_id.to_string())
}

/// Deletes one chat's clone. Missing directories are fine.
pub fn remove_chat_clone(worktrees_root: &Path, session_id: Uuid) -> std::io::Result<()> {
    let path = chat_clone_dir(worktrees_root, session_id);
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Deletes every chat clone. Called when the server starts.
pub fn sweep_chat_clones(worktrees_root: &Path) -> std::io::Result<()> {
    let path = worktrees_root.join("chat-clones");
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn prepare_isolated_clone(
    worktrees_root: &Path,
    session_id: Uuid,
    repo: &Path,
) -> std::io::Result<PathBuf> {
    let dest = chat_clone_dir(worktrees_root, session_id);
    validate_cwd(&dest)?;
    if isolated_clone_ready(&dest) {
        return Ok(dest);
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let source_head = git_stdout(Some(repo), &["rev-parse", "HEAD"])?;
    let src = repo.display().to_string();
    let dst = dest.display().to_string();
    git_stdout(
        None,
        &["clone", "--local", "--no-hardlinks", &src, &dst],
    )?;
    let cloned_head = git_stdout(Some(&dest), &["rev-parse", "HEAD"])?;
    if cloned_head != source_head {
        git_stdout(Some(&dest), &["checkout", "--detach", &source_head])?;
    }
    remove_all_remotes(&dest)?;
    Ok(dest)
}

fn isolated_clone_ready(dest: &Path) -> bool {
    if !dest.join(".git").is_dir() {
        return false;
    }
    matches!(git_stdout(Some(dest), &["remote"]), Ok(remotes) if remotes.is_empty())
}

fn remove_all_remotes(repo: &Path) -> std::io::Result<()> {
    let listed = git_stdout(Some(repo), &["remote"])?;
    for name in listed.split_whitespace() {
        git_stdout(Some(repo), &["remote", "remove", name])?;
    }
    let left = git_stdout(Some(repo), &["remote"])?;
    if !left.is_empty() {
        return Err(std::io::Error::other(format!(
            "temporary chat clone still has remotes: {left}"
        )));
    }
    Ok(())
}

fn git_stdout(cwd: Option<&Path>, args: &[&str]) -> std::io::Result<String> {
    let mut command = Command::new("git");
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().map_err(|err| {
        std::io::Error::other(format!("failed to run git {}: {err}", args.join(" ")))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::other(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn validate_cwd(path: &Path) -> std::io::Result<()> {
    if path.as_os_str().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "chat cwd must not be empty",
        ));
    }
    if path == Path::new("/") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "chat cwd must not be /",
        ));
    }
    let lossy = path.to_string_lossy();
    if lossy.contains("/TICKET-") || lossy.starts_with("TICKET-") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "chat cwd must not use a ticket worktree",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use std::process::Command;
    use tempfile::tempdir;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Coppice Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Coppice Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .expect("git");
        assert!(status.success(), "git {} failed", args.join(" "));
    }

    fn loose_object(repo: &Path, sha: &str) -> PathBuf {
        repo.join(".git")
            .join("objects")
            .join(&sha[..2])
            .join(&sha[2..])
    }

    fn init_repo_at_detached_head(root: &Path) -> (PathBuf, String) {
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        std::fs::write(repo.join("README.md"), "one\n").unwrap();
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-m", "one"]);
        std::fs::write(repo.join("README.md"), "two\n").unwrap();
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-m", "two"]);
        git(
            &repo,
            &["remote", "add", "origin", "https://example.com/repo.git"],
        );
        git(
            &repo,
            &[
                "remote",
                "add",
                "upstream",
                "https://example.com/upstream.git",
            ],
        );
        git(&repo, &["checkout", "--detach", "HEAD~1"]);
        let head = git_stdout(Some(&repo), &["rev-parse", "HEAD"]).unwrap();
        (repo, head)
    }

    #[test]
    fn bound_repo_uses_local_path_when_read_only_is_verified() {
        let root = tempdir().unwrap();
        let session = Uuid::new_v4();
        let cwd = resolve_chat_cwd(root.path(), session, Some("/repos/demo"), false).unwrap();
        assert_eq!(cwd, PathBuf::from("/repos/demo"));
        assert!(!root.path().join("chat-clones").exists());
    }

    #[test]
    fn unbound_creates_under_worktrees_chat_session() {
        let root = tempdir().unwrap();
        let session = Uuid::new_v4();
        let cwd = resolve_chat_cwd(root.path(), session, None, true).unwrap();
        assert_eq!(cwd, root.path().join("chat").join(session.to_string()));
        assert!(cwd.is_dir());
        assert!(!cwd.to_string_lossy().contains("TICKET-"));
        assert_ne!(cwd, PathBuf::from("/"));
    }

    #[test]
    fn rejects_root_path() {
        let root = tempdir().unwrap();
        let err = resolve_chat_cwd(root.path(), Uuid::new_v4(), Some("/"), true).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn isolated_clone_matches_head_and_has_no_remotes() {
        let root = tempdir().unwrap();
        let (repo, head) = init_repo_at_detached_head(root.path());
        let session = Uuid::new_v4();

        let cwd =
            resolve_chat_cwd(root.path(), session, Some(repo.to_str().unwrap()), true).unwrap();

        assert_ne!(cwd, repo);
        assert_eq!(cwd, chat_clone_dir(root.path(), session));
        assert_eq!(
            git_stdout(Some(&cwd), &["rev-parse", "HEAD"]).unwrap(),
            head
        );
        assert_eq!(
            git_stdout(Some(&cwd), &["remote"]).unwrap(),
            "",
            "clone must have no remotes"
        );
        assert!(git_stdout(Some(&cwd), &["remote", "get-url", "origin"]).is_err());
        let source_remotes = git_stdout(Some(&repo), &["remote"]).unwrap();
        assert!(source_remotes.contains("origin"));
        assert!(source_remotes.contains("upstream"));
        assert_eq!(
            std::fs::read_to_string(cwd.join("README.md")).unwrap(),
            "one\n"
        );
        let blob = git_stdout(Some(&repo), &["rev-parse", "HEAD:README.md"]).unwrap();
        let src_object = loose_object(&repo, &blob);
        let dst_object = loose_object(&cwd, &blob);
        let src_meta = std::fs::metadata(&src_object).unwrap();
        let dst_meta = std::fs::metadata(&dst_object).unwrap();
        assert_ne!(
            src_meta.ino(),
            dst_meta.ino(),
            "clone objects must not be hardlinked to the checkout"
        );

        let again =
            resolve_chat_cwd(root.path(), session, Some(repo.to_str().unwrap()), true).unwrap();
        assert_eq!(again, cwd);

        git(
            &cwd,
            &["remote", "add", "origin", "https://example.com/leaked.git"],
        );
        let recreated =
            resolve_chat_cwd(root.path(), session, Some(repo.to_str().unwrap()), true).unwrap();
        assert_eq!(git_stdout(Some(&recreated), &["remote"]).unwrap(), "");

        remove_chat_clone(root.path(), session).unwrap();
        assert!(!recreated.exists());

        let other = Uuid::new_v4();
        resolve_chat_cwd(root.path(), other, Some(repo.to_str().unwrap()), true).unwrap();
        sweep_chat_clones(root.path()).unwrap();
        assert!(!root.path().join("chat-clones").exists());
    }
}
