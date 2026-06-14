use std::path::{Path, PathBuf};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::slug::slugify;
use crate::domain::substatus::TicketStatus;
use crate::services::comment_service::CommentService;
use crate::services::repo_service::{RepoError, RepoService};
use crate::services::ticket_git_service::{list_local_branches, validate_branch_name};
use crate::services::ticket_service::TicketService;

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
    #[error("validation error: {0}")]
    Validation(String),
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileSummary {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    pub base_branch: String,
    pub base_sha: String,
    pub head_sha: String,
    pub head_branch: String,
    pub files: Vec<DiffFileSummary>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePatch {
    pub path: String,
    pub patch: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTicketInput {
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitReviewInput {
    pub repo_id: Uuid,
    pub worktree_path: String,
    pub base_branch: String,
    pub head_sha: String,
    pub ticket_id: Option<Uuid>,
    pub new_ticket: Option<NewTicketInput>,
    pub summary: String,
    pub inline_comments: Vec<InlineCommentInput>,
    pub workflow_action: Option<String>,
    pub reassign_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitReviewResponse {
    pub ticket_id: Uuid,
    pub comment_id: Uuid,
    pub ticket_created: bool,
}

struct WorktreeRepoContext {
    worktree: PathBuf,
    base_branch: String,
    base_sha: String,
    head_sha: String,
    head_branch: String,
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

    pub async fn diff_summary(
        &self,
        repo_id: Uuid,
        worktree_path: &str,
        base_branch: &str,
    ) -> Result<DiffSummary, CodeReviewError> {
        let ctx = self
            .resolve_worktree_repo(repo_id, worktree_path, base_branch)
            .await?;
        let range = format!("{}...HEAD", ctx.base_branch);
        let name_status = git_in(&ctx.worktree, &["diff", "--name-status", &range]).await?;
        let numstat = git_in(&ctx.worktree, &["diff", "--numstat", &range]).await?;
        let files = parse_diff_files(&name_status, &numstat);
        Ok(DiffSummary {
            base_branch: ctx.base_branch,
            base_sha: ctx.base_sha,
            head_sha: ctx.head_sha,
            head_branch: ctx.head_branch,
            files,
        })
    }

    pub async fn file_patch(
        &self,
        repo_id: Uuid,
        worktree_path: &str,
        base_branch: &str,
        path: &str,
    ) -> Result<FilePatch, CodeReviewError> {
        validate_diff_file_path(path)?;
        let ctx = self
            .resolve_worktree_repo(repo_id, worktree_path, base_branch)
            .await?;
        let range = format!("{}...HEAD", ctx.base_branch);
        let patch = git_in(&ctx.worktree, &["diff", &range, "--", path]).await?;
        if patch.len() > MAX_PATCH_BYTES {
            return Err(CodeReviewError::PatchTooLarge);
        }
        Ok(FilePatch {
            path: path.to_string(),
            patch,
        })
    }

    pub async fn submit_review(
        &self,
        user_id: Uuid,
        input: SubmitReviewInput,
    ) -> Result<SubmitReviewResponse, CodeReviewError> {
        if input.summary.trim().is_empty() {
            return Err(CodeReviewError::Validation("summary is required".into()));
        }

        let repo = RepoService::new(self.pool).get(input.repo_id).await.map_err(|e| match e {
            RepoError::NotFound => CodeReviewError::RepoNotFound,
            RepoError::Database(err) => CodeReviewError::Database(err),
            other => CodeReviewError::Git(other.to_string()),
        })?;
        if repo.verification_status != crate::domain::repo::VerificationStatus::Ready {
            return Err(CodeReviewError::RepoNotReady);
        }

        let ctx = self
            .resolve_worktree_repo(input.repo_id, &input.worktree_path, &input.base_branch)
            .await?;
        if ctx.head_sha != input.head_sha {
            return Err(CodeReviewError::Git(
                "worktree HEAD changed; refresh diff before submitting".into(),
            ));
        }

        let ticket_service = TicketService::new(self.pool);
        let mut ticket_created = false;
        let ticket_id = if let Some(ticket_id) = input.ticket_id {
            let ticket = ticket_service.get(ticket_id).await?;
            if ticket.ticket.repo_id != Some(input.repo_id) {
                return Err(CodeReviewError::TicketRepoMismatch);
            }
            ticket_id
        } else {
            let new_ticket = input.new_ticket.ok_or_else(|| {
                CodeReviewError::Validation(
                    "new_ticket is required when ticket_id is omitted".into(),
                )
            })?;
            let created = ticket_service
                .create(
                    new_ticket.project_id,
                    new_ticket.title.trim(),
                    new_ticket.description.as_deref().unwrap_or(""),
                    Some(input.repo_id),
                    None,
                    "human",
                    user_id,
                )
                .await?;
            ticket_created = true;
            created.ticket.id
        };

        for comment in &input.inline_comments {
            validate_diff_file_path(&comment.path)?;
        }

        let body = format_review_comment(
            &repo.name,
            &input.worktree_path,
            &input.base_branch,
            &ctx.head_branch,
            &ctx.head_sha,
            input.summary.trim(),
            &input.inline_comments,
        );

        let comment = CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::Human,
                Some(user_id),
                &body,
                CommentIntent::ReviewFeedback,
                &[],
                &[],
            )
            .await?;

        match input.workflow_action.as_deref() {
            Some("move_to_in_progress") => {
                ticket_service
                    .update_status(ticket_id, TicketStatus::InProgress, None, None)
                    .await?;
            }
            Some("reassign_engineer") => {
                if let Some(agent_id) = input.reassign_agent_id {
                    ticket_service
                        .assign_agent(ticket_id, Some(agent_id))
                        .await?;
                }
            }
            _ => {}
        }

        Ok(SubmitReviewResponse {
            ticket_id,
            comment_id: comment.id,
            ticket_created,
        })
    }

    async fn resolve_worktree_repo(
        &self,
        repo_id: Uuid,
        worktree_path: &str,
        base_branch: &str,
    ) -> Result<WorktreeRepoContext, CodeReviewError> {
        validate_branch_name(base_branch).map_err(|_| CodeReviewError::InvalidBranchName)?;

        let repo = RepoService::new(self.pool).get(repo_id).await.map_err(|e| match e {
            RepoError::NotFound => CodeReviewError::RepoNotFound,
            RepoError::Database(err) => CodeReviewError::Database(err),
            other => CodeReviewError::Git(other.to_string()),
        })?;
        if repo.verification_status != crate::domain::repo::VerificationStatus::Ready {
            return Err(CodeReviewError::RepoNotReady);
        }

        let worktree = validate_worktree_path(&self.worktrees_root, Path::new(worktree_path))?;
        verify_worktree_belongs_to_repo(&worktree, Path::new(&repo.local_path)).await?;

        let repo_path = PathBuf::from(&repo.local_path);
        let base_sha = git_in(&repo_path, &["rev-parse", base_branch]).await?;
        let head_sha = git_in(&worktree, &["rev-parse", "HEAD"]).await?;
        let head_branch = git_in(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;

        Ok(WorktreeRepoContext {
            worktree,
            base_branch: base_branch.to_string(),
            base_sha,
            head_sha,
            head_branch,
        })
    }
}

async fn verify_worktree_belongs_to_repo(
    worktree: &Path,
    repo_path: &Path,
) -> Result<(), CodeReviewError> {
    let worktree_common = resolve_git_common_dir(worktree).await?;
    let repo_common = resolve_git_common_dir(repo_path).await?;
    if worktree_common != repo_common {
        return Err(CodeReviewError::InvalidWorktreePath);
    }
    Ok(())
}

async fn resolve_git_common_dir(git_dir: &Path) -> Result<PathBuf, CodeReviewError> {
    let common_dir = git_in(git_dir, &["rev-parse", "--git-common-dir"]).await?;
    let path = PathBuf::from(&common_dir);
    let absolute = if path.is_absolute() {
        path
    } else {
        git_dir.join(path)
    };
    absolute
        .canonicalize()
        .map_err(|_| CodeReviewError::InvalidWorktreePath)
}

fn parse_diff_files(name_status: &str, numstat: &str) -> Vec<DiffFileSummary> {
    let mut stats: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
    for line in numstat.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let additions = parts
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let deletions = parts
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if let Some(path) = parts.next() {
            stats.insert(path.to_string(), (additions, deletions));
        }
    }

    let mut files = Vec::new();
    for line in name_status.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let status_code = parts.next().unwrap_or("");
        let status = match status_code.chars().next() {
            Some('A') => "added",
            Some('M') => "modified",
            Some('D') => "deleted",
            Some('R') => "renamed",
            _ => "modified",
        };
        let path = if status == "renamed" {
            parts.nth(1).unwrap_or("").to_string()
        } else {
            parts.next().unwrap_or("").to_string()
        };
        if path.is_empty() {
            continue;
        }
        let (additions, deletions) = stats.get(&path).copied().unwrap_or((0, 0));
        files.push(DiffFileSummary {
            path,
            status: status.to_string(),
            additions,
            deletions,
        });
    }
    files
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
    fn parse_diff_files_merges_name_status_and_numstat() {
        let files = parse_diff_files(
            "M\tsrc/a.ts\nA\tsrc/b.ts",
            "3\t1\tsrc/a.ts\n10\t0\tsrc/b.ts",
        );
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/a.ts");
        assert_eq!(files[0].status, "modified");
        assert_eq!(files[0].additions, 3);
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
