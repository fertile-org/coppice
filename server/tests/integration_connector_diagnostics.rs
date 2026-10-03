mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
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
