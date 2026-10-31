//! Records GitHub.com PR merges on accepted, unmerged Done tickets.
//!
//! `merged_at` is written by Coppice's Merge button and by this sweep. A merge
//! on GitHub does not notify Coppice, so the board would otherwise stay on
//! "Accepted, not merged". The sweep uses the GitHub API because squash and
//! rebase merges leave the accepted SHA off the default branch.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use sqlx::Row;
use uuid::Uuid;

use crate::domain::comment::{AuthorType, CommentIntent};
use crate::events::bus::AppEvent;
use crate::events::publish_ticket_updated;
use crate::services::comment_service::CommentService;
use crate::services::human_review_service::{
    normalize_sha, reviewed_sha_matches, HumanReviewService,
};
use crate::services::pr_create_url::github_com_owner_repo;
use crate::services::secret_service::SecretService;
use crate::services::ticket_attachments::{self, AttachmentFolderEnd};
use crate::services::ticket_git_service::{TicketGitError, TicketGitService};
use crate::services::ticket_service::TicketService;
use crate::services::worktree_cleanup;
use crate::services::worktree_service::compute_paths;
use crate::AppState;

pub const GITHUB_API_BASE: &str = "https://api.github.com";
const SWEEP_INTERVAL: Duration = Duration::from_secs(5 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepOutcome {
    pub listed: usize,
    pub recorded: usize,
    pub stopped_for_rate_limit: bool,
}

struct EligibleTicket {
    ticket_id: Uuid,
    repo_name: String,
    remote_url: String,
    default_branch: String,
    forge_token_secret_id: Uuid,
    accepted_sha: String,
}

struct MatchedPr {
    html_url: String,
    head_sha: String,
}

enum GithubOutcome<T> {
    Ok(T),
    NotFound,
    RateLimited { status: StatusCode },
    Failed(String),
}

enum SyncStop {
    RateLimited,
}

pub fn merge_recorded_comment(pr_url: &str) -> String {
    format!("**Merge:** pull request merged on GitHub ({pr_url})")
}

pub fn spawn_sweep(state: Arc<AppState>) {
    if state.db.is_none() {
        return;
    }
    tokio::spawn(async move {
        let client = match reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build() {
            Ok(client) => client,
            Err(err) => {
                tracing::error!(error = %err, "pr merge sync client failed");
                return;
            }
        };
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(err) = sweep(&state, &client, GITHUB_API_BASE).await {
                tracing::warn!(error = %err, "pr merge sync sweep failed");
            }
        }
    });
}

pub async fn sweep(
    state: &AppState,
    client: &reqwest::Client,
    github_api_base: &str,
) -> Result<SweepOutcome, sqlx::Error> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(SweepOutcome::default());
    };
    let rows = list_eligible(pool).await?;
    let mut outcome = SweepOutcome::default();
    let worktrees_root = PathBuf::from(&state.config.agent.worktrees_path);
    for row in rows {
        let Some((owner, repo)) = github_com_owner_repo(&row.remote_url) else {
            continue;
        };
        outcome.listed += 1;
        match sync_ticket(
            state,
            pool,
            client,
            github_api_base,
            &worktrees_root,
            &row,
            &owner,
            &repo,
        )
        .await
        {
            Ok(true) => outcome.recorded += 1,
            Ok(false) => {}
            Err(SyncStop::RateLimited) => {
                outcome.stopped_for_rate_limit = true;
                break;
            }
        }
    }
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
async fn sync_ticket(
    state: &AppState,
    pool: &sqlx::PgPool,
    client: &reqwest::Client,
    github_api_base: &str,
    worktrees_root: &Path,
    row: &EligibleTicket,
    owner: &str,
    repo: &str,
) -> Result<bool, SyncStop> {
    let token = match SecretService::new(pool, &state.secret_store)
        .decrypt_by_id(row.forge_token_secret_id)
        .await
    {
        Ok(token) => token,
        Err(err) => {
            tracing::warn!(
                ticket_id = %row.ticket_id,
                error = %err,
                "pr merge sync: could not decrypt forge token"
            );
            return Ok(false);
        }
    };

    let branch = compute_paths(worktrees_root, &row.repo_name, row.ticket_id).branch_name;
    let matched = match find_matching_pr(
        client,
        github_api_base,
        token.trim(),
        owner,
        repo,
        &branch,
        &row.default_branch,
        &row.accepted_sha,
    )
    .await
    {
        GithubOutcome::Ok(Some(pr)) => pr,
        GithubOutcome::Ok(None) | GithubOutcome::NotFound => return Ok(false),
        GithubOutcome::Failed(err) => {
            tracing::warn!(
                ticket_id = %row.ticket_id,
                error = %err,
                "pr merge sync: github error"
            );
            return Ok(false);
        }
        GithubOutcome::RateLimited { status } => {
            tracing::warn!(
                ticket_id = %row.ticket_id,
                %status,
                "pr merge sync: github rate-limited; retry next tick"
            );
            return Err(SyncStop::RateLimited);
        }
    };

    if local_work_would_be_lost(pool, worktrees_root, row, &matched.head_sha).await {
        return Ok(false);
    }

    let recorded = match HumanReviewService::new(pool)
        .record_remote_merge_if_current(row.ticket_id, &row.accepted_sha, &matched.head_sha)
        .await
    {
        Ok(recorded) => recorded,
        Err(err) => {
            tracing::warn!(
                ticket_id = %row.ticket_id,
                error = %err,
                "pr merge sync: could not record merge"
            );
            return Ok(false);
        }
    };
    if !recorded {
        return Ok(false);
    }

    after_recorded_merge(state, pool, row.ticket_id, &matched.html_url, worktrees_root).await;
    Ok(true)
}

async fn after_recorded_merge(
    state: &AppState,
    pool: &sqlx::PgPool,
    ticket_id: Uuid,
    pr_url: &str,
    worktrees_root: &Path,
) {
    match CommentService::new(pool)
        .create(
            ticket_id,
            AuthorType::System,
            None,
            &merge_recorded_comment(pr_url),
            CommentIntent::SystemEvent,
            &[],
            &[],
        )
        .await
    {
        Ok(comment) => {
            state.event_bus.publish(AppEvent::CommentCreated {
                comment_id: comment.id,
                ticket_id,
                author_type: "system".into(),
            });
        }
        Err(err) => {
            tracing::warn!(%ticket_id, error = %err, "pr merge sync: could not post merge comment");
        }
    }

    if let Err(err) = ticket_attachments::clear_if_ended(
        &state.config.storage.artifacts_dir,
        ticket_id,
        AttachmentFolderEnd::MergeSucceeded,
    ) {
        tracing::warn!(
            error = %err,
            %ticket_id,
            "could not remove staged ticket attachments after remote merge"
        );
    }

    worktree_cleanup::cleanup_ticket(pool, worktrees_root.to_path_buf(), ticket_id, true).await;

    match TicketService::new(pool).get(ticket_id).await {
        Ok(ticket) => publish_ticket_updated(&state.event_bus, &ticket),
        Err(err) => {
            tracing::warn!(%ticket_id, error = %err, "pr merge sync: could not publish ticket update");
        }
    }
}

async fn local_work_would_be_lost(
    pool: &sqlx::PgPool,
    worktrees_root: &Path,
    row: &EligibleTicket,
    pr_head: &str,
) -> bool {
    let tip = match TicketGitService::new(pool, worktrees_root.to_path_buf())
        .reviewed_head_sha(row.ticket_id)
        .await
    {
        Ok(Some(sha)) => sha,
        Ok(None) => return false,
        Err(TicketGitError::TicketBranchMissing(_)) => return false,
        Err(err) => {
            tracing::warn!(
                ticket_id = %row.ticket_id,
                error = %err,
                "pr merge sync: could not read local branch tip"
            );
            return true;
        }
    };
    if reviewed_sha_matches(&tip, &row.accepted_sha) || reviewed_sha_matches(&tip, pr_head) {
        return false;
    }
    if let Err(err) = TicketGitService::new(pool, worktrees_root.to_path_buf())
        .note_branch_head(row.ticket_id)
        .await
    {
        tracing::warn!(
            ticket_id = %row.ticket_id,
            error = %err,
            "pr merge sync: could not mark leftover local commits stale"
        );
    }
    true
}

async fn list_eligible(pool: &sqlx::PgPool) -> Result<Vec<EligibleTicket>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            ticket.id,
            repo.name,
            repo.remote_url,
            repo.default_branch,
            repo.forge_token_secret_id,
            review.head_sha
        FROM tickets ticket
        JOIN repos repo ON repo.id = ticket.repo_id
        JOIN ticket_human_reviews review ON review.ticket_id = ticket.id
        WHERE ticket.status = 'done'
          AND ticket.archived_at IS NULL
          AND review.stale = false
          AND review.merged_at IS NULL
          AND repo.forge_token_secret_id IS NOT NULL
          AND repo.remote_url IS NOT NULL
          AND btrim(repo.remote_url) <> ''
        ORDER BY review.accepted_at ASC, ticket.id ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| EligibleTicket {
            ticket_id: row.get("id"),
            repo_name: row.get("name"),
            remote_url: row.get("remote_url"),
            default_branch: row.get("default_branch"),
            forge_token_secret_id: row.get("forge_token_secret_id"),
            accepted_sha: row.get("head_sha"),
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
async fn find_matching_pr(
    client: &reqwest::Client,
    api_base: &str,
    token: &str,
    owner: &str,
    repo: &str,
    branch: &str,
    default_branch: &str,
    accepted_sha: &str,
) -> GithubOutcome<Option<MatchedPr>> {
    let pulls = match list_closed_prs(client, api_base, token, owner, repo, branch, default_branch)
        .await
    {
        GithubOutcome::Ok(pulls) => pulls,
        GithubOutcome::NotFound => return GithubOutcome::Ok(None),
        GithubOutcome::RateLimited { status } => return GithubOutcome::RateLimited { status },
        GithubOutcome::Failed(err) => return GithubOutcome::Failed(err),
    };

    for pull in pulls {
        if pull.merged_at.is_none() {
            continue;
        }
        if pull.base_ref != default_branch {
            continue;
        }
        match pr_contains_accepted(
            client,
            api_base,
            token,
            owner,
            repo,
            accepted_sha,
            &pull.head_sha,
        )
        .await
        {
            GithubOutcome::Ok(true) => {
                return GithubOutcome::Ok(Some(MatchedPr {
                    html_url: pull.html_url,
                    head_sha: pull.head_sha,
                }));
            }
            GithubOutcome::Ok(false) | GithubOutcome::NotFound => {}
            GithubOutcome::RateLimited { status } => {
                return GithubOutcome::RateLimited { status };
            }
            GithubOutcome::Failed(err) => return GithubOutcome::Failed(err),
        }
    }
    GithubOutcome::Ok(None)
}

struct ListedPr {
    html_url: String,
    head_sha: String,
    base_ref: String,
    merged_at: Option<String>,
}

async fn list_closed_prs(
    client: &reqwest::Client,
    api_base: &str,
    token: &str,
    owner: &str,
    repo: &str,
    branch: &str,
    default_branch: &str,
) -> GithubOutcome<Vec<ListedPr>> {
    let url = format!(
        "{}/repos/{owner}/{repo}/pulls",
        api_base.trim_end_matches('/')
    );
    let body = match github_get(
        client,
        token,
        &url,
        &[
            ("state", "closed"),
            ("head", &format!("{owner}:{branch}")),
            ("base", default_branch),
            ("per_page", "100"),
        ],
    )
    .await
    {
        GithubOutcome::Ok(body) => body,
        GithubOutcome::NotFound => return GithubOutcome::NotFound,
        GithubOutcome::RateLimited { status } => return GithubOutcome::RateLimited { status },
        GithubOutcome::Failed(err) => return GithubOutcome::Failed(err),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(parsed) => parsed,
        Err(err) => return GithubOutcome::Failed(err.to_string()),
    };
    let Some(items) = parsed.as_array() else {
        return GithubOutcome::Failed("pulls response was not an array".into());
    };
    let mut pulls = Vec::with_capacity(items.len());
    for item in items {
        let Some(html_url) = item.get("html_url").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(head_sha) = item
            .pointer("/head/sha")
            .and_then(|v| v.as_str())
            .map(normalize_sha)
        else {
            continue;
        };
        let base_ref = item
            .pointer("/base/ref")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let merged_at = item
            .get("merged_at")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        pulls.push(ListedPr {
            html_url: html_url.to_string(),
            head_sha,
            base_ref,
            merged_at,
        });
    }
    GithubOutcome::Ok(pulls)
}

async fn pr_contains_accepted(
    client: &reqwest::Client,
    api_base: &str,
    token: &str,
    owner: &str,
    repo: &str,
    accepted_sha: &str,
    head_sha: &str,
) -> GithubOutcome<bool> {
    if reviewed_sha_matches(accepted_sha, head_sha) {
        return GithubOutcome::Ok(true);
    }
    let url = format!(
        "{}/repos/{owner}/{repo}/compare/{accepted}...{head}",
        api_base.trim_end_matches('/'),
        accepted = normalize_sha(accepted_sha),
        head = normalize_sha(head_sha),
    );
    let body = match github_get(client, token, &url, &[("per_page", "1")]).await {
        GithubOutcome::Ok(body) => body,
        GithubOutcome::NotFound => return GithubOutcome::Ok(false),
        GithubOutcome::RateLimited { status } => return GithubOutcome::RateLimited { status },
        GithubOutcome::Failed(err) => return GithubOutcome::Failed(err),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(parsed) => parsed,
        Err(err) => return GithubOutcome::Failed(err.to_string()),
    };
    let status = parsed.get("status").and_then(|v| v.as_str()).unwrap_or("");
    GithubOutcome::Ok(status == "ahead" || status == "identical")
}

async fn github_get(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    query: &[(&str, &str)],
) -> GithubOutcome<String> {
    let response = match client
        .get(url)
        .query(query)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "coppice")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => return GithubOutcome::Failed(err.to_string()),
    };
    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(err) => return GithubOutcome::Failed(err.to_string()),
    };
    if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
        return GithubOutcome::RateLimited { status };
    }
    if status == StatusCode::NOT_FOUND {
        return GithubOutcome::NotFound;
    }
    if !status.is_success() {
        return GithubOutcome::Failed(format!("github {status}"));
    }
    GithubOutcome::Ok(body)
}
