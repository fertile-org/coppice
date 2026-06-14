use std::path::{Path, PathBuf};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::slug::slugify;
use crate::services::repo_service::{RepoError, RepoService};
use crate::services::ticket_git_service::list_local_branches;

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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSummary {
    pub path: String,
    pub branch: String,
    pub head_sha: String,
    pub ticket_id: Option<Uuid>,
    pub ticket_title: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchesResponse {
    pub default_branch: String,
    pub branches: Vec<String>,
}

pub struct CodeReviewService<'a> {
    pool: &'a PgPool,
    worktrees_root: PathBuf,
}

impl<'a> CodeReviewService<'a> {
    pub fn new(pool: &'a PgPool, worktrees_root: PathBuf) -> Self {
        Self {
            pool,
            worktrees_root,
        }
    }

    pub async fn list_worktrees(&self, repo_id: Uuid) -> Result<Vec<WorktreeSummary>, CodeReviewError> {
        let repo = RepoService::new(self.pool).get(repo_id).await.map_err(|e| match e {
            RepoError::NotFound => CodeReviewError::RepoNotFound,
            RepoError::Database(err) => CodeReviewError::Database(err),
            other => CodeReviewError::Git(other.to_string()),
        })?;
        if repo.verification_status != crate::domain::repo::VerificationStatus::Ready {
            return Err(CodeReviewError::RepoNotReady);
        }
        let repo_slug = slugify(&repo.name);
        let prefix = format!("TICKET-");
        let suffix = format!("-{repo_slug}");
        let mut out = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.worktrees_root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&prefix) || !name.ends_with(&suffix) {
                continue;
            }
            let path = entry.path();
            if !path.join(".git").exists() {
                continue;
            }
            let branch = git_in(&path, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
            let head_sha = git_in(&path, &["rev-parse", "HEAD"]).await?;
            let ticket_short = name
                .strip_prefix("TICKET-")
                .and_then(|rest| rest.strip_suffix(&suffix))
                .unwrap_or("");
            let (ticket_id, ticket_title) = self.lookup_ticket_by_short(repo_id, ticket_short).await?;
            out.push(WorktreeSummary {
                path: path.to_string_lossy().into_owned(),
                branch,
                head_sha,
                ticket_id,
                ticket_title,
            });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    async fn lookup_ticket_by_short(
        &self,
        repo_id: Uuid,
        ticket_short: &str,
    ) -> Result<(Option<Uuid>, Option<String>), CodeReviewError> {
        if ticket_short.is_empty() {
            return Ok((None, None));
        }
        let row = sqlx::query(
            r#"
            SELECT id, title
            FROM tickets
            WHERE repo_id = $1 AND CAST(id AS text) LIKE $2
            LIMIT 1
            "#,
        )
        .bind(repo_id)
        .bind(format!("{ticket_short}%"))
        .fetch_optional(self.pool)
        .await?;
        Ok(match row {
            Some(row) => (Some(row.get("id")), Some(row.get("title"))),
            None => (None, None),
        })
    }

    pub async fn list_branches(&self, repo_id: Uuid) -> Result<BranchesResponse, CodeReviewError> {
        let repo = RepoService::new(self.pool).get(repo_id).await.map_err(|e| match e {
            RepoError::NotFound => CodeReviewError::RepoNotFound,
            RepoError::Database(err) => CodeReviewError::Database(err),
            other => CodeReviewError::Git(other.to_string()),
        })?;
        if repo.verification_status != crate::domain::repo::VerificationStatus::Ready {
            return Err(CodeReviewError::RepoNotReady);
        }
        let git_dir = PathBuf::from(&repo.local_path);
        let branches = list_local_branches(&git_dir)
            .await
            .map_err(|e| CodeReviewError::Git(e.to_string()))?;
        Ok(BranchesResponse {
            default_branch: repo.default_branch,
            branches,
        })
    }
}

async fn git_in(worktree: &Path, args: &[&str]) -> Result<String, CodeReviewError> {
    let output = tokio::process::Command::new("git")
        .current_dir(worktree)
        .args(args)
        .output()
        .await?;
    if !output.status.success() {
        return Err(CodeReviewError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
