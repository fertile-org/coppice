use coppice_server::domain::Id;
mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use coppice_server::domain::plugin_build::NO_AGENT_READY_TO_BUILD;
use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::services::plugin_build_service::PluginBuildService;
use coppice_server::services::plugin_service::PluginService;
use coppice_server::services::run_service::RunService;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tower::ServiceExt;
fn builder_check_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/plugins/builder-check")
        .join(name)
}

fn copy_tree(src: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

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
    let id3: Id = created3["id"].as_str().unwrap().parse().unwrap();
    let workspace3 = Path::new(created3["workspacePath"].as_str().unwrap()).to_path_buf();
    let pool = state.db.as_ref().unwrap();
    sqlx::query("UPDATE plugin_builds SET status = 'ready_to_try' WHERE id = $1")
        .bind(id3)
        .execute(pool)
        .await
        .unwrap();
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

async fn wait_for_run_status(pool: &PgPool, run_id: Id, wanted: &str) {
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
    let run_id: Id = started["runId"].as_str().unwrap().parse().unwrap();
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
    let run2: Id = revision2["runId"].as_str().unwrap().parse().unwrap();
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
    let build_id: Id = id.parse().unwrap();
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
    let run_id: Id = started["runId"].as_str().unwrap().parse().unwrap();
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
    let id: Id = created["id"].as_str().unwrap().parse().unwrap();
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
    let build_id: Id = id.parse().unwrap();

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
    let run_id: Id = started["runId"].as_str().unwrap().parse().unwrap();

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
    let build_id: Id = id.parse().unwrap();

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
    let run_id: Id = started["runId"].as_str().unwrap().parse().unwrap();
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

#[tokio::test]
async fn finalize_installs_disabled_and_auto_tests() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let build_id: Id = created["id"].as_str().unwrap().parse().unwrap();
    let workspace = Path::new(created["workspacePath"].as_str().unwrap());

    // Agent writes only in the workspace; plant a valid fixture there.
    copy_tree(&builder_check_fixture("pass-minimal"), workspace);
    // Workspace may also have .git / .agent from create — install must skip them.
    std::fs::create_dir_all(workspace.join(".agent")).unwrap();
    std::fs::write(workspace.join(".agent/context.md"), "scratch").unwrap();

    sqlx::query("UPDATE plugin_builds SET status = 'building' WHERE id = $1")
        .bind(build_id)
        .execute(pool)
        .await
        .unwrap();

    let builds = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone());
    let plugins = PluginService::new(pool);
    let build = builds
        .finalize_after_build(
            build_id,
            &plugins,
            &state.config.plugins.dir,
            Path::new(&state.config.plugins.marketplaces_dir),
            &state.secret_store,
            &state.plugin_mcp,
            &state.skills,
            |_| true,
        )
        .await
        .expect("finalize");

    assert_eq!(build.status, "ready_to_try");
    assert!(build.check_errors.is_none());
    assert!(build.plugin_id.is_some());
    assert!(build.test_result.is_some());
    let servers = build.test_result.as_ref().unwrap()["servers"]
        .as_array()
        .unwrap();
    assert!(servers.is_empty(), "skills-only → empty servers is a pass");

    let installed = Path::new(&state.config.plugins.dir).join("pass-minimal");
    assert!(installed.join(".claude-plugin/plugin.json").is_file());
    assert!(installed.join("skills/hello/SKILL.md").is_file());
    assert!(
        !installed.join(".agent").exists(),
        ".agent must not be copied into Installed"
    );
    assert!(
        !installed.join(".git").exists(),
        ".git must not be copied into Installed"
    );

    let row = plugins.get_plugin(build.plugin_id.unwrap()).await.unwrap();
    assert_eq!(row.status, "ok");
    assert!(!row.enabled);

    let (status, got) = send(
        &app,
        "GET",
        &format!("/api/plugin-builds/{build_id}"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["status"], "ready_to_try");
    assert_eq!(got["pluginName"], "pass-minimal");
    assert!(got["pluginId"].is_string());
    assert!(got["testResult"]["servers"].is_array());
}

#[tokio::test]
async fn finalize_reuses_name_when_prior_install_is_missing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();
    let plugins = PluginService::new(pool);
    let builds = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone());

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let first_id: Id = created["id"].as_str().unwrap().parse().unwrap();
    let workspace = Path::new(created["workspacePath"].as_str().unwrap());
    copy_tree(&builder_check_fixture("pass-minimal"), workspace);
    sqlx::query("UPDATE plugin_builds SET status = 'building' WHERE id = $1")
        .bind(first_id)
        .execute(pool)
        .await
        .unwrap();
    let first = builds
        .finalize_after_build(
            first_id,
            &plugins,
            &state.config.plugins.dir,
            Path::new(&state.config.plugins.marketplaces_dir),
            &state.secret_store,
            &state.plugin_mcp,
            &state.skills,
            |_| true,
        )
        .await
        .expect("first finalize");
    assert_eq!(first.status, "ready_to_try");
    let plugin_id = first.plugin_id.expect("installed");

    // Match Compose smoke: Accept with enable, then rm+rescan → missing+enabled.
    let (status, accepted) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{first_id}/accept"),
        json!({ "enable": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let enabled = plugins.get_plugin(plugin_id).await.unwrap();
    assert!(enabled.enabled);
    assert_eq!(enabled.status, "ok");

    let installed = Path::new(&state.config.plugins.dir).join("pass-minimal");
    std::fs::remove_dir_all(&installed).unwrap();
    plugins.rescan().await.unwrap();
    let prior = plugins.get_plugin(plugin_id).await.unwrap();
    assert_eq!(prior.status, "missing");
    assert!(prior.enabled, "Accept enable flag must survive missing");
    assert_eq!(prior.name, "pass-minimal");

    let (status, created2) =
        send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED, "{created2}");
    let second_id: Id = created2["id"].as_str().unwrap().parse().unwrap();
    let workspace2 = Path::new(created2["workspacePath"].as_str().unwrap());
    copy_tree(&builder_check_fixture("pass-minimal"), workspace2);
    sqlx::query("UPDATE plugin_builds SET status = 'building' WHERE id = $1")
        .bind(second_id)
        .execute(pool)
        .await
        .unwrap();
    let second = builds
        .finalize_after_build(
            second_id,
            &plugins,
            &state.config.plugins.dir,
            Path::new(&state.config.plugins.marketplaces_dir),
            &state.secret_store,
            &state.plugin_mcp,
            &state.skills,
            |_| true,
        )
        .await
        .expect("second finalize after Accept+enable+missing");
    assert_eq!(second.status, "ready_to_try", "{second:?}");
    assert!(second.check_errors.is_none(), "{:?}", second.check_errors);
    let revived = plugins
        .get_plugin(second.plugin_id.expect("reinstalled"))
        .await
        .unwrap();
    assert_eq!(revived.status, "ok");
    assert!(!revived.enabled, "builder reinstall must force disabled");
    assert!(installed.join(".claude-plugin/plugin.json").is_file());
}

#[tokio::test]
async fn finalize_check_failure_never_installs() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();

    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let build_id: Id = created["id"].as_str().unwrap().parse().unwrap();
    let workspace = Path::new(created["workspacePath"].as_str().unwrap());
    copy_tree(&builder_check_fixture("fail-gateway"), workspace);

    // Exhaust automatic retries so finalize only records errors.
    sqlx::query(
        "UPDATE plugin_builds SET status = 'building', format_retries = 2 WHERE id = $1",
    )
    .bind(build_id)
    .execute(pool)
    .await
    .unwrap();

    let builds = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone());
    let plugins = PluginService::new(pool);
    let build = builds
        .finalize_after_build(
            build_id,
            &plugins,
            &state.config.plugins.dir,
            Path::new(&state.config.plugins.marketplaces_dir),
            &state.secret_store,
            &state.plugin_mcp,
            &state.skills,
            |_| true,
        )
        .await
        .expect("finalize");

    assert_eq!(build.status, "building");
    assert!(build.plugin_id.is_none());
    assert!(build.test_result.is_none());
    let errors = build.check_errors.as_ref().unwrap().as_array().unwrap();
    assert!(!errors.is_empty());
    assert!(
        errors.iter().any(|e| e.as_str().unwrap().contains("gateway")),
        "{errors:?}"
    );
    assert!(
        !Path::new(&state.config.plugins.dir)
            .join("fail-gateway")
            .exists(),
        "half-written / failed check must not appear under Installed"
    );
}

#[tokio::test]
async fn try_again_and_ask_for_changes_from_check_failure() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
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
    let build_id: Id = created["id"].as_str().unwrap().parse().unwrap();

    sqlx::query(
        "UPDATE plugin_builds
         SET status = 'building',
             answers = $2::jsonb,
             plans = $3::jsonb,
             check_errors = $4::jsonb,
             format_retries = 2
         WHERE id = $1",
    )
    .bind(build_id)
    .bind(json!({ "purpose": "changelog" }))
    .bind(json!([{"revision": 1, "body": "plan"}]))
    .bind(json!(["Do not reference Coppice gateway tools (found `ticket_`)."]))
    .execute(pool)
    .await
    .unwrap();

    // Try again from building + check_errors.
    let (status, started) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{build_id}/build-runs"),
        json!({}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(started["build"]["status"], "building");
    assert!(started["build"]["checkErrors"].is_null());
    assert_eq!(started["build"]["formatRetries"], 0);

    // Stop the queued run so we can exercise Ask for changes next.
    let run_id: Id = started["runId"].as_str().unwrap().parse().unwrap();
    RunService::new(pool).stop(run_id).await.unwrap();
    wait_for_run_status(pool, run_id, "cancelled").await;

    // Re-seed check failure (stop reverted to plan_ready).
    sqlx::query(
        "UPDATE plugin_builds
         SET status = 'building', check_errors = $2::jsonb, format_retries = 2
         WHERE id = $1",
    )
    .bind(build_id)
    .bind(json!(["A skill is missing its description."]))
    .execute(pool)
    .await
    .unwrap();

    let (status, planned) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{build_id}/plan-runs"),
        json!({ "changeRequest": "Drop the gateway tool." }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{planned}");
    assert_eq!(planned["build"]["status"], "planning");
    assert!(planned["build"]["checkErrors"].is_null());
}

#[tokio::test]
async fn accept_enable_discard_uninstall_ask_from_ready_and_active_run_id() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
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

    // Finalize a build so we have ready_to_try + installed copy.
    let (status, created) = send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let build_id: Id = created["id"].as_str().unwrap().parse().unwrap();
    let workspace = Path::new(created["workspacePath"].as_str().unwrap());
    copy_tree(&builder_check_fixture("pass-minimal"), workspace);
    sqlx::query("UPDATE plugin_builds SET status = 'building' WHERE id = $1")
        .bind(build_id)
        .execute(pool)
        .await
        .unwrap();
    let builds = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone());
    let plugins = PluginService::new(pool);
    let build = builds
        .finalize_after_build(
            build_id,
            &plugins,
            &state.config.plugins.dir,
            Path::new(&state.config.plugins.marketplaces_dir),
            &state.secret_store,
            &state.plugin_mcp,
            &state.skills,
            |_| true,
        )
        .await
        .expect("finalize");
    assert_eq!(build.status, "ready_to_try");
    let plugin_id = build.plugin_id.unwrap();
    let installed = Path::new(&state.config.plugins.dir).join("pass-minimal");
    assert!(installed.exists());

    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{build_id}"),
        json!({ "answers": { "purpose": "changelog" } }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Ask for changes from ready_to_try keeps plugin_id for replace path.
    let (status, planned) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{build_id}/plan-runs"),
        json!({ "changeRequest": "Add a skill." }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{planned}");
    assert_eq!(planned["build"]["status"], "planning");
    assert_eq!(planned["build"]["pluginId"], plugin_id.to_string());
    assert!(planned["build"]["testResult"].is_null());
    assert!(planned["build"]["activeRunId"].is_string());

    // Cancel that plan run and restore ready_to_try for Accept.
    let plan_run: Id = planned["runId"].as_str().unwrap().parse().unwrap();
    RunService::new(pool).stop(plan_run).await.unwrap();
    wait_for_run_status(pool, plan_run, "cancelled").await;
    sqlx::query(
        "UPDATE plugin_builds
         SET status = 'ready_to_try', plugin_id = $2, test_result = $3::jsonb
         WHERE id = $1",
    )
    .bind(build_id)
    .bind(plugin_id)
    .bind(json!({ "servers": [] }))
    .execute(pool)
    .await
    .unwrap();

    let (status, accepted) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{build_id}/accept"),
        json!({ "enable": true, "publish": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["status"], "accepted");
    assert!(!workspace.exists());
    let row = plugins.get_plugin(plugin_id).await.unwrap();
    assert!(row.enabled);

    // Fresh build + plant an install to exercise discard uninstall.
    let (status, created2) =
        send(&app, "POST", "/api/plugin-builds", Value::Null, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let build_id2: Id = created2["id"].as_str().unwrap().parse().unwrap();
    let workspace2 = Path::new(created2["workspacePath"].as_str().unwrap()).to_path_buf();
    let dest = Path::new(&state.config.plugins.dir).join("discard-me");
    copy_tree(&builder_check_fixture("pass-minimal"), &dest);
    let manifest = dest.join(".claude-plugin/plugin.json");
    let mut body: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    body["name"] = json!("discard-me");
    std::fs::write(&manifest, serde_json::to_string_pretty(&body).unwrap()).unwrap();
    plugins.rescan().await.unwrap();
    let row = plugins
        .list_plugins()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.name == "discard-me")
        .expect("discard-me installed");
    sqlx::query(
        "UPDATE plugin_builds
         SET status = 'ready_to_try', plugin_id = $2, plugin_name = 'discard-me',
             test_result = $3::jsonb
         WHERE id = $1",
    )
    .bind(build_id2)
    .bind(row.id)
    .bind(json!({ "servers": [] }))
    .execute(pool)
    .await
    .unwrap();
    assert!(dest.exists());

    let (status, discarded) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{build_id2}/discard"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{discarded}");
    assert_eq!(discarded["status"], "discarded");
    assert!(!workspace2.exists());
    assert!(!dest.exists(), "installed copy must be removed");
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Bare marketplace repo with one commit on `main` (empty tree + marketplace.json).
fn marketplace_bare() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    let bare = tmp.path().join("market.git");
    std::fs::create_dir_all(&work).unwrap();
    git_in(&work, &["init", "-q"]);
    std::fs::create_dir_all(work.join(".claude-plugin")).unwrap();
    std::fs::write(
        work.join(".claude-plugin/marketplace.json"),
        r#"{"name":"team","plugins":[]}"#,
    )
    .unwrap();
    git_in(&work, &["add", "-A"]);
    git_in(&work, &["commit", "-q", "-m", "init"]);
    git_in(tmp.path(), &["clone", "-q", "--bare", "work", "market.git"]);
    let url = format!("file://{}", bare.display());
    (tmp, url)
}

fn ensure_git_identity() {
    std::env::set_var("GIT_AUTHOR_NAME", "Test");
    std::env::set_var("GIT_AUTHOR_EMAIL", "test@localhost");
    std::env::set_var("GIT_COMMITTER_NAME", "Test");
    std::env::set_var("GIT_COMMITTER_EMAIL", "test@localhost");
}

async fn ready_builder_plugin(
    state: &std::sync::Arc<coppice_server::AppState>,
    app: &Router,
    cookie: &str,
    csrf: &str,
    name: &str,
) -> (Id, Id) {
    let pool = state.db.as_ref().unwrap();
    let (status, created) = send(app, "POST", "/api/plugin-builds", Value::Null, cookie, csrf).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let build_id: Id = created["id"].as_str().unwrap().parse().unwrap();
    let workspace = Path::new(created["workspacePath"].as_str().unwrap());
    copy_tree(&builder_check_fixture("pass-minimal"), workspace);
    let manifest = workspace.join(".claude-plugin/plugin.json");
    let mut body: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    body["name"] = json!(name);
    std::fs::write(&manifest, serde_json::to_string_pretty(&body).unwrap()).unwrap();
    sqlx::query("UPDATE plugin_builds SET status = 'building', plugin_name = $2 WHERE id = $1")
        .bind(build_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    let builds = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone());
    let plugins = PluginService::new(pool);
    let build = builds
        .finalize_after_build(
            build_id,
            &plugins,
            &state.config.plugins.dir,
            Path::new(&state.config.plugins.marketplaces_dir),
            &state.secret_store,
            &state.plugin_mcp,
            &state.skills,
            |_| true,
        )
        .await
        .expect("finalize");
    assert_eq!(build.status, "ready_to_try");
    sqlx::query(
        "UPDATE plugin_builds SET test_result = $2::jsonb WHERE id = $1",
    )
    .bind(build_id)
    .bind(json!({ "servers": [] }))
    .execute(pool)
    .await
    .unwrap();
    (build_id, build.plugin_id.unwrap())
}

#[tokio::test]
async fn publish_accept_and_publish_api() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    ensure_git_identity();
    let (_market, url) = marketplace_bare();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|cfg| {
        cfg.plugins.allow_file_git_urls = true;
        cfg.git.push_enabled = true;
    })
    .await;

    let (build_id, plugin_id) =
        ready_builder_plugin(&state, &app, &cookie, &csrf, "pub-happy").await;

    let (status, patched) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{build_id}"),
        json!({ "publishTarget": url }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");

    let (status, accepted) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{build_id}/accept"),
        json!({ "enable": false, "publish": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["status"], "accepted");
    assert_eq!(accepted["publish"]["status"], "pushed", "{accepted}");
    assert_eq!(
        accepted["publish"]["branch"],
        "coppice/plugin-pub-happy",
        "{accepted}"
    );
    assert!(
        Path::new(&state.config.plugins.dir).join("pub-happy").exists(),
        "local install remains"
    );

    // Remote branch holds marketplace entry + plugin files.
    let check = tempfile::tempdir().unwrap();
    let work = check.path().join("check");
    git_in(
        check.path(),
        &[
            "clone",
            "-q",
            "--branch",
            "coppice/plugin-pub-happy",
            &url,
            "check",
        ],
    );
    let market = std::fs::read_to_string(work.join(".claude-plugin/marketplace.json")).unwrap();
    assert!(market.contains("pub-happy"), "{market}");
    assert!(market.contains("./plugins/pub-happy"), "{market}");
    assert!(work.join("plugins/pub-happy/.claude-plugin/plugin.json").is_file());

    // Branch already exists → conflict + suggested -2; confirmSuffix pushes it.
    let (status, conflict) = send(
        &app,
        "POST",
        &format!("/api/plugins/{plugin_id}/publish"),
        json!({ "url": url }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(
        conflict["suggestedBranch"],
        "coppice/plugin-pub-happy-2",
        "{conflict}"
    );

    let (status, pushed) = send(
        &app,
        "POST",
        &format!("/api/plugins/{plugin_id}/publish"),
        json!({ "url": url, "confirmSuffix": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pushed}");
    assert_eq!(pushed["status"], "pushed", "{pushed}");
    assert_eq!(pushed["branch"], "coppice/plugin-pub-happy-2", "{pushed}");

    // builderMade flag on list/get
    let (status, plugins) = send(&app, "GET", "/api/plugins", Value::Null, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    let row = plugins
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == plugin_id.to_string())
        .unwrap();
    assert_eq!(row["builderMade"], true, "{row}");
}

#[tokio::test]
async fn publish_bad_url_and_accept_survives_unreachable() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    ensure_git_identity();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|cfg| {
        cfg.plugins.allow_file_git_urls = true;
        cfg.git.push_enabled = true;
    })
    .await;

    let (build_id, plugin_id) =
        ready_builder_plugin(&state, &app, &cookie, &csrf, "pub-fail").await;

    let (status, patched) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-builds/{build_id}"),
        json!({ "publishTarget": "not-a-url" }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{patched}");

    let (status, accepted) = send(
        &app,
        "POST",
        &format!("/api/plugin-builds/{build_id}/accept"),
        json!({ "enable": false, "publish": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["status"], "accepted");
    assert_eq!(accepted["publish"]["status"], "failed", "{accepted}");
    assert_eq!(
        accepted["publish"]["error"],
        "That doesn't look like a repository URL.",
        "{accepted}"
    );
    assert!(
        Path::new(&state.config.plugins.dir).join("pub-fail").exists(),
        "local install remains after publish failure"
    );

    let (status, bad) = send(
        &app,
        "POST",
        &format!("/api/plugins/{plugin_id}/publish"),
        json!({ "url": "http://example.test/nope.git" }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    assert_eq!(
        bad["error"],
        "That doesn't look like a repository URL.",
        "{bad}"
    );

    let (status, unreachable) = send(
        &app,
        "POST",
        &format!("/api/plugins/{plugin_id}/publish"),
        json!({ "url": "file:///nonexistent/market.git" }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{unreachable}");
    assert!(
        unreachable["error"]
            .as_str()
            .unwrap_or("")
            .contains("Couldn't push to"),
        "{unreachable}"
    );
}
