mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use coppice_server::services::connector_check_service::ConnectorCheckService;
use coppice_server::services::connector_probe_service::{last_real_runs, ConnectorProbes};
use coppice_server::AppState;
use serde_json::Value;
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

const KILO: &str = "kilo-code";

/// A `kilo` script that prints the contents of a sibling `version` file, so
/// tests change the output without rewriting the executable.
struct FakeKilo {
    dir: tempfile::TempDir,
}

impl FakeKilo {
    fn new(version: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let fake = Self { dir };
        fake.set_version(version);
        let script = fake.script();
        std::fs::write(
            &script,
            "#!/bin/sh\nread -r v < \"${0%/*}/version\"\necho \"$v\"\n",
        )
        .expect("write script");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("chmod script");
        }
        fake.wait_until_executable();
        fake
    }

    fn script(&self) -> PathBuf {
        self.dir.path().join("kilo")
    }

    fn set_version(&self, version: &str) {
        std::fs::write(self.dir.path().join("version"), format!("{version}\n"))
            .expect("write version");
    }

    /// A concurrent fork can briefly inherit the write fd (ETXTBSY); once one
    /// exec succeeds no writer remains.
    fn wait_until_executable(&self) {
        for _ in 0..50 {
            match std::process::Command::new(self.script()).output() {
                Ok(out) if out.status.success() => return,
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        panic!("fake kilo never became executable");
    }
}

fn configure_kilo(script: &Path) -> impl FnOnce(&mut coppice_server::AppConfig) {
    let script = script.display().to_string();
    move |config| {
        config.agent.connectors.kilo_code.enabled = true;
        config.agent.connectors.kilo_code.command = script;
    }
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(common::json_request(method, uri, "", cookie, csrf))
        .await
        .unwrap();
    let status = res.status();
    let body = res.into_body();
    let bytes = http_body_util::BodyExt::collect(body)
        .await
        .unwrap()
        .to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

fn entry<'a>(list: &'a Value, id: &str) -> &'a Value {
    list.as_array()
        .expect("array")
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no entry {id} in {list}"))
}

#[tokio::test]
async fn connectors_lists_non_mock_descriptors() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let kilo = FakeKilo::new("kilo 9.9.9");
    let (state, app, cookie, csrf) =
        common::bootstrap_and_login_with_state_config(configure_kilo(&kilo.script())).await;
    state
        .connector_probes
        .refresh(&state.config, KILO)
        .await
        .expect("kilo probe");

    let (status, body) = send(&app, "GET", "/api/tools/connectors", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    let expected: Vec<&str> = coppice_connectors::all()
        .iter()
        .filter(|d| d.id != coppice_connectors::MOCK)
        .map(|d| d.id)
        .collect();
    assert_eq!(ids, expected);

    let k = entry(&body, KILO);
    assert_eq!(
        k["displayName"],
        coppice_connectors::get(KILO).unwrap().display_name
    );
    assert_eq!(k["enabled"], true);
    assert_eq!(k["cli"]["found"], true);
    assert_eq!(k["cli"]["path"], kilo.script().display().to_string());
    assert_eq!(k["cli"]["probe"]["status"], "ok");
    assert_eq!(k["cli"]["probe"]["detail"], "kilo 9.9.9");
    assert!(k["auth"]["envSet"].is_array());
    assert!(k["auth"]["pathsFound"].is_array());
    assert!(k["authHint"].is_string());
    assert!(k["docsUrl"].as_str().unwrap().starts_with("https://"));
    assert!(k["lastRun"].is_null());
    assert!(k["lastCheck"].is_null());
    assert!(k["probedAt"].is_string());

    let codex = entry(&body, coppice_connectors::CODEX);
    assert_eq!(codex["enabled"], false);
    assert!(codex["probedAt"].is_null(), "not probed yet: {codex}");
    assert_eq!(codex["cli"]["probe"]["status"], "not_run");
}

#[tokio::test]
async fn connectors_requires_admin() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tools/connectors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let pool = state.db.clone().unwrap();
    sqlx::query("UPDATE users SET role = 'member' WHERE email = 'admin@localhost'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, "GET", "/api/tools/connectors", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        &app,
        "POST",
        "/api/tools/connectors/kilo-code/check",
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn check_reprobes_one_connector() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let kilo = FakeKilo::new("kilo 9.9.9");
    let (state, app, cookie, csrf) =
        common::bootstrap_and_login_with_state_config(configure_kilo(&kilo.script())).await;
    state
        .connector_probes
        .refresh(&state.config, KILO)
        .await
        .expect("kilo probe");
    kilo.set_version("kilo 10.0.0");

    let (status, body) = send(
        &app,
        "POST",
        "/api/tools/connectors/kilo-code/check",
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], KILO);
    assert_eq!(body["cli"]["probe"]["status"], "ok");
    assert_eq!(body["cli"]["probe"]["detail"], "kilo 10.0.0");
    assert!(body["probedAt"].is_string());

    let (_, list) = send(&app, "GET", "/api/tools/connectors", &cookie, &csrf).await;
    assert_eq!(entry(&list, KILO)["cli"]["probe"]["detail"], "kilo 10.0.0");
}

#[tokio::test]
async fn check_requires_csrf() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, _csrf) = common::bootstrap_and_login_with_state().await;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tools/connectors/kilo-code/check")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn check_unknown_or_mock_404() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    for id in ["mock", "nope"] {
        let (status, _) = send(
            &app,
            "POST",
            &format!("/api/tools/connectors/{id}/check"),
            &cookie,
            &csrf,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
    }
}

#[tokio::test]
async fn connectors_never_return_env_values() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    const SENTINEL: &str = "sentinel-secret-value-7f3a";
    let (state, _app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let probes = Arc::new(ConnectorProbes::with_env_lookup(Arc::new(|key: &str| {
        (key == "ANTHROPIC_API_KEY").then(|| SENTINEL.to_string())
    })));
    let state = Arc::new(AppState {
        connector_probes: probes,
        ..(*state).clone()
    });
    let app = coppice_server::app(state);

    let (status, body) = send(
        &app,
        "POST",
        &format!(
            "/api/tools/connectors/{}/check",
            coppice_connectors::CLAUDE_CODE
        ),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["auth"]["status"], "detected");
    assert_eq!(
        body["auth"]["envSet"],
        serde_json::json!(["ANTHROPIC_API_KEY"])
    );

    let (_, list) = send(&app, "GET", "/api/tools/connectors", &cookie, &csrf).await;
    for text in [body.to_string(), list.to_string()] {
        assert!(text.contains("ANTHROPIC_API_KEY"));
        assert!(!text.contains(SENTINEL), "env value leaked: {text}");
    }
}

#[tokio::test]
async fn last_run_reports_gateway_calls() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_gateway("mcp/ticket_get_submit_result").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;

    let (status, body) = send(
        &app,
        "POST",
        &format!("/api/tickets/{ticket_id}/run-agent"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id: Uuid = body["run"]["id"].as_str().unwrap().parse().unwrap();
    let pool = state.db.clone().unwrap();
    wait_for_run_end(&pool, run_id).await;

    let runs = last_real_runs(&pool).await.expect("last runs");
    let mock = runs.get(coppice_connectors::MOCK).expect("mock last run");
    assert_eq!(mock.run_id, run_id);
    assert_eq!(mock.ticket_id, Some(ticket_id.parse().unwrap()));
    assert_eq!(mock.status, "succeeded");
    assert!(mock.ticket_get);
    assert!(mock.result_submit);
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

async fn wait_for_run_end(pool: &PgPool, run_id: Uuid) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(pool)
            .await
            .unwrap();
        if !matches!(status.as_str(), "queued" | "running") {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for run {run_id}; last={status}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn send_json(
    app: &Router,
    method: &str,
    uri: &str,
    body: &str,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(common::json_request(method, uri, body, cookie, csrf))
        .await
        .unwrap();
    let status = res.status();
    let bytes = http_body_util::BodyExt::collect(res.into_body())
        .await
        .unwrap()
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn admin_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE email = 'admin@localhost'")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn set_agent_connector(pool: &PgPool, agent_id: Uuid, connector: &str) {
    sqlx::query("UPDATE agents SET connector = $2 WHERE id = $1")
        .bind(agent_id)
        .bind(connector)
        .execute(pool)
        .await
        .unwrap();
}

async fn wait_for_check_end(pool: &PgPool, check_id: Uuid) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let status: String =
            sqlx::query_scalar("SELECT status FROM connector_checks WHERE id = $1")
                .bind(check_id)
                .fetch_one(pool)
                .await
                .unwrap();
        if !matches!(status.as_str(), "queued" | "running") {
            return status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for check {check_id}; last={status}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Gateway + workers with `mock_response`, one mock agent, and a check started
/// through the service (the HTTP route hides `mock`).
async fn run_mock_check(
    mock_response: &str,
) -> (
    Arc<AppState>,
    Router,
    String,
    String,
    Uuid,
    Uuid,
    common::AgentTestEnv,
) {
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_gateway(mock_response).await;
    let pool = state.db.clone().unwrap();
    let agent_id: Uuid = common::create_test_agent_from_preset(&app, "Checker", &cookie, &csrf)
        .await
        .parse()
        .unwrap();
    let (check_id, run_id) = ConnectorCheckService::new(&pool)
        .start(
            &state.config,
            coppice_connectors::MOCK,
            agent_id,
            admin_id(&pool).await,
        )
        .await
        .expect("start check");
    wait_for_check_end(&pool, check_id).await;
    (state, app, cookie, csrf, check_id, run_id, env)
}

#[tokio::test]
async fn check_run_passes_with_gateway_calls() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_gateway("mcp/connector_check").await;
    let pool = state.db.clone().unwrap();
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let agent_id: Uuid = common::create_test_agent_from_preset(&app, "Checker", &cookie, &csrf)
        .await
        .parse()
        .unwrap();
    let tickets_before = count(&pool, "SELECT COUNT(*) FROM tickets").await;
    let comments_before = count(&pool, "SELECT COUNT(*) FROM ticket_comments").await;
    let tickets_updated: Vec<String> =
        sqlx::query_scalar("SELECT updated_at::text FROM tickets ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();

    let (check_id, run_id) = ConnectorCheckService::new(&pool)
        .start(
            &state.config,
            coppice_connectors::MOCK,
            agent_id,
            admin_id(&pool).await,
        )
        .await
        .expect("start check");
    assert_eq!(wait_for_check_end(&pool, check_id).await, "passed");

    let (status, body) = send(
        &app,
        "GET",
        &format!("/api/tools/connector-checks/{check_id}"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], check_id.to_string());
    assert_eq!(body["connector"], coppice_connectors::MOCK);
    assert_eq!(body["agentId"], agent_id.to_string());
    assert_eq!(body["status"], "passed");
    assert!(body["failure"].is_null(), "{body}");
    assert_eq!(body["runId"], run_id.to_string());
    assert!(body["createdAt"].is_string());
    assert!(body["finishedAt"].is_string());
    let calls = body["toolCalls"].as_array().expect("toolCalls");
    for tool in ["ticket_get", "result_submit"] {
        assert!(
            calls
                .iter()
                .any(|c| c["tool"] == tool && c["status"] == "ok"),
            "{tool} missing in {body}"
        );
    }

    let run_status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(run_status, "succeeded");
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM tickets").await,
        tickets_before
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM ticket_comments").await,
        comments_before
    );
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM notifications").await, 0);
    let tickets_after: Vec<String> =
        sqlx::query_scalar("SELECT updated_at::text FROM tickets ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(tickets_after, tickets_updated);

    let (_, list) = send(&app, "GET", "/api/tools/connectors", &cookie, &csrf).await;
    assert!(list.as_array().unwrap().iter().all(|e| e["id"] != "mock"));
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn check_run_fails_without_ticket_get() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, _app, _cookie, _csrf, check_id, _run_id, _env) =
        run_mock_check("mcp/connector_check_skip_ticket").await;
    let detail = ConnectorCheckService::new(state.db.as_ref().unwrap())
        .get(check_id)
        .await
        .unwrap();
    assert_eq!(detail.status, "failed");
    assert_eq!(detail.failure.as_deref(), Some("ticket_get was not called"));
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn check_provider_error_marks_failed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, _app, _cookie, _csrf, check_id, run_id, _env) =
        run_mock_check("mcp/connector_check_missing_fixture").await;
    let pool = state.db.clone().unwrap();
    let detail = ConnectorCheckService::new(&pool)
        .get(check_id)
        .await
        .unwrap();
    assert_eq!(detail.status, "failed");
    let failure = detail.failure.expect("failure reason");
    assert!(!failure.trim().is_empty());
    assert!(failure.chars().count() <= 500);
    let token_hashes: Vec<String> =
        sqlx::query_scalar("SELECT token_hash FROM run_tool_tokens WHERE run_id = $1")
            .bind(run_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    for hash in token_hashes {
        assert!(!failure.contains(&hash));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let run_status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        if run_status == "failed" {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "run not failed: {run_status}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn check_failed_run_error_message_is_sanitized() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    const SECRET: &str = "connector_check_missing_fixture";
    let previous = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::set_var("ANTHROPIC_API_KEY", SECRET);
    let (state, _app, _cookie, _csrf, check_id, run_id, _env) =
        run_mock_check("mcp/connector_check_missing_fixture").await;
    let pool = state.db.clone().unwrap();
    wait_for_run_end(&pool, run_id).await;
    match previous {
        Some(value) => std::env::set_var("ANTHROPIC_API_KEY", value),
        None => std::env::remove_var("ANTHROPIC_API_KEY"),
    }
    std::env::remove_var("MOCK_AGENT_RESPONSE");

    let failure = ConnectorCheckService::new(&pool)
        .get(check_id)
        .await
        .unwrap()
        .failure
        .expect("check failure");
    let error_message: String =
        sqlx::query_scalar("SELECT error_message FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    for text in [&failure, &error_message] {
        assert!(!text.contains(SECRET), "secret leaked: {text}");
        assert!(text.contains("[redacted]"), "{text}");
        assert!(text.chars().count() <= 500);
    }
}

#[tokio::test]
async fn check_fails_when_agent_connector_changed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_gateway("mcp/connector_check").await;
    let pool = state.db.clone().unwrap();
    let agent_id: Uuid = common::create_test_agent_from_preset(&app, "Checker", &cookie, &csrf)
        .await
        .parse()
        .unwrap();
    let check_id = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE agents SET connector = $2 WHERE id = $1")
        .bind(agent_id)
        .bind(KILO)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connector_checks (id, connector, agent_id, status, created_by) \
         VALUES ($1, $2, $3, 'queued', $4)",
    )
    .bind(check_id)
    .bind(coppice_connectors::MOCK)
    .bind(agent_id)
    .bind(admin_id(&pool).await)
    .execute(&mut *tx)
    .await
    .unwrap();
    let run = coppice_server::services::run_service::RunService::create_connector_check_run(
        &mut tx, check_id, agent_id,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(wait_for_check_end(&pool, check_id).await, "failed");
    wait_for_run_end(&pool, run.id).await;
    let detail = ConnectorCheckService::new(&pool)
        .get(check_id)
        .await
        .unwrap();
    assert_eq!(
        detail.failure.as_deref(),
        Some("agent connector changed (expected mock, got kilo-code)")
    );
    assert!(detail.tool_calls.is_empty(), "{:?}", detail.tool_calls);
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM agent_runs WHERE id = $1")
            .bind(run.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(
        error_message.as_deref(),
        Some("agent connector changed (expected mock, got kilo-code)")
    );
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn fail_for_run_leaves_finished_check_alone() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, _app, _cookie, _csrf, check_id, run_id, _env) =
        run_mock_check("mcp/connector_check").await;
    let service = ConnectorCheckService::new(state.db.as_ref().unwrap());
    service.fail_for_run(run_id, "late failure").await.unwrap();
    let detail = service.get(check_id).await.unwrap();
    assert_eq!(detail.status, "passed");
    assert!(detail.failure.is_none());
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn check_ticket_get_is_synthetic() {
    use coppice_server::domain::context_profile::ContextProfile;
    use coppice_server::mcp::token::{NewRunToolScope, TokenService};

    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().unwrap();
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let agent_id: Uuid = common::create_test_agent_from_preset(&app, "Checker", &cookie, &csrf)
        .await
        .parse()
        .unwrap();
    let (_check_id, run_id) = ConnectorCheckService::new(&pool)
        .start(
            &state.config,
            coppice_connectors::MOCK,
            agent_id,
            admin_id(&pool).await,
        )
        .await
        .expect("start check");
    let token = TokenService::new(&pool)
        .mint(
            &NewRunToolScope {
                run_id,
                agent_id,
                ticket_id: None,
                chat_session_id: None,
                board_id: None,
                profile: ContextProfile::ConnectorCheck,
                job_type: "connector_check".into(),
                compaction_ticket_ids: vec![],
                plugin_ids: vec![],
            },
            Duration::from_secs(60),
        )
        .await
        .expect("mint token");
    let addr = common::spawn_test_server(app.clone()).await;
    let url = format!("http://{addr}/mcp");
    let rpc = |method: &'static str, params: Value| {
        let url = url.clone();
        let token = token.clone();
        async move {
            reqwest::Client::new()
                .post(&url)
                .bearer_auth(&token)
                .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
                .send()
                .await
                .expect("mcp request")
                .json::<Value>()
                .await
                .expect("mcp json")
        }
    };

    let listed = rpc("tools/list", serde_json::json!({})).await;
    let mut names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, vec!["result_submit", "ticket_get"]);

    for args in [
        serde_json::json!({}),
        serde_json::json!({ "ticketId": ticket_id }),
    ] {
        let res = rpc(
            "tools/call",
            serde_json::json!({ "name": "ticket_get", "arguments": args }),
        )
        .await;
        assert_eq!(res["result"]["isError"], false, "{res}");
        let ticket: Value =
            serde_json::from_str(res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(ticket["title"], "Coppice connection check");
        assert_eq!(
            ticket["description"],
            "Submit a done result with summary 'connection ok'."
        );
    }

    let res = rpc(
        "tools/call",
        serde_json::json!({ "name": "ticket_comments", "arguments": { "ticketId": ticket_id } }),
    )
    .await;
    assert!(
        res["result"]["isError"] == true || res.get("error").is_some(),
        "ticket_comments must be unavailable: {res}"
    );
}

#[tokio::test]
async fn test_route_validation() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.agent.connectors.kilo_code.enabled = true;
    })
    .await;
    let pool = state.db.clone().unwrap();
    let mock_agent = common::create_test_agent_from_preset(&app, "Mocky", &cookie, &csrf).await;
    let codex_agent: Uuid = common::create_test_agent_from_preset(&app, "Codexy", &cookie, &csrf)
        .await
        .parse()
        .unwrap();
    set_agent_connector(&pool, codex_agent, coppice_connectors::CODEX).await;
    let kilo_agent: Uuid = common::create_test_agent_from_preset(&app, "Kiloy", &cookie, &csrf)
        .await
        .parse()
        .unwrap();
    set_agent_connector(&pool, kilo_agent, KILO).await;
    let body = |agent: &dyn std::fmt::Display| format!(r#"{{"agentId":"{agent}"}}"#);
    let test_uri = |id: &str| format!("/api/tools/connectors/{id}/test");

    let (status, _) = send_json(
        &app,
        "POST",
        &test_uri(KILO),
        &body(&mock_agent),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "wrong connector agent");

    let (status, _) = send_json(
        &app,
        "POST",
        &test_uri(coppice_connectors::CODEX),
        &body(&codex_agent),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "disabled connector");

    for id in ["mock", "nope"] {
        let (status, _) = send_json(
            &app,
            "POST",
            &test_uri(id),
            &body(&mock_agent),
            &cookie,
            &csrf,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
    }
    let (status, _) = send_json(
        &app,
        "POST",
        &test_uri(KILO),
        &body(&Uuid::new_v4()),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown agent");

    let (status, started) = send_json(
        &app,
        "POST",
        &test_uri(KILO),
        &body(&kilo_agent),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    let check_id = started["checkId"].as_str().expect("checkId").to_string();
    let run_id = started["runId"].as_str().expect("runId").to_string();
    let (status, _) = send_json(
        &app,
        "POST",
        &test_uri(KILO),
        &body(&kilo_agent),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "active check");

    let (status, detail) = send(
        &app,
        "GET",
        &format!("/api/tools/connector-checks/{check_id}"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["status"], "queued");
    assert_eq!(detail["runId"], run_id);
    assert_eq!(detail["toolCalls"], serde_json::json!([]));
    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/tools/connector-checks/{}", Uuid::new_v4()),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, list) = send(&app, "GET", "/api/tools/connectors", &cookie, &csrf).await;
    let last = &entry(&list, KILO)["lastCheck"];
    assert_eq!(last["id"], check_id);
    assert_eq!(last["status"], "queued");
    assert!(last["failure"].is_null());
    assert!(last["createdAt"].is_string());

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(test_uri(KILO))
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body(&kilo_agent)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "missing CSRF");

    sqlx::query("UPDATE users SET role = 'member' WHERE email = 'admin@localhost'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send_json(
        &app,
        "POST",
        &test_uri(KILO),
        &body(&kilo_agent),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "non-admin test");
    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/tools/connector-checks/{check_id}"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "non-admin detail");
}

#[tokio::test]
async fn stale_checks_failed_on_startup() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().unwrap();
    let agent_id: Uuid = common::create_test_agent_from_preset(&app, "Checker", &cookie, &csrf)
        .await
        .parse()
        .unwrap();
    let service = ConnectorCheckService::new(&pool);
    let (check_id, _run_id) = service
        .start(
            &state.config,
            coppice_connectors::MOCK,
            agent_id,
            admin_id(&pool).await,
        )
        .await
        .expect("start check");
    sqlx::query("UPDATE connector_checks SET status = 'running' WHERE id = $1")
        .bind(check_id)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(service.fail_stale().await.unwrap(), 1);
    let detail = service.get(check_id).await.unwrap();
    assert_eq!(detail.status, "failed");
    assert_eq!(detail.failure.as_deref(), Some("server restarted"));
    assert!(detail.finished_at.is_some());
    assert_eq!(service.fail_stale().await.unwrap(), 0);
}

#[tokio::test]
async fn last_real_run_ignores_check_runs() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, _app, _cookie, _csrf, check_id, _run_id, _env) =
        run_mock_check("mcp/connector_check").await;
    let pool = state.db.clone().unwrap();
    let detail = ConnectorCheckService::new(&pool)
        .get(check_id)
        .await
        .unwrap();
    assert_eq!(detail.status, "passed");
    let runs = last_real_runs(&pool).await.expect("last runs");
    assert!(
        !runs.contains_key(coppice_connectors::MOCK),
        "check run counted as a real run"
    );
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn check_records_non_done_result_and_fails_with_its_name() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, _app, _cookie, _csrf, check_id, _run_id, _env) =
        run_mock_check("mcp/connector_check_blocked").await;
    let detail = ConnectorCheckService::new(state.db.as_ref().unwrap())
        .get(check_id)
        .await
        .unwrap();
    assert_eq!(detail.status, "failed");
    assert_eq!(detail.failure.as_deref(), Some("result was blocked"));
    assert!(
        detail
            .tool_calls
            .iter()
            .any(|c| c.tool == "result_submit" && c.status == "ok"),
        "blocked submission must be recorded: {:?}",
        detail.tool_calls
    );
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn check_first_submitted_outcome_wins() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, _app, _cookie, _csrf, check_id, _run_id, _env) =
        run_mock_check("mcp/connector_check_blocked_then_done").await;
    let detail = ConnectorCheckService::new(state.db.as_ref().unwrap())
        .get(check_id)
        .await
        .unwrap();
    assert_eq!(detail.status, "failed");
    assert_eq!(detail.failure.as_deref(), Some("result was blocked"));
    let submits: Vec<&str> = detail
        .tool_calls
        .iter()
        .filter(|c| c.tool == "result_submit")
        .map(|c| c.status.as_str())
        .collect();
    assert_eq!(submits, vec!["ok", "denied"]);
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}
