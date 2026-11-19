mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use coppice_server::domain::plugin_build::NO_AGENT_READY_TO_BUILD;
use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::services::plugin_build_service::PluginBuildService;
use coppice_server::services::run_service::RunService;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::path::Path;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

macro_rules! require_db {
    () => {
        if !common::db_available().await {
            eprintln!("skipping: postgres not available");
            return;
        }
    };
}

async fn login_as(app: &Router, email: &str, password: &str) -> (String, String) {
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"email":"{email}","password":"{password}"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let set_cookie = login.headers().get(header::SET_COOKIE).unwrap();
    let token = parse_session_cookie(set_cookie.to_str().unwrap()).unwrap();
    let body = common::json_body(login).await;
    (
        format!("coppice_session={token}"),
        body["csrfToken"].as_str().unwrap().to_string(),
    )
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, Value) {
    use http_body_util::BodyExt;
    let res = app
        .clone()
        .oneshot(common::json_request(
            method,
            uri,
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

#[tokio::test]
async fn create_get_list_cancel_discard_and_accept_cleanup() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["status"], "drafting");
    assert!(created["answers"].is_object());
    assert_eq!(created["plans"], json!([]));
    assert!(created["pluginName"].is_null());
    assert!(created["publishTarget"].is_null());
    let id = created["id"].as_str().unwrap();
    let workspace = created["workspacePath"].as_str().unwrap();
    let workspace_path = Path::new(workspace);
    assert!(
        workspace.contains("plugin-builds"),
        "workspace path {workspace}"
    );
    assert!(workspace_path.join(".git").exists(), "missing .git");

    let (status, got) = send(
        &app,
        "GET",
        &format!("/api/plugin-builds/{id}"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["id"], id);
    assert_eq!(got["workspacePath"], workspace);

    let (status, patched) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{id}"),
        json!({
            "answers": {"q1": "cli"},
            "pluginName": "hello",
            "publishTarget": "https://example.test/plugin.git",
            "planRevision": {"body": "plan v1", "changeRequest": "tweak"}
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");
    assert_eq!(patched["answers"]["q1"], "cli");
    assert_eq!(patched["pluginName"], "hello");
    assert_eq!(patched["publishTarget"], "https://example.test/plugin.git");
    assert_eq!(patched["plans"][0]["revision"], 1);
    assert_eq!(patched["plans"][0]["body"], "plan v1");
    assert_eq!(patched["plans"][0]["changeRequest"], "tweak");

    let (status, listed) = send(&app, "GET", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::OK);
    let listed = listed.as_array().unwrap();
    assert!(listed.iter().any(|b| b["id"] == id));

    // Cancel keeps the workspace.
    let (status, cancelled) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{id}/cancel"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["status"], "cancelled");
    assert!(workspace_path.exists());

    // Fresh build for discard.
    let (status, created2) =
        send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let id2 = created2["id"].as_str().unwrap().to_string();
    let workspace2 = Path::new(created2["workspacePath"].as_str().unwrap()).to_path_buf();
    assert!(workspace2.exists());

    let (status, discarded) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{id2}/discard"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{discarded}");
    assert_eq!(discarded["status"], "discarded");
    assert!(!workspace2.exists());

    let (status, listed_default) =
        send(&app, "GET", "/api/plugin-builds", Value::Null, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed_default
        .as_array()
        .unwrap()
        .iter()
        .all(|b| b["id"] != id2));

    let (status, listed_all) = send(
        &app,
        "GET",
        "/api/plugin-builds?includeDiscarded=true",
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed_all
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["id"] == id2));

    // Accept cleanup deletes workspace and marks accepted.
    let (status, created3) =
        send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let id3: uuid::Uuid = created3["id"].as_str().unwrap().parse().unwrap();
    let workspace3 = Path::new(created3["workspacePath"].as_str().unwrap()).to_path_buf();
    let pool = state.db.as_ref().unwrap();
    let accepted = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone())
        .delete_workspace_on_accept(id3)
        .await
        .unwrap();
    assert_eq!(accepted.status, "accepted");
    assert!(!workspace3.exists());
}

#[tokio::test]
async fn non_admin_gets_403_and_unknown_id_gets_404() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let create = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/users",
            r#"{"email":"member@localhost","password":"secret123"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let (member_cookie, member_csrf) = login_as(&app, "member@localhost", "secret123").await;

    for (method, uri) in [
        ("POST", "/api/plugin-builds"),
        ("GET", "/api/plugin-builds"),
        (
            "GET",
            "/api/plugin-builds/00000000-0000-0000-0000-000000000001",
        ),
        (
            "PATCH",
            "/api/plugin-builds/00000000-0000-0000-0000-000000000001",
        ),
        (
            "POST",
            "/api/plugin-builds/00000000-0000-0000-0000-000000000001/cancel",
        ),
        (
            "POST",
            "/api/plugin-builds/00000000-0000-0000-0000-000000000001/discard",
        ),
    ] {
        let body = if method == "PATCH" {
            json!({"pluginName": "x"})
        } else {
            Value::Null
        };
        let (status, _) = send(&app, method, uri, body, &member_cookie, &member_csrf).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
    }

    // Demoting the session user is another 403 path; unknown id as admin is 404.
    let _ = state;
    let (status, _) = send(
        &app,
        "GET",
        "/api/plugin-builds/00000000-0000-0000-0000-000000000001",
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(
        &app,
        "POST",
        "/api/plugin-builds/00000000-0000-0000-0000-000000000001/cancel",
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn wait_for_run_status(pool: &PgPool, run_id: Uuid, wanted: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(pool)
            .await
            .unwrap();
        if status == wanted {
            return;
        }
        assert!(
            !matches!(status.as_str(), "failed" | "cancelled" | "blocked") || status == wanted,
            "run ended as {status}, wanted {wanted}"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for run {run_id} to be {wanted}; last={status}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn plan_run_persists_revision_with_advanced_and_ask_for_changes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("plugin_plan_done", |_| {}).await;
    let pool = state.db.as_ref().unwrap();
    let _pm = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let _be = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap();

    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{id}"),
        json!({
            "answers": {
                "purpose": "changelog",
                "jobs": ["summarize commits"],
                "tools": ["git"],
                "secrets": "none",
                "where": "local"
            }
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, started) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{id}/plan-runs"),
        json!({}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(started["build"]["status"], "planning");
    let run_id: Uuid = started["runId"].as_str().unwrap().parse().unwrap();
    wait_for_run_status(pool, run_id, "succeeded").await;

    let (status, got) = send(
        &app,
        "GET",
        &format!("/api/plugin-builds/{id}"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["status"], "plan_ready");
    assert_eq!(got["plans"][0]["revision"], 1);
    assert!(got["plans"][0]["body"].as_str().unwrap().contains("changelog-helper"));
    assert_eq!(got["plans"][0]["advanced"]["pluginName"], "changelog-helper");
    assert_eq!(got["pluginName"], "changelog-helper");

    let (status, revision2) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{id}/plan-runs"),
        json!({ "changeRequest": "Make the description shorter." }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{revision2}");
    let run2: Uuid = revision2["runId"].as_str().unwrap().parse().unwrap();
    wait_for_run_status(pool, run2, "succeeded").await;

    let (status, got2) = send(
        &app,
        "GET",
        &format!("/api/plugin-builds/{id}"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{got2}");
    assert_eq!(got2["status"], "plan_ready");
    assert_eq!(got2["plans"].as_array().unwrap().len(), 2);
    assert_eq!(
        got2["plans"][1]["changeRequest"],
        "Make the description shorter."
    );
}

#[tokio::test]
async fn build_run_writes_workspace_files_and_keeps_building() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("plugin_build_done", |_| {}).await;
    let pool = state.db.as_ref().unwrap();
    let _pm = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let _be = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap();
    let workspace = Path::new(created["workspacePath"].as_str().unwrap());

    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{id}"),
        json!({
            "answers": { "purpose": "changelog" },
            "planRevision": { "body": "approved plan" },
            "pluginName": "changelog-helper"
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Move to plan_ready (PATCH alone leaves drafting).
    let build_id: Uuid = id.parse().unwrap();
    sqlx::query("UPDATE plugin_builds SET status = 'plan_ready' WHERE id = $1")
        .bind(build_id)
        .execute(pool)
        .await
        .unwrap();

    let (status, started) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{id}/build-runs"),
        json!({}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(started["build"]["status"], "building");
    let run_id: Uuid = started["runId"].as_str().unwrap().parse().unwrap();
    wait_for_run_status(pool, run_id, "succeeded").await;

    assert!(workspace.join("README.md").is_file());
    assert!(workspace.join(".claude-plugin/plugin.json").is_file());

    let (status, got) = send(
        &app,
        "GET",
        &format!("/api/plugin-builds/{id}"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["status"], "building");
    // finish_run clears submitted_result after apply; success is the files + status.
    let _ = run_id;
}

#[tokio::test]
async fn plan_run_errors_when_no_ready_agent() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{id}"),
        json!({ "answers": { "purpose": "x" } }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // No PM agent on the board.
    let (status, err) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{id}/plan-runs"),
        json!({}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{err}");
    assert_eq!(err["error"], NO_AGENT_READY_TO_BUILD);
}

#[tokio::test]
async fn plan_run_errors_when_connector_not_ready() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();
    let _pm = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{id}"),
        json!({ "answers": { "purpose": "x" } }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let err = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone())
        .queue_plan_run(id, None, |_| false)
        .await
        .expect_err("connector not ready");
    assert!(
        matches!(
            err,
            coppice_server::services::plugin_build_service::PluginBuildError::Conflict(ref msg)
                if msg == NO_AGENT_READY_TO_BUILD
        ),
        "{err}"
    );
}

#[tokio::test]
async fn cancelling_queued_build_run_reverts_to_plan_ready() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("plugin_build_done", |_| {}).await;
    let pool = state.db.as_ref().unwrap();
    let _pm = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let _be = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    let workspace = Path::new(created["workspacePath"].as_str().unwrap()).to_path_buf();
    let build_id: Uuid = id.parse().unwrap();

    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{id}"),
        json!({
            "answers": { "purpose": "changelog" },
            "planRevision": { "body": "plan" }
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    sqlx::query("UPDATE plugin_builds SET status = 'plan_ready' WHERE id = $1")
        .bind(build_id)
        .execute(pool)
        .await
        .unwrap();

    // Hold mock slots so the worker cannot claim the job while it stays queued.
    let mock_cap = state.config.agent.connectors.max_concurrent_runs("mock") as usize;
    let _held: Vec<_> = (0..mock_cap)
        .map(|_| state.connector_slots.acquire("mock"))
        .collect();

    let (status, started) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{id}/build-runs"),
        json!({}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(started["build"]["status"], "building");
    let run_id: Uuid = started["runId"].as_str().unwrap().parse().unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(status, "queued", "run must still be queued under saturated slots");

    // stop() cancels the pending job (worker never claims) and must revert status.
    RunService::new(pool).stop(run_id).await.unwrap();
    wait_for_run_status(pool, run_id, "cancelled").await;

    let build = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone())
        .get(build_id)
        .await
        .unwrap();
    assert_eq!(build.status, "plan_ready");
    assert!(workspace.exists(), "workspace must remain after queued cancel");
    drop(_held);
}

#[tokio::test]
async fn stopping_build_run_reverts_to_plan_ready_without_discard() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    // Slow fixture so we can stop mid-run.
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("plugin_build_slow", |_| {}).await;
    let pool = state.db.as_ref().unwrap();
    let _pm = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let _be = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    let workspace = Path::new(created["workspacePath"].as_str().unwrap()).to_path_buf();
    let build_id: Uuid = id.parse().unwrap();

    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{id}"),
        json!({
            "answers": { "purpose": "changelog" },
            "planRevision": { "body": "plan" }
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    sqlx::query("UPDATE plugin_builds SET status = 'plan_ready' WHERE id = $1")
        .bind(build_id)
        .execute(pool)
        .await
        .unwrap();

    let (status, started) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{id}/build-runs"),
        json!({}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    let run_id: Uuid = started["runId"].as_str().unwrap().parse().unwrap();
    wait_for_run_status(pool, run_id, "running").await;

    // Cancel the agent run (Stop build), not the build record.
    RunService::new(pool).stop(run_id).await.unwrap();
    if let Some(handle) = state.run_streams.get(run_id) {
        handle.cancel();
    }
    wait_for_run_status(pool, run_id, "cancelled").await;

    // Worker reconciles build status after JobCancelled.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let build = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone())
            .get(build_id)
            .await
            .unwrap();
        if build.status == "plan_ready" {
            assert!(workspace.exists(), "workspace must remain after stop");
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for plan_ready; status={}",
            build.status
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
