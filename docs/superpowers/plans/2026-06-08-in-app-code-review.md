# In-App Code Review — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dedicated `/code` page for PR-style worktree diff review with inline line comments and submit-to-ticket flow, reachable from the ticket metadata sidebar and Repositories page.

**Architecture:** Server-side git diff via new `CodeReviewService` (read-only git commands, path validation). Repo-scoped GET endpoints for worktrees/branches/diff; single POST submit that creates tickets when needed and posts one `review_feedback` comment. Frontend uses `react-diff-view` + `gitdiff-parser` with lazy per-file patch loading.

**Tech Stack:** Rust (Axum, SQLx, tokio::process), React 19, TanStack Query, `react-diff-view`, `gitdiff-parser`, `highlight.js`

**Design spec:** [docs/superpowers/specs/2026-06-08-in-app-code-review-design.md](../specs/2026-06-08-in-app-code-review-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/src/services/code_review_service.rs` | Path validation, worktree scan, diff, review markdown formatting, submit orchestration |
| `server/src/services/ticket_git_service.rs` | Export `validate_branch_name`, `list_local_branches` as `pub(crate)` |
| `server/src/api/code_reviews.rs` | `POST /api/code-reviews/submit` |
| `server/src/api/repos.rs` | Extend with worktrees/branches/diff routes |
| `server/src/api/mod.rs` | Register `code_reviews` routes |
| `server/src/services/mod.rs` | Export `code_review_service` |
| `server/tests/integration_code_review.rs` | End-to-end API tests |
| `server/tests/common/mod.rs` | `setup_repo_with_worktree_diff` test helper |
| `web/src/features/code/CodeReviewPage.tsx` | Full-page layout, toolbar, state wiring |
| `web/src/features/code/ChangedFilesPanel.tsx` | File list with status badges |
| `web/src/features/code/DiffViewer.tsx` | react-diff-view + inline comment widgets |
| `web/src/features/code/SubmitReviewDialog.tsx` | Submit form (summary, ticket create, workflow action) |
| `web/src/features/code/formatReviewPreview.ts` | Client preview mirror of server markdown formatter |
| `web/src/features/code/useCodeReview.ts` | TanStack Query hooks for all code-review APIs |
| `web/src/lib/schemas/codeReview.ts` | Zod schemas for API types |
| `web/src/App.tsx` | Add `/code` route |
| `web/src/features/tickets/TicketMetadataPanel.tsx` | "Review code" button |
| `web/src/features/repos/RepositoriesPage.tsx` | "View code" per repo row |
| `web/src/features/code/formatReviewPreview.test.ts` | Vitest for preview formatter |

---

### Task 1: CodeReviewService — validation and review formatting

**Files:**
- Create: `server/src/services/code_review_service.rs`
- Modify: `server/src/services/mod.rs`
- Modify: `server/src/services/ticket_git_service.rs` (export shared git helpers)

- [ ] **Step 1: Export shared git helpers from ticket_git_service**

In `server/src/services/ticket_git_service.rs`, change:

```rust
fn validate_branch_name(branch: &str) -> Result<(), TicketGitError> {
async fn list_local_branches(git_dir: &Path) -> Result<Vec<String>, TicketGitError> {
```

To:

```rust
pub(crate) fn validate_branch_name(branch: &str) -> Result<(), TicketGitError> {
pub(crate) async fn list_local_branches(git_dir: &Path) -> Result<Vec<String>, TicketGitError> {
```

- [ ] **Step 2: Create service skeleton with error type**

Create `server/src/services/code_review_service.rs`:

```rust
use std::path::{Path, PathBuf};
use uuid::Uuid;

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
```

Add `pub mod code_review_service;` to `server/src/services/mod.rs`.

- [ ] **Step 3: Write unit tests**

Append to `code_review_service.rs`:

```rust
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server code_review_service -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/services/code_review_service.rs server/src/services/mod.rs server/src/services/ticket_git_service.rs
git commit -m "$(cat <<'EOF'
Add CodeReviewService validation and review comment formatting.

Foundation for in-app worktree diff review with path safety checks.
EOF
)"
```

---

### Task 2: Worktree listing and branches API

**Files:**
- Modify: `server/src/services/code_review_service.rs`
- Modify: `server/src/api/repos.rs`

- [ ] **Step 1: Implement worktree listing**

Add to `code_review_service.rs`:

```rust
use sqlx::{PgPool, Row};
use crate::domain::slug::slugify;
use crate::services::repo_service::{RepoError, RepoService};
use crate::services::ticket_git_service::{list_local_branches, validate_branch_name};

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
        Self { pool, worktrees_root }
    }

    pub async fn list_worktrees(&self, repo_id: Uuid) -> Result<Vec<WorktreeSummary>, CodeReviewError> {
        let repo = RepoService::new(self.pool).get(repo_id).await.map_err(|e| match e {
            RepoError::NotFound => CodeReviewError::RepoNotFound,
            other => CodeReviewError::Database(other.into()),
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
            other => CodeReviewError::Database(other.into()),
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
        return Err(CodeReviewError::Git(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
```

- [ ] **Step 2: Add repo API routes**

In `server/src/api/repos.rs`, add routes and handlers:

```rust
.route("/api/repos/{repo_id}/worktrees", get(list_repo_worktrees))
.route("/api/repos/{repo_id}/branches", get(list_repo_branches))
```

Handlers:

```rust
async fn list_repo_worktrees(
    AuthUser { .. }: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CodeReviewService::new(
        pool,
        state.config.agent.worktrees_path.clone().into(),
    );
    let worktrees = service.list_worktrees(repo_id).await.map_err(map_code_review_error)?;
    Ok(Json(serde_json::json!({ "worktrees": worktrees })))
}

async fn list_repo_branches(/* same pattern */) -> Result<Json<BranchesResponse>, StatusCode> { /* ... */ }

fn map_code_review_error(err: CodeReviewError) -> StatusCode {
    match err {
        CodeReviewError::RepoNotFound | CodeReviewError::TicketNotFound => StatusCode::NOT_FOUND,
        CodeReviewError::RepoNotReady | CodeReviewError::InvalidWorktreePath
        | CodeReviewError::InvalidFilePath | CodeReviewError::InvalidBranchName => StatusCode::BAD_REQUEST,
        CodeReviewError::PatchTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
```

- [ ] **Step 3: Run compile check**

Run: `cargo check -p coppice-server`

Expected: success

- [ ] **Step 4: Commit**

```bash
git add server/src/services/code_review_service.rs server/src/api/repos.rs
git commit -m "$(cat <<'EOF'
Add repo worktrees and branches endpoints for code review.
EOF
)"
```

---

### Task 3: Diff summary and per-file patch API

**Files:**
- Modify: `server/src/services/code_review_service.rs`
- Modify: `server/src/api/repos.rs`

- [ ] **Step 1: Add diff types and summary method**

```rust
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

impl CodeReviewService<'_> {
    pub async fn diff_summary(
        &self,
        repo_id: Uuid,
        worktree_path: &str,
        base_branch: &str,
    ) -> Result<DiffSummary, CodeReviewError> {
        let ctx = self.resolve_worktree_repo(repo_id, worktree_path, base_branch).await?;
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
        let ctx = self.resolve_worktree_repo(repo_id, worktree_path, base_branch).await?;
        let range = format!("{}...HEAD", ctx.base_branch);
        let patch = git_in(
            &ctx.worktree,
            &["diff", &range, "--", path],
        ).await?;
        if patch.len() > MAX_PATCH_BYTES {
            return Err(CodeReviewError::PatchTooLarge);
        }
        Ok(FilePatch { path: path.to_string(), patch })
    }
}
```

Implement `resolve_worktree_repo` to: load repo, validate branch name, canonicalize worktree under root, verify worktree git dir resolves to repo `local_path` via `git rev-parse --git-common-dir`, resolve `base_sha` from main repo and `head_sha`/`head_branch` from worktree.

Implement `parse_diff_files` to merge `--name-status` and `--numstat` lines into `DiffFileSummary` (`A`→`added`, `M`→`modified`, `D`→`deleted`, `R`→`renamed`).

- [ ] **Step 2: Add API routes**

```rust
.route("/api/repos/{repo_id}/diff", get(get_repo_diff))
.route("/api/repos/{repo_id}/diff/file", get(get_repo_diff_file))
```

Query structs:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiffQuery {
    worktree_path: String,
    base_branch: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiffFileQuery {
    worktree_path: String,
    base_branch: String,
    path: String,
}
```

- [ ] **Step 3: Unit test parse_diff_files**

```rust
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
```

Run: `cargo test -p coppice-server parse_diff_files -- --nocapture`

- [ ] **Step 4: Commit**

```bash
git add server/src/services/code_review_service.rs server/src/api/repos.rs
git commit -m "$(cat <<'EOF'
Add diff summary and per-file patch endpoints for code review.
EOF
)"
```

---

### Task 4: Submit review API

**Files:**
- Create: `server/src/api/code_reviews.rs`
- Modify: `server/src/services/code_review_service.rs`
- Modify: `server/src/api/mod.rs`

- [ ] **Step 1: Add submit types and service method**

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTicketInput {
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitReviewResponse {
    pub ticket_id: Uuid,
    pub comment_id: Uuid,
    pub ticket_created: bool,
}

impl CodeReviewService<'_> {
    pub async fn submit_review(
        &self,
        user_id: Uuid,
        input: SubmitReviewInput,
    ) -> Result<SubmitReviewResponse, CodeReviewError> {
        if input.summary.trim().is_empty() {
            return Err(CodeReviewError::Git("summary is required".into())); // or add Validation variant
        }
        let repo = /* load repo, ensure ready */;
        let ctx = self.resolve_worktree_repo(input.repo_id, &input.worktree_path, &input.base_branch).await?;
        if ctx.head_sha != input.head_sha {
            return Err(CodeReviewError::Git("worktree HEAD changed; refresh diff before submitting".into()));
        }

        let (ticket_id, ticket_created) = if let Some(ticket_id) = input.ticket_id {
            let ticket = TicketService::new(self.pool).get(ticket_id).await?;
            if ticket.ticket.repo_id != Some(input.repo_id) {
                return Err(CodeReviewError::TicketRepoMismatch);
            }
            (ticket_id, false)
        } else {
            let new_ticket = input.new_ticket.ok_or(/* validation error */)?;
            let created = TicketService::new(self.pool).create(
                new_ticket.project_id,
                new_ticket.title.trim(),
                new_ticket.description.as_deref().unwrap_or(""),
                Some(input.repo_id),
                None,
                "human",
                user_id,
            ).await?;
            (created.ticket.id, true)
        };

        let body = format_review_comment(
            &repo.name,
            &input.worktree_path,
            &input.base_branch,
            &ctx.head_branch,
            &ctx.head_sha,
            input.summary.trim(),
            &input.inline_comments,
        );

        let comment = CommentService::new(self.pool).create(
            ticket_id,
            AuthorType::Human,
            Some(user_id),
            &body,
            CommentIntent::ReviewFeedback,
            &[],
            &[],
        ).await?;

        match input.workflow_action.as_deref() {
            Some("move_to_in_progress") => {
                TicketService::new(self.pool)
                    .update_status(ticket_id, TicketStatus::InProgress, None, None, None)
                    .await?;
            }
            Some("reassign_engineer") => {
                if let Some(agent_id) = input.reassign_agent_id {
                    TicketService::new(self.pool).assign_agent(ticket_id, Some(agent_id)).await?;
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
}
```

Add optional `reassign_agent_id: Option<Uuid>` to `SubmitReviewInput` for the reassign workflow (frontend sends last engineer agent id or user-picked id).

- [ ] **Step 2: Create API module**

Create `server/src/api/code_reviews.rs`:

```rust
pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/code-reviews/submit", post(submit_code_review))
}

async fn submit_code_review(
    AuthUser { user_id, .. }: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SubmitReviewInput>,
) -> Result<(StatusCode, Json<SubmitReviewResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CodeReviewService::new(pool, state.config.agent.worktrees_path.clone().into());
    let result = service.submit_review(user_id, body).await.map_err(map_code_review_error)?;
    state.event_bus.publish(AppEvent::CommentCreated {
        comment_id: result.comment_id,
        ticket_id: result.ticket_id,
        author_type: "human".into(),
    });
    Ok((StatusCode::CREATED, Json(result)))
}
```

Register in `server/src/api/mod.rs`:

```rust
mod code_reviews;
// in protected router:
.merge(code_reviews::routes())
```

- [ ] **Step 3: Commit**

```bash
git add server/src/services/code_review_service.rs server/src/api/code_reviews.rs server/src/api/mod.rs
git commit -m "$(cat <<'EOF'
Add code review submit endpoint with ticket creation and workflow actions.
EOF
)"
```

---

### Task 5: Integration tests

**Files:**
- Create: `server/tests/integration_code_review.rs`
- Modify: `server/tests/common/mod.rs`

- [ ] **Step 1: Add test helper for worktree with diff**

In `server/tests/common/mod.rs`:

```rust
pub fn setup_worktree_with_commit(
    git_dir: &Path,
    worktrees_root: &Path,
    repo_name: &str,
    ticket_id: &str,
) -> PathBuf {
    use coppice_server::services::worktree_service::compute_paths;
    let ticket_uuid: uuid::Uuid = ticket_id.parse().expect("ticket uuid");
    let paths = compute_paths(worktrees_root, repo_name, ticket_uuid);
    std::fs::create_dir_all(&paths.worktree_dir).expect("worktree dir");
    Command::new("git")
        .args(["worktree", "add", "-B", &paths.branch_name, paths.worktree_dir.to_str().unwrap(), "main"])
        .current_dir(git_dir)
        .output()
        .expect("worktree add");
    std::fs::write(paths.worktree_dir.join("feature.txt"), "new feature\n").expect("write");
    Command::new("git")
        .args(["add", "feature.txt"])
        .current_dir(&paths.worktree_dir)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "feature"])
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(&paths.worktree_dir)
        .output()
        .expect("git commit");
    paths.worktree_dir
}
```

- [ ] **Step 2: Write integration tests**

Create `server/tests/integration_code_review.rs`:

```rust
mod common;

#[tokio::test]
async fn list_worktrees_and_diff_for_repo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await { return; }

    let (git_dir, local_path) = common::create_temp_git_checkout();
    let (state, env) = common::bootstrap_and_login_with_state().await;
    std::env::set_var("WORKTREES_PATH", env.worktrees.path()); // if using AgentTestEnv pattern
    // register repo, create ticket, setup_worktree_with_commit(...)
    // GET /api/repos/{repo_id}/worktrees → 200, len >= 1
    // GET /api/repos/{repo_id}/diff?worktreePath=...&baseBranch=main → files contains feature.txt
    // GET /api/repos/{repo_id}/diff/file?...&path=feature.txt → patch contains +new feature
}

#[tokio::test]
async fn submit_review_on_existing_ticket() { /* POST submit with ticketId → 201, comment in list */ }

#[tokio::test]
async fn submit_review_creates_ticket_when_missing() { /* POST with newTicket → ticketCreated true */ }

#[tokio::test]
async fn submit_review_move_to_in_progress() { /* ticket status updated */ }

#[tokio::test]
async fn reject_invalid_worktree_path() { /* 400 */ }
```

Use existing `bootstrap_and_login`, `register_test_repo`, `create_test_project`, `create_test_ticket`, `json_request` helpers from `common/mod.rs`. Wire `WORKTREES_PATH` via test state config (mirror `integration_comments` setup with `AgentTestEnv` if needed).

- [ ] **Step 3: Run integration tests**

Run: `cargo test -p coppice-server integration_code_review -- --nocapture`

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add server/tests/integration_code_review.rs server/tests/common/mod.rs
git commit -m "$(cat <<'EOF'
Add integration tests for code review diff and submit APIs.
EOF
)"
```

---

### Task 6: Frontend — dependencies, schemas, and hooks

**Files:**
- Modify: `web/package.json`
- Create: `web/src/lib/schemas/codeReview.ts`
- Create: `web/src/features/code/useCodeReview.ts`
- Create: `web/src/features/code/formatReviewPreview.ts`

- [ ] **Step 1: Install dependencies**

Run: `cd web && npm install react-diff-view gitdiff-parser && npm install -D @types/gitdiff-parser`

- [ ] **Step 2: Add Zod schemas**

Create `web/src/lib/schemas/codeReview.ts`:

```typescript
import { z } from 'zod';

export const worktreeSummarySchema = z.object({
  path: z.string(),
  branch: z.string(),
  headSha: z.string(),
  ticketId: z.string().uuid().nullable(),
  ticketTitle: z.string().nullable(),
});

export const diffFileSummarySchema = z.object({
  path: z.string(),
  status: z.string(),
  additions: z.number(),
  deletions: z.number(),
});

export const diffSummarySchema = z.object({
  baseBranch: z.string(),
  baseSha: z.string(),
  headSha: z.string(),
  headBranch: z.string(),
  files: z.array(diffFileSummarySchema),
});

export const inlineCommentSchema = z.object({
  path: z.string(),
  line: z.number(),
  side: z.enum(['old', 'new', 'delete']),
  body: z.string(),
});

export const submitReviewSchema = z.object({
  repoId: z.string().uuid(),
  worktreePath: z.string(),
  baseBranch: z.string(),
  headSha: z.string(),
  ticketId: z.string().uuid().nullable().optional(),
  newTicket: z.object({
    projectId: z.string().uuid(),
    title: z.string().min(1),
    description: z.string().optional(),
  }).nullable().optional(),
  summary: z.string().min(1),
  inlineComments: z.array(inlineCommentSchema),
  workflowAction: z.enum(['none', 'move_to_in_progress', 'reassign_engineer']).optional(),
  reassignAgentId: z.string().uuid().optional(),
});
```

- [ ] **Step 3: Add hooks**

Create `web/src/features/code/useCodeReview.ts` with:
- `useRepoWorktrees(repoId)`
- `useRepoBranches(repoId)`
- `useRepoDiff(repoId, worktreePath, baseBranch)`
- `useFilePatch(repoId, worktreePath, baseBranch, path)`
- `useSubmitCodeReview()` mutation

Use `apiFetch` pattern from `web/src/features/tickets/useTicket.ts`.

- [ ] **Step 4: Add preview formatter + test**

Create `web/src/features/code/formatReviewPreview.ts` mirroring server `format_review_comment`.

Create `web/src/features/code/formatReviewPreview.test.ts` with one test asserting grouped markdown output.

Run: `cd web && npm test -- formatReviewPreview`

- [ ] **Step 5: Commit**

```bash
git add web/package.json web/package-lock.json web/src/lib/schemas/codeReview.ts web/src/features/code/
git commit -m "$(cat <<'EOF'
Add code review schemas, hooks, and review preview formatter.
EOF
)"
```

---

### Task 7: Code review page UI

**Files:**
- Create: `web/src/features/code/CodeReviewPage.tsx`
- Create: `web/src/features/code/ChangedFilesPanel.tsx`
- Create: `web/src/features/code/DiffViewer.tsx`
- Create: `web/src/features/code/SubmitReviewDialog.tsx`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: Add route**

In `web/src/App.tsx`, inside protected routes:

```tsx
import { CodeReviewPage } from './features/code/CodeReviewPage';

<Route path="/code" element={<CodeReviewPage />} />
```

`CodeReviewPage` reads `useSearchParams()` for `repoId`, `worktree`, `ticketId`, `baseBranch`.

- [ ] **Step 2: Build ChangedFilesPanel**

List files from diff summary; highlight selected file; show status badge + `+/-` counts.

- [ ] **Step 3: Build DiffViewer with react-diff-view**

```tsx
import { Diff, Hunk, parseDiff } from 'react-diff-view';
import { parse as parsePatch } from 'gitdiff-parser';
import 'react-diff-view/style/index.css';

// fetch patch via useFilePatch; parseDiff(patch); map hunks
// onClick line number → open InlineCommentWidget via renderWidget
// pending comments stored in parent state: InlineCommentDraft[]
```

Add syntax highlighting using `highlight.js` in a custom `renderToken` or pre-process hunk content (follow react-diff-view docs for token renderer).

- [ ] **Step 4: Build SubmitReviewDialog**

Fields per spec Section 3:
- Always: summary textarea, inline comments preview
- No ticket: project select (use `useProjects`), title, optional description
- Existing ticket: optional workflow action select; if `reassign_engineer`, show agent picker filtered to engineer presets

On success: toast + link to `/projects/{projectId}/board?ticket={ticketId}`.

- [ ] **Step 5: Wire CodeReviewPage layout**

Full viewport height (`h-screen flex flex-col`), toolbar with Selects for worktree/base branch, split/unified toggle, Submit button. Auto-select first worktree when none in URL; sync URL params on selection change via `setSearchParams`.

- [ ] **Step 6: Manual smoke**

Run: `cd web && npm run dev` (with server running)

1. Open `/code?repoId=...` — file list and diff render
2. Add inline comment — appears in diff
3. Submit — comment visible on ticket

- [ ] **Step 7: Commit**

```bash
git add web/src/features/code/ web/src/App.tsx
git commit -m "$(cat <<'EOF'
Add Code review page with diff viewer and submit review dialog.
EOF
)"
```

---

### Task 8: Entry points — ticket sidebar and Repositories page

**Files:**
- Modify: `web/src/features/tickets/TicketMetadataPanel.tsx`
- Modify: `web/src/features/repos/RepositoriesPage.tsx`

- [ ] **Step 1: Ticket metadata "Review code" button**

In `TicketMetadataPanel.tsx`, after repo is set and repo is ready:

```tsx
function buildCodeReviewUrl(ticket: Ticket, repoId: string, worktreePath: string | null) {
  const params = new URLSearchParams({ repoId, ticketId: ticket.id });
  if (worktreePath) params.set('worktree', worktreePath);
  return `/code?${params.toString()}`;
}

<Button
  type="button"
  variant="secondary"
  disabled={!ticket.repoId || repoNotReady}
  onClick={() => {
    const url = buildCodeReviewUrl(ticket, ticket.repoId!, latestRunWorktreePath);
    window.open(url, '_blank', 'noopener,noreferrer');
  }}
>
  Review code
</Button>
```

Use `latestRunWorktreePath` from runs, or fetch `useTicketGitInfo` for worktree path when ticket has repo.

- [ ] **Step 2: Repositories "View code" button**

Add an "Actions" column visible to all users (not admin-only), or add View code alongside admin actions:

```tsx
<Button
  type="button"
  variant="secondary"
  disabled={repo.verificationStatus !== 'ready'}
  onClick={() => {
    window.open(`/code?repoId=${repo.id}`, '_blank', 'noopener,noreferrer');
  }}
>
  View code
</Button>
```

- [ ] **Step 3: Commit**

```bash
git add web/src/features/tickets/TicketMetadataPanel.tsx web/src/features/repos/RepositoriesPage.tsx
git commit -m "$(cat <<'EOF'
Add Review code and View code entry points for in-app code review.
EOF
)"
```

---

### Task 9: Spec status and final verification

**Files:**
- Modify: `docs/superpowers/specs/2026-06-08-in-app-code-review-design.md`

- [ ] **Step 1: Run full test suites**

Run: `cargo test -p coppice-server`
Run: `cd web && npm test && npm run build`

Expected: all pass

- [ ] **Step 2: Update spec status**

In design spec, change:

```markdown
**Status:** Draft (pending review)
```

To:

```markdown
**Status:** Approved (plan ready)
```

Check all acceptance criteria boxes that are implemented.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-06-08-in-app-code-review-design.md
git commit -m "$(cat <<'EOF'
Mark in-app code review spec approved after implementation plan.
EOF
)"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| `/code` route in new tab | Task 7, 8 |
| Ticket sidebar + Repositories entry | Task 8 |
| Worktree picker (all on disk) | Task 2 |
| Selectable base branch | Task 2, 3, 7 |
| react-diff-view + inline comments | Task 6, 7 |
| Combined review comment | Task 1, 4 |
| Create ticket on submit (no ticket) | Task 4 |
| Optional workflow actions | Task 4, 7 |
| Path validation + patch size cap | Task 1, 3 |
| Integration tests | Task 5 |

## Out of scope (confirmed not in plan)

- Monaco editor, sessionStorage drafts, binary diffs, GitHub sync, uncommitted working-tree diff
