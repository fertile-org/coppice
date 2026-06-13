mod common;

use axum::http::StatusCode;
use http_body_util::BodyExt;
use std::time::{Duration, Instant};
use tower::ServiceExt;

async fn setup_agent_ticket(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    repo_id: &str,
) -> (String, String, String) {
    let project_id = common::create_test_project(app, cookie, csrf).await;
    let agent_id = common::create_test_agent_from_preset(app, "Worker", cookie, csrf).await;
    let ticket_id = common::create_test_ticket(app, &project_id, cookie, csrf).await;
    common::set_ticket_repo(app, &ticket_id, &repo_id, cookie, csrf).await;
    common::assign_agent_to_ticket(app, &ticket_id, &agent_id, cookie, csrf).await;
    (ticket_id, agent_id, repo_id.to_string())
}

async fn post_run_agent(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, Option<serde_json::Value>) {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/run-agent"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    if body.is_empty() {
        (status, None)
    } else {
        (status, Some(serde_json::from_slice(&body).unwrap()))
    }
}

async fn poll_run_until(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
    target: &str,
    timeout: Duration,
) -> serde_json::Value {
    let deadline = Instant::now() + timeout;
    loop {
        let res = app
            .clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/tickets/{ticket_id}/runs"),
                "",
                cookie,
                csrf,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body: serde_json::Value = common::json_body(res).await;
        if let Some(run) = body["runs"].as_array().and_then(|runs| runs.first()) {
            if run["status"].as_str() == Some(target) {
                return run.clone();
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for run status {target}");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn run_agent_applies_done_fixture() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) =
        setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = body.as_ref().unwrap()["run"]["id"].as_str().unwrap();

    let run = poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "succeeded",
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(run["id"].as_str().unwrap(), run_id);

    let ticket = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ticket.status(), StatusCode::OK);
    let ticket: serde_json::Value = common::json_body(ticket).await;
    assert_eq!(ticket["status"], "in_review");

    let comments = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/comments"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(comments.status(), StatusCode::OK);
    let comments: serde_json::Value = common::json_body(comments).await;
    let agent_comment = comments
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["authorType"] == "agent")
        .expect("agent comment");
    assert_eq!(agent_comment["intent"], "implementation_done");
}

#[tokio::test]
async fn run_agent_applies_blocked_fixture() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("blocked").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) =
        setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, _) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);

    poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "blocked",
        Duration::from_secs(10),
    )
    .await;

    let ticket = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let ticket: serde_json::Value = common::json_body(ticket).await;
    assert_eq!(ticket["status"], "in_progress");
    assert_eq!(ticket["substatus"], "blocked_by_error");

    let comments = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/comments"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let comments: serde_json::Value = common::json_body(comments).await;
    let agent_comment = comments
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["authorType"] == "agent")
        .expect("agent comment");
    assert_eq!(agent_comment["intent"], "blocked");
}

#[tokio::test]
async fn reject_second_run_while_active() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) =
        setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, _) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);

    let (second_status, _) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(second_status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn stop_queued_run_cancels() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) =
        setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = body.as_ref().unwrap()["run"]["id"].as_str().unwrap();

    let stop = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/agent-runs/{run_id}/stop"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(stop.status(), StatusCode::OK);
    let stopped: serde_json::Value = common::json_body(stop).await;
    assert_eq!(stopped["run"]["status"], "cancelled");

    let run = poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "cancelled",
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(run["id"].as_str().unwrap(), run_id);
}

#[tokio::test]
async fn reject_run_when_repo_path_missing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) =
        setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    drop(git_dir);

    let (status, _body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn retry_after_failed_creates_new_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_workers("nonexistent-fixture").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) =
        setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let failed_run_id = body.as_ref().unwrap()["run"]["id"].as_str().unwrap().to_string();

    let failed_run = poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "failed",
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(failed_run["id"].as_str().unwrap(), failed_run_id);

    let error_message = failed_run["errorMessage"].as_str().unwrap_or("");
    assert!(error_message.len() > "ensure worktree".len());
    assert!(error_message.contains("fixture"));

    std::env::set_var("MOCK_AGENT_RESPONSE", "done");

    let retry = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/agent-runs/{failed_run_id}/retry"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::CREATED);
    let retry_body: serde_json::Value = common::json_body(retry).await;
    let new_run_id = retry_body["run"]["id"].as_str().unwrap();
    assert_ne!(new_run_id, failed_run_id);

    poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "succeeded",
        Duration::from_secs(10),
    )
    .await;
}

#[tokio::test]
async fn reject_run_when_agent_provider_missing_config() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let create_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            r#"{"name":"OpenCode Bot","role":"Developer","systemPrompt":"You are a developer","connector":"opencode"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);
    let agent: serde_json::Value = common::json_body(create_res).await;
    let agent_id = agent["id"].as_str().unwrap();

    coppice_server::workers::health_worker::run_health_pass_once(&state).await;

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, agent_id, &cookie, &csrf).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let body = body.unwrap();
    assert_eq!(
        body["message"].as_str().unwrap(),
        "Connector 'opencode' is not configured on this server"
    );
}
