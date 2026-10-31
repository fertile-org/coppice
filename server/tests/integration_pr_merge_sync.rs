mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use coppice_server::events::bus::AppEvent;
use coppice_server::services::human_review_service::HumanReviewService;
use coppice_server::services::pr_merge_sync::{
    self, leftover_local_commits_comment, merge_recorded_comment,
};
use coppice_server::services::ticket_attachments;
use coppice_server::services::worktree_service::compute_paths;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

const FORGE_TOKEN: &str = "ghs_testtoken";

#[derive(Clone, Default)]
struct GhState {
    inner: Arc<Mutex<GhInner>>,
}

#[derive(Default)]
struct GhInner {
    pulls_status: Option<u16>,
    pulls_headers: Vec<(String, String)>,
    pulls: Vec<Value>,
    compare: HashMap<String, (u16, Value)>,
    requests: Vec<Recorded>,
}

#[derive(Clone, Debug)]
struct Recorded {
    path: String,
    query: String,
    authorization: String,
}

impl GhState {
    fn recorded(&self) -> Vec<Recorded> {
        self.inner.lock().expect("gh lock").requests.clone()
    }
}

#[derive(serde::Deserialize)]
struct PullsQuery {
    state: Option<String>,
    head: Option<String>,
    base: Option<String>,
}

async fn list_pulls(
    State(state): State<GhState>,
    AxumPath((_, _)): AxumPath<(String, String)>,
    Query(query): Query<PullsQuery>,
    headers: HeaderMap,
) -> (StatusCode, HeaderMap, Json<Value>) {
    let mut inner = state.inner.lock().expect("gh lock");
    inner.requests.push(Recorded {
        path: "/pulls".into(),
        query: format!(
            "state={}&head={}&base={}",
            query.state.unwrap_or_default(),
            query.head.unwrap_or_default(),
            query.base.unwrap_or_default()
        ),
        authorization: header_text(&headers, "authorization"),
    });
    let mut out = HeaderMap::new();
    for (name, value) in &inner.pulls_headers {
        out.insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
            value.parse().expect("header value"),
        );
    }
    if let Some(status) = inner.pulls_status {
        return (
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            out,
            Json(json!({"message": "mock"})),
        );
    }
    (StatusCode::OK, out, Json(Value::Array(inner.pulls.clone())))
}

async fn compare(
    State(state): State<GhState>,
    AxumPath((_, _, spec)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let mut inner = state.inner.lock().expect("gh lock");
    inner.requests.push(Recorded {
        path: format!("/compare/{spec}"),
        query: String::new(),
        authorization: header_text(&headers, "authorization"),
    });
    match inner.compare.get(&spec).cloned() {
        Some((status, body)) => (
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(body),
        ),
        None => (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))),
    }
}

fn header_text(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

async fn spawn_github(state: GhState) -> String {
    let app = Router::new()
        .route("/repos/{owner}/{repo}/pulls", get(list_pulls))
        .route("/repos/{owner}/{repo}/compare/{spec}", get(compare))
        .with_state(state);
    let addr = common::spawn_test_server(app).await;
    format!("http://{addr}")
}

fn git_rev_parse(dir: &Path, rev: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(dir)
        .output()
        .expect("git rev-parse");
    assert!(output.status.success(), "rev-parse {rev} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_file(dir: &Path, name: &str, body: &str, message: &str) {
    std::fs::write(dir.join(name), body).expect("write");
    let add = Command::new("git")
        .args(["add", name])
        .current_dir(dir)
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    let commit = Command::new("git")
        .args(["commit", "-m", message])
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(dir)
        .output()
        .expect("git commit");
    assert!(
        commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

fn branch_exists(repo: &Path, ticket_id: Uuid) -> bool {
    let short = ticket_id.to_string();
    let short = short.split('-').next().unwrap();
    Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/agent/TICKET-{short}"),
        ])
        .current_dir(repo)
        .output()
        .expect("git rev-parse")
        .status
        .success()
}

fn ticket_branch(worktrees: &Path, ticket_id: Uuid) -> String {
    compute_paths(worktrees, "test-repo", ticket_id).branch_name
}

fn merged_pr(head_sha: &str, base: &str) -> Value {
    json!({
        "html_url": "https://github.com/acme/coppice/pull/7",
        "merged_at": "2026-10-31T00:00:00Z",
        "head": { "sha": head_sha },
        "base": { "ref": base },
    })
}

struct Workspace {
    _git_keep: tempfile::TempDir,
    env: common::AgentTestEnv,
    state: std::sync::Arc<coppice_server::AppState>,
    app: axum::Router,
    cookie: String,
    csrf: String,
    local_path: PathBuf,
    repo_id: String,
    board_id: String,
}

struct TicketFx {
    id: Uuid,
    worktree: PathBuf,
    accepted_sha: String,
}

impl Workspace {
    async fn new(remote_url: &str, with_token: bool) -> Self {
        let (git_dir, local_path) = common::create_temp_git_checkout();
        let (state, app, cookie, csrf, env) =
            common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
        let body = serde_json::json!({
            "name": "test-repo",
            "localPath": local_path.display().to_string(),
            "defaultBranch": "main",
            "remoteUrl": remote_url,
        });
        let res = app
            .clone()
            .oneshot(common::json_request(
                "POST",
                "/api/repos",
                &body.to_string(),
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let created: Value = common::json_body(res).await;
        let repo_id = created["id"].as_str().unwrap().to_string();
        if with_token {
            let res = app
                .clone()
                .oneshot(common::json_request(
                    "POST",
                    &format!("/api/repos/{repo_id}/forge-token"),
                    &json!({ "token": FORGE_TOKEN }).to_string(),
                    &cookie,
                    &csrf,
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }
        let board_id = common::create_test_board(&app, &cookie, &csrf).await;
        Self {
            _git_keep: git_dir,
            env,
            state,
            app,
            cookie,
            csrf,
            local_path,
            repo_id,
            board_id,
        }
    }

    async fn accept_ticket(&self) -> TicketFx {
        let ticket_id =
            common::create_test_ticket(&self.app, &self.board_id, &self.cookie, &self.csrf).await;
        common::set_ticket_repo(&self.app, &ticket_id, &self.repo_id, &self.cookie, &self.csrf)
            .await;
        let worktree = common::setup_worktree_with_commit(
            &self.local_path,
            self.env.worktrees.path(),
            "test-repo",
            &ticket_id,
        );
        let res = self
            .app
            .clone()
            .oneshot(common::json_request(
                "PATCH",
                &format!("/api/tickets/{ticket_id}/status"),
                &json!({ "status": "wait_for_final_review" }).to_string(),
                &self.cookie,
                &self.csrf,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let res = self
            .app
            .clone()
            .oneshot(common::json_request(
                "POST",
                &format!("/api/tickets/{ticket_id}/final-approve"),
                "{}",
                &self.cookie,
                &self.csrf,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "final approve");
        let accepted_sha = git_rev_parse(&worktree, "HEAD");
        TicketFx {
            id: ticket_id.parse().unwrap(),
            worktree,
            accepted_sha,
        }
    }

    async fn get_ticket(&self, ticket_id: Uuid) -> Value {
        let res = self
            .app
            .clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/tickets/{ticket_id}"),
                "",
                &self.cookie,
                &self.csrf,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        common::json_body(res).await
    }

    async fn comment_count(&self, ticket_id: Uuid, body: &str) -> i64 {
        let pool = self.state.db.clone().unwrap();
        sqlx::query_scalar(
            "SELECT count(*) FROM ticket_comments WHERE ticket_id = $1 AND body = $2",
        )
        .bind(ticket_id)
        .bind(body)
        .fetch_one(&pool)
        .await
        .unwrap()
    }

    async fn merge_comment_count(&self, ticket_id: Uuid, pr_url: &str) -> i64 {
        self.comment_count(ticket_id, &merge_recorded_comment(pr_url))
            .await
    }

    async fn sweep(&self, api: &str) -> pr_merge_sync::SweepOutcome {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        pr_merge_sync::sweep(&self.state, &client, api)
            .await
            .expect("sweep")
    }
}

fn merged_github(head_sha: &str) -> GhState {
    let state = GhState::default();
    state.inner.lock().expect("gh lock").pulls = vec![merged_pr(head_sha, "main")];
    state
}

#[tokio::test]
async fn merged_pr_clears_accepted_not_merged_for_each_merge_style() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    for style in ["merge commit", "squash", "rebase"] {
        let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
        let ticket = ws.accept_ticket().await;
        let gh = merged_github(&ticket.accepted_sha);
        let api = spawn_github(gh.clone()).await;
        let mut events = ws.state.event_bus.subscribe();

        let outcome = ws.sweep(&api).await;
        assert_eq!(outcome.recorded, 1, "{style}");
        assert!(!outcome.stopped_for_rate_limit);

        let body = ws.get_ticket(ticket.id).await;
        assert_eq!(body["humanReview"]["merged"], true, "{style}");
        assert_eq!(body["humanReview"]["stale"], false, "{style}");
        assert_eq!(
            ws.merge_comment_count(ticket.id, "https://github.com/acme/coppice/pull/7")
                .await,
            1,
            "{style}"
        );
        assert!(!ticket.worktree.exists(), "{style} worktree left behind");
        assert!(
            !branch_exists(&ws.local_path, ticket.id),
            "{style} branch left behind"
        );

        let mut saw_ticket = false;
        let mut saw_comment = false;
        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::TicketUpdated { ticket_id, .. } if ticket_id == ticket.id => {
                    saw_ticket = true;
                }
                AppEvent::CommentCreated { ticket_id, .. } if ticket_id == ticket.id => {
                    saw_comment = true;
                }
                _ => {}
            }
        }
        assert!(saw_ticket, "{style} missing ticket.updated");
        assert!(saw_comment, "{style} missing comment.created");

        let requests = gh.recorded();
        assert!(
            requests.iter().any(|r| r.path == "/pulls"
                && r.query.contains("state=closed")
                && r.query.contains(&format!(
                    "head=acme:{}",
                    ticket_branch(ws.env.worktrees.path(), ticket.id)
                ))
                && r.authorization == format!("Bearer {FORGE_TOKEN}")),
            "{style} pulls request: {requests:?}"
        );
        assert!(
            requests.iter().all(|r| !r.path.starts_with("/compare")),
            "{style} compare should be skipped when SHAs match"
        );
    }
}

#[tokio::test]
async fn first_sweep_records_a_ticket_already_stuck() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
    let ticket = ws.accept_ticket().await;
    let before = ws.get_ticket(ticket.id).await;
    assert_eq!(before["humanReview"]["merged"], false);

    let api = spawn_github(merged_github(&ticket.accepted_sha)).await;
    let outcome = ws.sweep(&api).await;
    assert_eq!(outcome.recorded, 1);
    assert_eq!(ws.get_ticket(ticket.id).await["humanReview"]["merged"], true);
}

#[tokio::test]
async fn descendant_head_counts_as_merged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
    let ticket = ws.accept_ticket().await;
    let head = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let gh = GhState::default();
    {
        let mut inner = gh.inner.lock().expect("gh lock");
        inner.pulls = vec![merged_pr(head, "main")];
        inner.compare.insert(
            format!("{}...{head}", ticket.accepted_sha.to_ascii_lowercase()),
            (200, json!({ "status": "ahead" })),
        );
    }
    let api = spawn_github(gh).await;
    let outcome = ws.sweep(&api).await;
    assert_eq!(outcome.recorded, 1);
    assert_eq!(ws.get_ticket(ticket.id).await["humanReview"]["merged"], true);
}

#[tokio::test]
async fn predating_head_or_compare_404_leaves_ticket_unmerged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    for (label, compare) in [
        ("behind", Some((200, json!({ "status": "behind" })))),
        ("404", None),
    ] {
        let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
        let ticket = ws.accept_ticket().await;
        let head = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let gh = GhState::default();
        {
            let mut inner = gh.inner.lock().expect("gh lock");
            inner.pulls = vec![merged_pr(head, "main")];
            if let Some((status, body)) = compare {
                inner.compare.insert(
                    format!("{}...{head}", ticket.accepted_sha.to_ascii_lowercase()),
                    (status, body),
                );
            }
        }
        let api = spawn_github(gh).await;
        let outcome = ws.sweep(&api).await;
        assert_eq!(outcome.recorded, 0, "{label}");
        let body = ws.get_ticket(ticket.id).await;
        assert_eq!(body["humanReview"]["merged"], false, "{label}");
        assert_eq!(body["humanReview"]["stale"], false, "{label}");
        assert!(ticket.worktree.exists(), "{label} worktree removed");
        assert!(
            branch_exists(&ws.local_path, ticket.id),
            "{label} branch removed"
        );
        assert_eq!(
            ws.merge_comment_count(ticket.id, "https://github.com/acme/coppice/pull/7")
                .await,
            0,
            "{label}"
        );
    }
}

#[tokio::test]
async fn open_unmerged_or_non_default_base_changes_nothing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cases = [
        json!({
            "html_url": "https://github.com/acme/coppice/pull/1",
            "merged_at": Value::Null,
            "head": { "sha": "deadbeef" },
            "base": { "ref": "main" },
        }),
        json!({
            "html_url": "https://github.com/acme/coppice/pull/2",
            "merged_at": "2026-10-31T00:00:00Z",
            "head": { "sha": "deadbeef" },
            "base": { "ref": "develop" },
        }),
    ];
    for pull in cases {
        let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
        let ticket = ws.accept_ticket().await;
        let mut pull = pull;
        pull["head"]["sha"] = json!(ticket.accepted_sha);
        let gh = GhState::default();
        gh.inner.lock().expect("gh lock").pulls = vec![pull];
        let api = spawn_github(gh).await;
        assert_eq!(ws.sweep(&api).await.recorded, 0);
        assert_eq!(ws.get_ticket(ticket.id).await["humanReview"]["merged"], false);
        assert!(ticket.worktree.exists());
    }
}

#[tokio::test]
async fn leftover_local_commits_mark_acceptance_stale() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
    let ticket = ws.accept_ticket().await;
    commit_file(&ticket.worktree, "extra.txt", "local only\n", "extra");
    let tip = git_rev_parse(&ticket.worktree, "HEAD");
    assert_ne!(tip, ticket.accepted_sha);

    let api = spawn_github(merged_github(&ticket.accepted_sha)).await;
    let mut events = ws.state.event_bus.subscribe();
    assert_eq!(ws.sweep(&api).await.recorded, 0);
    assert_eq!(ws.sweep(&api).await.recorded, 0);

    let body = ws.get_ticket(ticket.id).await;
    assert_eq!(body["status"], "in_review");
    assert_eq!(body["humanReview"]["merged"], false);
    assert_eq!(body["humanReview"]["stale"], true);
    assert!(ticket.worktree.exists());
    assert!(branch_exists(&ws.local_path, ticket.id));
    let pr_url = "https://github.com/acme/coppice/pull/7";
    assert_eq!(ws.merge_comment_count(ticket.id, pr_url).await, 0);
    assert_eq!(
        ws.comment_count(ticket.id, &leftover_local_commits_comment(pr_url))
            .await,
        1
    );

    let mut saw_ticket = false;
    while let Ok(event) = events.try_recv() {
        if let AppEvent::TicketUpdated { ticket_id, .. } = event {
            if ticket_id == ticket.id {
                saw_ticket = true;
            }
        }
    }
    assert!(saw_ticket, "missing ticket.updated after leftover local commits");
}

#[tokio::test]
async fn github_error_skips_ticket_and_rate_limit_stops_sweep() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
    let first = ws.accept_ticket().await;
    let second = ws.accept_ticket().await;

    let error = GhState::default();
    error.inner.lock().expect("gh lock").pulls_status = Some(500);
    let api = spawn_github(error.clone()).await;
    let outcome = ws.sweep(&api).await;
    assert_eq!(outcome.recorded, 0);
    assert!(!outcome.stopped_for_rate_limit);
    assert!(error.recorded().len() >= 2, "500 should continue to next ticket");
    assert_eq!(ws.get_ticket(first.id).await["humanReview"]["merged"], false);
    assert_eq!(ws.get_ticket(second.id).await["humanReview"]["merged"], false);

    let limited = GhState::default();
    limited.inner.lock().expect("gh lock").pulls_status = Some(429);
    let api = spawn_github(limited.clone()).await;
    let outcome = ws.sweep(&api).await;
    assert_eq!(outcome.recorded, 0);
    assert!(outcome.stopped_for_rate_limit);
    assert_eq!(limited.recorded().len(), 1, "429 must stop the current sweep");

    let forbidden = GhState::default();
    forbidden.inner.lock().expect("gh lock").pulls_status = Some(403);
    let api = spawn_github(forbidden.clone()).await;
    let outcome = ws.sweep(&api).await;
    assert_eq!(outcome.recorded, 0);
    assert!(!outcome.stopped_for_rate_limit);
    assert!(
        forbidden.recorded().len() >= 2,
        "plain 403 should continue to the next ticket"
    );

    let rate_limited_403 = GhState::default();
    {
        let mut inner = rate_limited_403.inner.lock().expect("gh lock");
        inner.pulls_status = Some(403);
        inner.pulls_headers = vec![("x-ratelimit-remaining".into(), "0".into())];
    }
    let api = spawn_github(rate_limited_403.clone()).await;
    let outcome = ws.sweep(&api).await;
    assert_eq!(outcome.recorded, 0);
    assert!(outcome.stopped_for_rate_limit);
    assert_eq!(
        rate_limited_403.recorded().len(),
        1,
        "403 with x-ratelimit-remaining: 0 must stop the current sweep"
    );
}

#[tokio::test]
async fn missing_token_and_enterprise_host_never_call_github() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    for (remote, token) in [
        ("https://github.com/acme/coppice.git", false),
        ("https://github.example.com/acme/coppice.git", true),
    ] {
        let ws = Workspace::new(remote, token).await;
        let ticket = ws.accept_ticket().await;
        let gh = merged_github(&ticket.accepted_sha);
        let api = spawn_github(gh.clone()).await;
        let outcome = ws.sweep(&api).await;
        assert_eq!(outcome.listed, 0, "{remote} token={token}");
        assert_eq!(outcome.recorded, 0);
        assert!(gh.recorded().is_empty(), "{remote} leaked a request");
        assert_eq!(ws.get_ticket(ticket.id).await["humanReview"]["merged"], false);
    }
}

#[tokio::test]
async fn conditional_write_ignores_stale_or_already_merged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
    let ticket = ws.accept_ticket().await;
    let pool = ws.state.db.clone().unwrap();
    let reviews = HumanReviewService::new(&pool);

    assert!(reviews
        .record_remote_merge_if_current(ticket.id, &ticket.accepted_sha, "cccccccccccccccccccccccccccccccccccccccc")
        .await
        .unwrap());
    assert!(!reviews
        .record_remote_merge_if_current(ticket.id, &ticket.accepted_sha, "dddddddddddddddddddddddddddddddddddddddd")
        .await
        .unwrap());
    let review = reviews.get(ticket.id).await.unwrap().unwrap();
    assert_eq!(
        review.merged_at.is_some(),
        true
    );
    assert_eq!(
        review.head_sha.to_ascii_lowercase(),
        ticket.accepted_sha.to_ascii_lowercase()
    );

    let other = ws.accept_ticket().await;
    sqlx::query("UPDATE ticket_human_reviews SET stale = true WHERE ticket_id = $1")
        .bind(other.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!reviews
        .record_remote_merge_if_current(other.id, &other.accepted_sha, "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
        .await
        .unwrap());
    assert!(reviews.get(other.id).await.unwrap().unwrap().merged_at.is_none());

    let raced = ws.accept_ticket().await;
    assert!(!reviews
        .record_remote_merge_if_current(raced.id, "ffffffffffffffffffffffffffffffffffffffff", &raced.accepted_sha)
        .await
        .unwrap());
    assert!(reviews.get(raced.id).await.unwrap().unwrap().merged_at.is_none());
}

#[tokio::test]
async fn repeated_sweeps_post_one_comment() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
    let ticket = ws.accept_ticket().await;
    let dir = ticket_attachments::ticket_attachment_dir(
        &ws.state.config.storage.artifacts_dir,
        ticket.id,
    )
    .unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("staged.txt"), "copy").unwrap();

    let api = spawn_github(merged_github(&ticket.accepted_sha)).await;
    assert_eq!(ws.sweep(&api).await.recorded, 1);
    assert_eq!(ws.sweep(&api).await.recorded, 0);
    assert_eq!(
        ws.merge_comment_count(ticket.id, "https://github.com/acme/coppice/pull/7")
            .await,
        1
    );
    assert!(!dir.join("staged.txt").exists());
}

#[tokio::test]
async fn later_merged_pr_on_the_branch_is_still_found() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let ws = Workspace::new("https://github.com/acme/coppice.git", true).await;
    let ticket = ws.accept_ticket().await;
    let gh = GhState::default();
    gh.inner.lock().expect("gh lock").pulls = vec![
        json!({
            "html_url": "https://github.com/acme/coppice/pull/1",
            "merged_at": Value::Null,
            "head": { "sha": "1111111111111111111111111111111111111111" },
            "base": { "ref": "main" },
        }),
        merged_pr(&ticket.accepted_sha, "main"),
    ];
    let api = spawn_github(gh).await;
    assert_eq!(ws.sweep(&api).await.recorded, 1);
    assert_eq!(ws.get_ticket(ticket.id).await["humanReview"]["merged"], true);
}
