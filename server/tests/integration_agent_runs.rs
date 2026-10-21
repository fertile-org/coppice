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
    let board_id = common::create_test_board(app, cookie, csrf).await;
    let agent_id = common::create_test_agent_from_preset(app, "Worker", cookie, csrf).await;
    let ticket_id = common::create_test_ticket(app, &board_id, cookie, csrf).await;
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
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

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
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

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
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

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
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

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
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

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
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let failed_run_id = body.as_ref().unwrap()["run"]["id"]
        .as_str()
        .unwrap()
        .to_string();

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

    let off = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            "/api/tools/connectors/opencode",
            r#"{"enabled":false}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(off.status(), StatusCode::OK);

    coppice_server::workers::health_worker::run_health_pass_once(&state).await;

    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
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
        "OpenCode is turned off. Turn it on in System → Connectors, or switch this agent to another connector."
    );
}

fn process_rss_kb() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("status");
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .split_whitespace()
                .next()
                .expect("VmRSS value")
                .parse()
                .expect("VmRSS kb");
        }
    }
    panic!("VmRSS missing from /proc/self/status");
}

async fn wait_stream_gone(state: &std::sync::Arc<coppice_server::AppState>, run_id: uuid::Uuid) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if state.run_streams.get(run_id).is_none() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("run stream {run_id} was still registered after the run ended");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Ten sequential MockProvider runs on one ticket. Server RSS is sampled after
/// runs 1, 5 and 10. Growth during a real desktop run is rustc/cargo in the
/// connector tree, not this in-process mock path — this catches a leftover
/// live buffer or stream handle.
#[tokio::test]
async fn ten_mock_runs_do_not_grow_server_rss_or_leave_streams() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("backend_engineer/continued", |_| {})
            .await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let mut samples = Vec::new();
    let mut run_ids = Vec::new();
    for n in 1..=10 {
        let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
        assert_eq!(status, StatusCode::CREATED, "run {n}");
        let run_id = body.as_ref().unwrap()["run"]["id"]
            .as_str()
            .unwrap()
            .parse::<uuid::Uuid>()
            .expect("run id");
        run_ids.push(run_id);

        let run = poll_run_until(
            &app,
            &ticket_id,
            &cookie,
            &csrf,
            "succeeded",
            Duration::from_secs(15),
        )
        .await;
        assert_eq!(run["id"].as_str().unwrap(), run_id.to_string());
        wait_stream_gone(&state, run_id).await;

        if n == 1 || n == 5 || n == 10 {
            samples.push((n, process_rss_kb()));
        }
    }

    for run_id in &run_ids {
        assert!(
            state.run_streams.get(*run_id).is_none(),
            "stream {run_id} leaked after ten runs"
        );
    }

    let after1 = samples[0].1;
    let slack = after1 / 100 * 15 + 8 * 1024;
    for (n, rss) in &samples[1..] {
        assert!(
            *rss <= after1 + slack,
            "server RSS grew steadily across mock runs: after 1 = {after1} KB, after {n} = {rss} KB (samples {samples:?})"
        );
    }
}
