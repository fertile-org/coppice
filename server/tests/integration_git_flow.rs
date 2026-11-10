mod common;

use axum::http::StatusCode;
use coppice_server::domain::git_state::GitOutcome;
use coppice_server::services::human_review_service::HumanReviewService;
use tower::ServiceExt;
use uuid::Uuid;

const SHA: &str = "abc1234abc1234abc1234abc1234abc1234abc12";

async fn setup_ticket() -> (
    std::sync::Arc<coppice_server::AppState>,
    Uuid,
    Uuid,
    common::AgentTestEnv,
) {
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let pool = state.db.clone().expect("pool");
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("user");
    (state, Uuid::parse_str(&ticket_id).unwrap(), user_id, env)
}

#[tokio::test]
async fn reacceptance_clears_git_outcome() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, ticket_id, user_id, _env) = setup_ticket().await;
    let pool = state.db.clone().expect("pool");
    let svc = HumanReviewService::new(&pool);
    svc.record_acceptance(ticket_id, Some(user_id), SHA)
        .await
        .unwrap();
    assert!(svc
        .set_git_outcome(
            ticket_id,
            Some(GitOutcome::Conflict),
            Some("conflict in a.txt")
        )
        .await
        .unwrap());
    let review = svc.get(ticket_id).await.unwrap().unwrap();
    assert_eq!(review.git_outcome, Some(GitOutcome::Conflict));
    assert_eq!(
        review.git_outcome_reason.as_deref(),
        Some("conflict in a.txt")
    );
    assert!(!review.accepted_by_coppice);

    let again = svc.record_acceptance(ticket_id, None, SHA).await.unwrap();
    assert_eq!(again.git_outcome, None);
    assert_eq!(again.git_outcome_reason, None);
    assert_eq!(again.merged_base_branch, None);
    assert!(again.accepted_by_coppice);
    assert_eq!(svc.get(ticket_id).await.unwrap().unwrap().git_outcome, None);
}

#[tokio::test]
async fn set_git_outcome_reports_change() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, ticket_id, user_id, _env) = setup_ticket().await;
    let pool = state.db.clone().expect("pool");
    let svc = HumanReviewService::new(&pool);
    svc.record_acceptance(ticket_id, Some(user_id), SHA)
        .await
        .unwrap();
    assert!(svc
        .set_git_outcome(ticket_id, Some(GitOutcome::MainDirty), None)
        .await
        .unwrap());
    assert!(!svc
        .set_git_outcome(ticket_id, Some(GitOutcome::MainDirty), None)
        .await
        .unwrap());
    assert!(svc.set_git_outcome(ticket_id, None, None).await.unwrap());
}

async fn create_repo_json(
    app: &axum::Router,
    remote: Option<&str>,
    cookie: &str,
    csrf: &str,
) -> serde_json::Value {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut body = serde_json::json!({
        "name": "flow-repo",
        "localPath": dir.path().display().to_string(),
        "defaultBranch": "main",
    });
    if let Some(url) = remote {
        body["remoteUrl"] = serde_json::json!(url);
    }
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/repos",
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    common::json_body(res).await
}

async fn patch_repo(
    app: &axum::Router,
    id: &str,
    body: serde_json::Value,
    cookie: &str,
    csrf: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/repos/{id}"),
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap()
}

#[tokio::test]
async fn repo_git_flow_defaults_off() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let repo = create_repo_json(&app, None, &cookie, &csrf).await;
    assert_eq!(repo["mergeAfterAccept"], false);
    assert_eq!(repo["pushAfterMerge"], false);
}

#[tokio::test]
async fn repo_git_flow_patch_roundtrip() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;
    let repo = create_repo_json(
        &app,
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;
    let id = repo["id"].as_str().unwrap().to_string();
    assert!(repo["pushAfterMergeDisabledReason"].is_null());

    let res = patch_repo(
        &app,
        &id,
        serde_json::json!({ "mergeAfterAccept": true, "pushAfterMerge": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let got: serde_json::Value = common::json_body(res).await;
    assert_eq!(got["mergeAfterAccept"], true);
    assert_eq!(got["pushAfterMerge"], true);
}

#[tokio::test]
async fn push_after_merge_rejected_without_remote() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;
    let repo = create_repo_json(&app, None, &cookie, &csrf).await;
    assert!(repo["pushAfterMergeDisabledReason"].is_string());
    let id = repo["id"].as_str().unwrap().to_string();
    let res = patch_repo(
        &app,
        &id,
        serde_json::json!({ "pushAfterMerge": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

struct MergeFixture {
    _git_keep: tempfile::TempDir,
    _worktrees: tempfile::TempDir,
    state: std::sync::Arc<coppice_server::AppState>,
    app: axum::Router,
    cookie: String,
    csrf: String,
    ticket_id: String,
    local_path: std::path::PathBuf,
    worktree_path: std::path::PathBuf,
}

/// A repo-linked ticket with a worktree commit, accepted to Done.
async fn accepted_ticket_with_worktree() -> MergeFixture {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let worktrees = tempfile::tempdir().expect("worktrees");
    let path = worktrees.path().to_string_lossy().into_owned();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(move |config| {
        config.agent.worktrees_path = path;
    })
    .await;
    let worktrees_root = std::path::PathBuf::from(&state.config.agent.worktrees_path);
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree_path =
        common::setup_worktree_with_commit(&local_path, &worktrees_root, "test-repo", &ticket_id);
    MergeFixture {
        _git_keep: git_dir,
        _worktrees: worktrees,
        state,
        app,
        cookie,
        csrf,
        ticket_id,
        local_path,
        worktree_path,
    }
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn post(fx: &MergeFixture, method: &str, uri: &str, body: &str) -> axum::response::Response {
    fx.app
        .clone()
        .oneshot(common::json_request(
            method, uri, body, &fx.cookie, &fx.csrf,
        ))
        .await
        .unwrap()
}

async fn accept(fx: &MergeFixture) {
    let res = post(
        fx,
        "PATCH",
        &format!("/api/tickets/{}/status", fx.ticket_id),
        r#"{"status":"wait_for_final_review"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = post(
        fx,
        "POST",
        &format!("/api/tickets/{}/final-approve", fx.ticket_id),
        "{}",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "final approve");
}

async fn merge_main(fx: &MergeFixture) -> axum::response::Response {
    post(
        fx,
        "POST",
        &format!("/api/tickets/{}/merge-branch", fx.ticket_id),
        r#"{"baseBranch":"main"}"#,
    )
    .await
}

async fn ticket_status(fx: &MergeFixture) -> String {
    let res = post(fx, "GET", &format!("/api/tickets/{}", fx.ticket_id), "").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body["status"].as_str().unwrap_or_default().to_string()
}

async fn review_of(
    fx: &MergeFixture,
) -> coppice_server::services::human_review_service::HumanReview {
    let pool = fx.state.db.clone().expect("pool");
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    HumanReviewService::new(&pool)
        .get(id)
        .await
        .unwrap()
        .expect("review")
}

#[tokio::test]
async fn manual_merge_conflict_records_outcome() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = accepted_ticket_with_worktree().await;
    std::fs::write(fx.local_path.join("README.md"), "# test\nmain side\n").unwrap();
    git(&fx.local_path, &["add", "README.md"]);
    git(&fx.local_path, &["commit", "-m", "main conflict"]);
    std::fs::write(fx.worktree_path.join("README.md"), "# test\nfeature side\n").unwrap();
    git(&fx.worktree_path, &["add", "README.md"]);
    git(&fx.worktree_path, &["commit", "-m", "feature conflict"]);
    accept(&fx).await;

    let res = merge_main(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let review = review_of(&fx).await;
    assert_eq!(review.git_outcome, Some(GitOutcome::Conflict));
    assert_eq!(
        review.git_outcome_reason.as_deref(),
        Some("1 file conflicts with main")
    );
    assert_eq!(ticket_status(&fx).await, "done");
}

#[tokio::test]
async fn manual_merge_dirty_main_records_outcome() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = accepted_ticket_with_worktree().await;
    accept(&fx).await;
    std::fs::write(fx.local_path.join("README.md"), "# test\nuncommitted\n").unwrap();

    let res = merge_main(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let review = review_of(&fx).await;
    assert_eq!(review.git_outcome, Some(GitOutcome::MainDirty));
    assert_eq!(
        review.git_outcome_reason.as_deref(),
        Some("Your main checkout has uncommitted changes")
    );
    assert_eq!(ticket_status(&fx).await, "done");

    // A successful retry clears the outcome.
    git(&fx.local_path, &["checkout", "--", "README.md"]);
    let res = merge_main(&fx).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(review_of(&fx).await.git_outcome, None);
}
