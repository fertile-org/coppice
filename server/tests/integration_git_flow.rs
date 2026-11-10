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
