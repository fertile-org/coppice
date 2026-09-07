use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Resolve the working directory for a chat turn.
///
/// - Bound repo → registered `local_path` (read-oriented via write denial)
/// - Unbound → `$WORKTREES_PATH/chat/{session_id}/` (created if missing)
///
/// Never returns `/` and never attaches to `TICKET-*` worktrees.
pub fn resolve_chat_cwd(
    worktrees_root: &Path,
    session_id: Uuid,
    bound_repo_local_path: Option<&str>,
) -> std::io::Result<PathBuf> {
    if let Some(local_path) = bound_repo_local_path {
        let path = PathBuf::from(local_path);
        validate_cwd(&path)?;
        return Ok(path);
    }

    let path = worktrees_root
        .join("chat")
        .join(session_id.to_string());
    validate_cwd(&path)?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
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
    use tempfile::tempdir;

    #[test]
    fn bound_repo_uses_local_path() {
        let root = tempdir().unwrap();
        let session = Uuid::new_v4();
        let cwd = resolve_chat_cwd(root.path(), session, Some("/repos/demo")).unwrap();
        assert_eq!(cwd, PathBuf::from("/repos/demo"));
    }

    #[test]
    fn unbound_creates_under_worktrees_chat_session() {
        let root = tempdir().unwrap();
        let session = Uuid::new_v4();
        let cwd = resolve_chat_cwd(root.path(), session, None).unwrap();
        assert_eq!(cwd, root.path().join("chat").join(session.to_string()));
        assert!(cwd.is_dir());
        assert!(!cwd.to_string_lossy().contains("TICKET-"));
        assert_ne!(cwd, PathBuf::from("/"));
    }

    #[test]
    fn rejects_root_path() {
        let root = tempdir().unwrap();
        let err = resolve_chat_cwd(root.path(), Uuid::new_v4(), Some("/")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
}
