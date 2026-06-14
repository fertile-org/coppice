use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum CodeReviewError {
    #[error("repository not found")]
    RepoNotFound,
    #[error("repository is not ready")]
    RepoNotReady,
    #[error("invalid worktree path")]
    InvalidWorktreePath,
    #[error("invalid file path")]
    InvalidFilePath,
    #[error("invalid branch name")]
    InvalidBranchName,
    #[error("patch too large")]
    PatchTooLarge,
    #[error("ticket not found")]
    TicketNotFound,
    #[error("ticket repo mismatch")]
    TicketRepoMismatch,
    #[error("git error: {0}")]
    Git(String),
    #[error(transparent)]
    Ticket(#[from] crate::services::ticket_service::TicketError),
    #[error(transparent)]
    Comment(#[from] crate::services::comment_service::CommentError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub const MAX_PATCH_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineCommentInput {
    pub path: String,
    pub line: u32,
    pub side: String,
    pub body: String,
}

pub fn validate_worktree_path(worktrees_root: &Path, worktree_path: &Path) -> Result<PathBuf, CodeReviewError> {
    let canonical_root = worktrees_root
        .canonicalize()
        .map_err(|_| CodeReviewError::InvalidWorktreePath)?;
    let canonical_worktree = worktree_path
        .canonicalize()
        .map_err(|_| CodeReviewError::InvalidWorktreePath)?;
    if !canonical_worktree.starts_with(&canonical_root) {
        return Err(CodeReviewError::InvalidWorktreePath);
    }
    if !canonical_worktree.join(".git").exists() {
        return Err(CodeReviewError::InvalidWorktreePath);
    }
    Ok(canonical_worktree)
}

pub fn validate_diff_file_path(path: &str) -> Result<(), CodeReviewError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.split('/').any(|seg| seg == "..")
    {
        return Err(CodeReviewError::InvalidFilePath);
    }
    Ok(())
}

pub fn format_review_comment(
    repo_name: &str,
    worktree_path: &str,
    base_branch: &str,
    head_branch: &str,
    head_sha: &str,
    summary: &str,
    inline_comments: &[InlineCommentInput],
) -> String {
    let mut body = format!(
        "## Code review\n\n**Repo:** {repo_name} · **Worktree:** {worktree_path} · **Compare:** {base_branch} → {head_branch} ({head_sha})\n\n### Summary\n{summary}\n"
    );
    if inline_comments.is_empty() {
        return body;
    }
    body.push_str("\n### Inline comments\n");
    let mut by_path: std::collections::BTreeMap<&str, Vec<&InlineCommentInput>> =
        std::collections::BTreeMap::new();
    for comment in inline_comments {
        by_path.entry(comment.path.as_str()).or_default().push(comment);
    }
    for (path, comments) in by_path {
        body.push_str(&format!("\n#### `{path}`\n"));
        for comment in comments {
            body.push_str(&format!(
                "- **L{}** ({}): {}\n",
                comment.line, comment.side, comment.body.trim()
            ));
        }
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn validate_diff_file_path_rejects_traversal() {
        assert!(validate_diff_file_path("../etc/passwd").is_err());
        assert!(validate_diff_file_path("/abs/path").is_err());
        assert!(validate_diff_file_path("src/foo.rs").is_ok());
    }

    #[test]
    fn format_review_comment_groups_by_file() {
        let body = format_review_comment(
            "coppice",
            "/data/worktrees/TICKET-abc-coppice",
            "main",
            "agent/TICKET-abc",
            "abc1234",
            "Looks good overall.",
            &[
                InlineCommentInput {
                    path: "src/a.ts".into(),
                    line: 10,
                    side: "new".into(),
                    body: "Fix this".into(),
                },
                InlineCommentInput {
                    path: "src/b.rs".into(),
                    line: 3,
                    side: "old".into(),
                    body: "Rename".into(),
                },
            ],
        );
        assert!(body.contains("### Summary\nLooks good overall."));
        assert!(body.contains("#### `src/a.ts`"));
        assert!(body.contains("**L10** (new): Fix this"));
        assert!(body.contains("#### `src/b.rs`"));
    }

    #[test]
    fn validate_worktree_path_requires_under_root() {
        let root = tempfile::tempdir().unwrap();
        let inside = root.path().join("TICKET-abc-repo");
        fs::create_dir_all(inside.join(".git")).unwrap();
        assert!(validate_worktree_path(root.path(), &inside).is_ok());
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(outside.path().join(".git")).unwrap();
        assert!(validate_worktree_path(root.path(), outside.path()).is_err());
    }
}
