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

#[tokio::test]
async fn run_agent_waits_for_the_startup_connector_check() {
    use coppice_server::services::agent_health::AgentHealthStatus;
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    // No `enabled` key, whatever a local config.toml says: Automatic.
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.agent.connectors.claude_code.enabled = None;
    })
    .await;
    let create_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            r#"{"name":"Claude Bot","role":"Developer","systemPrompt":"You are a developer","connector":"mock"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);
    let agent: serde_json::Value = common::json_body(create_res).await;
    let agent_id = agent["id"].as_str().unwrap();
    let agent_uuid = uuid::Uuid::parse_str(agent_id).unwrap();
    // Saving an agent turns its connector on; an older agent can still be
    // on an Automatic one.
    sqlx::query("UPDATE agents SET connector = 'claude-code' WHERE id = $1")
        .bind(agent_uuid)
        .execute(state.db.as_ref().unwrap())
        .await
        .unwrap();

    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, agent_id, &cookie, &csrf).await;

    // Startup: Claude Code has no `enabled` key, so it is off until checked,
    // and a health pass made now saves "turned off".
    state.connectors.begin_readiness_check();
    coppice_server::workers::health_worker::run_health_pass_once(&state).await;
    assert_eq!(
        state.agent_health.get(agent_uuid).status,
        AgentHealthStatus::MissingConfig
    );

    let run = tokio::spawn({
        let app = app.clone();
        let (ticket_id, cookie, csrf) = (ticket_id.clone(), cookie.clone(), csrf.clone());
        async move { post_run_agent(&app, &ticket_id, &cookie, &csrf).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(!run.is_finished(), "Run Agent answered before the check finished");
    state
        .connectors
        .finish_readiness_check([coppice_connectors::CLAUDE_CODE]);

    let (status, body) = run.await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    assert_eq!(
        state.agent_health.get(agent_uuid).status,
        AgentHealthStatus::Healthy
    );
}

/// Resident memory in KiB: `/proc` on Linux, `ps` where there is no `/proc` (macOS).
fn process_rss_kb() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .expect("ps");
        assert!(output.status.success(), "ps: {output:?}");
        return String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .expect("ps rss kb");
    };
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

async fn get_json(
    app: &axum::Router,
    uri: &str,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(common::json_request("GET", uri, "", cookie, csrf))
        .await
        .unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (status, json)
}

#[tokio::test]
async fn run_usage_is_stored_and_rolled_up_for_the_ticket() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("usage").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = body.unwrap()["run"]["id"].as_str().unwrap().to_string();
    poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "succeeded",
        Duration::from_secs(10),
    )
    .await;

    let (status, run) = get_json(
        &app,
        &format!("/api/agent-runs/{run_id}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(run["totals"]["status"], "reported");
    assert_eq!(run["totals"]["tokens"]["output"], 120);
    assert_eq!(run["totals"]["tokens"]["cacheRead"], 2200);
    assert_eq!(run["totals"]["requestCount"], 2);
    assert_eq!(run["totals"]["subagentCount"], 1);
    assert!(run["totals"].get("costUsd").is_none(), "cost is not served");
    let requests = run["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["id"], "req-1");
    assert_eq!(requests[0]["granularity"], "request");
    assert_eq!(requests[1]["tokens"]["reasoning"], 10);
    // Two events for one subagent are merged into one row.
    let subagents = run["subagents"].as_array().unwrap();
    assert_eq!(subagents.len(), 1);
    assert_eq!(subagents[0]["agentType"], "explore");
    assert_eq!(subagents[0]["status"], "completed");
    assert_eq!(subagents[0]["tokens"]["output"], 40);
    assert_eq!(subagents[0]["toolUses"], 2);

    let (status, ticket) = get_json(
        &app,
        &format!("/api/tickets/{ticket_id}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ticket["own"]["runs"], 1);
    assert_eq!(ticket["own"]["runsWithUsage"], 1);
    assert_eq!(ticket["own"]["tokens"]["output"], 120);
    assert!(ticket["subtickets"].is_null());
    assert_eq!(ticket["runs"][0]["runId"], run_id.as_str());
    assert_eq!(ticket["runs"][0]["tracked"], true);
    assert_eq!(ticket["byAgent"][0]["key"], agent_id.as_str());
    assert_eq!(ticket["byJobType"][0]["key"], "work_on_ticket");

    let missing = uuid::Uuid::new_v4();
    let (status, _) = get_json(
        &app,
        &format!("/api/agent-runs/{missing}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get_json(
        &app,
        &format!("/api/tickets/{missing}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn run_without_usage_reports_none() {
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
    let run_id = body.unwrap()["run"]["id"].as_str().unwrap().to_string();
    poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "succeeded",
        Duration::from_secs(10),
    )
    .await;

    let (status, run) = get_json(
        &app,
        &format!("/api/agent-runs/{run_id}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(run["totals"].is_null());
    assert_eq!(run["requests"].as_array().unwrap().len(), 0);

    let (_, ticket) = get_json(
        &app,
        &format!("/api/tickets/{ticket_id}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(ticket["own"]["runs"], 1);
    assert_eq!(ticket["own"]["runsWithUsage"], 0);
    assert!(ticket["own"]["tokens"]["output"].is_null());
    assert!(ticket["runs"][0]["usage"].is_null());
}

/// Waits for an event matching `pick`, or returns None at the deadline.
async fn drain_events<T>(
    events: &mut tokio::sync::broadcast::Receiver<coppice_server::events::AppEvent>,
    timeout: Duration,
    mut pick: impl FnMut(&coppice_server::events::AppEvent) -> Option<T>,
) -> Option<T> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Ok(event)) => {
                if let Some(found) = pick(&event) {
                    return Some(found);
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

/// Stopping a run publishes `agent_run.finished` before the worker saves the
/// final snapshot, so the worker has to announce the save itself. The test does
/// not assert which of the two events arrives first.
#[tokio::test]
async fn stopped_run_publishes_usage_updated_after_final_save() {
    use coppice_server::events::AppEvent;

    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("usage_hold", |_| {}).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;
    let ticket_uuid = uuid::Uuid::parse_str(&ticket_id).unwrap();

    let mut events = state.event_bus.subscribe();
    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = body.unwrap()["run"]["id"].as_str().unwrap().to_string();
    let run_uuid = uuid::Uuid::parse_str(&run_id).unwrap();

    // The mock replays usage before it holds, and the live writer saves it.
    // Stopping earlier could cancel the run before any usage exists.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (_, run) = get_json(
            &app,
            &format!("/api/agent-runs/{run_id}/usage"),
            &cookie,
            &csrf,
        )
        .await;
        if !run["totals"].is_null() {
            break;
        }
        assert!(Instant::now() < deadline, "usage never reported");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

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

    let mut finished = false;
    let mut usage_updated = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !(finished && usage_updated) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let seen = drain_events(&mut events, remaining, |event| match event {
            AppEvent::AgentRunFinished {
                run_id, status, ..
            } if *run_id == run_uuid => {
                assert_eq!(status, "cancelled");
                Some(true)
            }
            AppEvent::UsageUpdated {
                run_id,
                ticket_id,
            } if *run_id == run_uuid => {
                assert_eq!(*ticket_id, ticket_uuid);
                Some(false)
            }
            _ => None,
        })
        .await
        .expect("timed out waiting for agent_run.finished and usage.updated");
        if seen {
            finished = true;
        } else {
            usage_updated = true;
        }
    }

    let (_, run) = get_json(
        &app,
        &format!("/api/agent-runs/{run_id}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(run["totals"]["tokens"]["output"], 120);
}

#[tokio::test]
async fn run_without_usage_publishes_no_usage_updated() {
    use coppice_server::events::AppEvent;

    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("done", |_| {}).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let mut events = state.event_bus.subscribe();
    let (status, _) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "succeeded",
        Duration::from_secs(10),
    )
    .await;

    // The worker commits `succeeded` before it publishes `agent_run.finished`,
    // so wait for the finish rather than draining what is already queued. A
    // `usage.updated` would be published before both.
    drain_events(&mut events, Duration::from_secs(10), |event| match event {
        AppEvent::UsageUpdated { .. } => panic!("no usage was reported, got usage.updated"),
        AppEvent::AgentRunFinished { .. } => Some(()),
        _ => None,
    })
    .await
    .expect("timed out waiting for agent_run.finished");
}

/// `tracked` follows the connector that ran, not the agent's current one, and
/// a stored usage row always proves the run was tracked.
#[tokio::test]
async fn usage_tracked_flag_survives_an_agent_connector_switch() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Usage Tracked Agent",
        &cookie,
        &csrf,
    )
    .await;
    let ticket_uuid = uuid::Uuid::parse_str(&ticket_id).unwrap();
    let agent_uuid = uuid::Uuid::parse_str(&agent_id).unwrap();

    let insert_run = |session_connector: Option<&'static str>, with_usage: bool| async move {
        let run_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs
                (id, ticket_id, agent_id, job_type, status, sandbox_profile_id, session_connector)
            VALUES ($1, $2, $3, 'work_on_ticket', 'succeeded', 'permissive', $4)
            "#,
        )
        .bind(run_id)
        .bind(ticket_uuid)
        .bind(agent_uuid)
        .bind(session_connector)
        .execute(pool)
        .await
        .unwrap();
        if with_usage {
            sqlx::query(
                "INSERT INTO run_usage (run_id, status, version, output_tokens) VALUES ($1, 'reported', 1, 5)",
            )
            .bind(run_id)
            .execute(pool)
            .await
            .unwrap();
        }
        run_id
    };
    // (a) plan-run shape: usage row, no session connector.
    let plan_shaped = insert_run(None, true).await;
    // (b) a Claude session run with no usage row.
    let claude_session = insert_run(Some("claude-code"), false).await;
    // (c) a Codex session run.
    let codex_session = insert_run(Some("codex"), false).await;

    let tracked = |ticket: &serde_json::Value, run_id: uuid::Uuid| -> bool {
        ticket["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["runId"] == run_id.to_string().as_str())
            .unwrap()["tracked"]
            .as_bool()
            .unwrap()
    };

    // The agent switches to Codex: a and b stay tracked, c stays untracked.
    sqlx::query("UPDATE agents SET connector = 'codex' WHERE id = $1")
        .bind(agent_uuid)
        .execute(pool)
        .await
        .unwrap();
    let (status, ticket) = get_json(
        &app,
        &format!("/api/tickets/{ticket_id}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(tracked(&ticket, plan_shaped), "usage row, agent on codex");
    assert!(
        tracked(&ticket, claude_session),
        "claude session, agent on codex"
    );
    assert!(
        !tracked(&ticket, codex_session),
        "codex session, agent on codex"
    );

    // And back to Claude: c is still a Codex run.
    sqlx::query("UPDATE agents SET connector = 'claude-code' WHERE id = $1")
        .bind(agent_uuid)
        .execute(pool)
        .await
        .unwrap();
    let (_, ticket) = get_json(
        &app,
        &format!("/api/tickets/{ticket_id}/usage"),
        &cookie,
        &csrf,
    )
    .await;
    assert!(tracked(&ticket, plan_shaped));
    assert!(tracked(&ticket, claude_session));
    assert!(
        !tracked(&ticket, codex_session),
        "codex session, agent on claude"
    );
}
