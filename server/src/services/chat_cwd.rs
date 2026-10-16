use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Working directory for a chat turn.
///
/// Every connector, including ones whose read-only mode was verified, uses
/// `$WORKTREES_PATH/chat/{session_id}/`. Repository files are copied into that
/// folder by [`crate::services::chat_repos`]. This never returns the registered
/// checkout and never attaches to a `TICKET-*` worktree.
pub fn resolve_chat_cwd(worktrees_root: &Path, session_id: Uuid) -> std::io::Result<PathBuf> {
    let path = worktrees_root.join("chat").join(session_id.to_string());
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
    fn chat_cwd_is_the_session_folder() {
        let root = tempdir().unwrap();
        let session = Uuid::new_v4();
        let cwd = resolve_chat_cwd(root.path(), session).unwrap();
        assert_eq!(cwd, root.path().join("chat").join(session.to_string()));
        assert!(cwd.is_dir());
        assert!(!cwd.to_string_lossy().contains("TICKET-"));
        assert_ne!(cwd, PathBuf::from("/"));
    }

    #[test]
    fn rejects_a_worktree_root_that_would_resolve_to_slash() {
        let err = validate_cwd(Path::new("/")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
}
