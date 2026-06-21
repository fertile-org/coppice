mod common;

use axum::http::{header, StatusCode};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use std::time::{Duration, Instant};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        http::HeaderValue,
        Error as WsError,
        Message,
    },
};
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

async fn connect_ws(url: &str, cookie: Option<&str>) -> tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
> {
    let mut request = url.into_client_request().unwrap();
    if let Some(cookie) = cookie {
        request.headers_mut().insert(
            header::COOKIE,
            HeaderValue::from_str(cookie).unwrap(),
        );
    }
    connect_async(request).await.expect("ws connect").0
}

#[tokio::test]
async fn live_ws_receives_mock_frames_and_terminal_log_written() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let addr = common::spawn_test_server(app.clone()).await;

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _, _) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = body.as_ref().unwrap()["run"]["id"].as_str().unwrap();

    let ws_url = format!("ws://{addr}/ws/agent-runs/{run_id}/live");

    // The stream handle is registered before the run is marked running, so a
    // single connect attaches deterministically (live frames while active, or
    // artifact replay once terminal). No reconnect loop required.
    let ws = connect_ws(&ws_url, Some(&cookie)).await;
    let (_, mut read) = ws.split();

    let mut saw_mock_frame = false;
    let mut saw_end = false;
    let deadline = Instant::now() + Duration::from_secs(15);

    while Instant::now() < deadline && !saw_end {
        let msg = tokio::time::timeout(Duration::from_millis(500), read.next()).await;
        match msg {
            Ok(Some(Ok(Message::Text(text)))) => {
                let json: serde_json::Value = serde_json::from_str(&text).unwrap();
                if json["type"] == "frame" {
                    if json["data"]
                        .as_str()
                        .unwrap_or("")
                        .contains("Mock agent")
                    {
                        saw_mock_frame = true;
                    }
                } else if json["type"] == "end" {
                    if json["status"] == "succeeded" {
                        saw_end = true;
                    }
                    break;
                }
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(e))) => panic!("ws error: {e}"),
            Ok(None) | Err(_) => break,
        }
    }

    assert!(saw_mock_frame, "expected frame with Mock agent");
    assert!(saw_end, "expected end message");

    poll_run_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "succeeded",
        Duration::from_secs(10),
    )
    .await;

    let terminal_log = std::path::PathBuf::from("/tmp/coppice-test-artifacts")
        .join("runs")
        .join(run_id)
        .join("terminal.log");
    assert!(terminal_log.exists(), "terminal.log should exist");
    let content = std::fs::read_to_string(&terminal_log).unwrap();
    assert!(content.contains("Mock agent"));
}

#[tokio::test]
async fn ws_rejects_unauthenticated() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, _cookie, _csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let addr = common::spawn_test_server(app).await;

    let ws_url = format!("ws://{addr}/ws/events");
    match connect_async(ws_url).await {
        Err(WsError::Http(resp)) => assert_eq!(resp.status(), 401),
        Err(e) => panic!("expected Http 401, got {e}"),
        Ok(_) => panic!("expected connection rejection"),
    }
}

#[tokio::test]
async fn events_ws_receives_run_finished() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let addr = common::spawn_test_server(app.clone()).await;

    let ws_url = format!("ws://{addr}/ws/events");
    let ws = connect_ws(&ws_url, Some(&cookie)).await;
    let (_, mut read) = ws.split();

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _, _) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = body.as_ref().unwrap()["run"]["id"].as_str().unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_finished = false;

    while Instant::now() < deadline {
        let msg = tokio::time::timeout(Duration::from_secs(2), read.next()).await;
        match msg {
            Ok(Some(Ok(Message::Text(text)))) => {
                let json: serde_json::Value = serde_json::from_str(&text).unwrap();
                if json["type"] == "agent_run.finished" {
                    assert_eq!(json["run_id"].as_str().unwrap(), run_id);
                    saw_finished = true;
                    break;
                }
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(e))) => panic!("ws error: {e}"),
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    assert!(saw_finished, "expected agent_run.finished event");
}

/// Insert a run directly into the DB in the given status, tied to an existing
/// ticket/agent. Used to deterministically model "a run that already
/// transitioned to running before the client connected" without racing the
/// worker (which finishes mock runs in milliseconds).
async fn insert_run_row(
    ticket_id: &str,
    agent_id: &str,
    status: &str,
) -> String {
    use coppice_server::db;
    let run_id = uuid::Uuid::new_v4().to_string();
    let pool = db::shared_test_pool()
        .await
        .expect("shared test pool");
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(&run_id)
    .bind(ticket_id)
    .bind(agent_id)
    .bind("work_on_ticket")
    .bind(status)
    .bind("permissive-default")
    .execute(&pool)
    .await
    .expect("insert active run");
    run_id
}

#[tokio::test]
async fn events_ws_late_subscriber_learns_running_status() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let addr = common::spawn_test_server(app.clone()).await;

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, agent_id, _) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    // A run already transitioned to `running` before the client connects — no
    // agent_run.started broadcast is pending. The snapshot-on-connect path must
    // surface current truth.
    let run_id = insert_run_row(&ticket_id, &agent_id, "running").await;

    let ws_url = format!("ws://{addr}/ws/events");
    let ws = connect_ws(&ws_url, Some(&cookie)).await;
    let (_, mut read) = ws.split();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_started = false;

    while Instant::now() < deadline {
        let msg = tokio::time::timeout(Duration::from_secs(2), read.next()).await;
        match msg {
            Ok(Some(Ok(Message::Text(text)))) => {
                let json: serde_json::Value = serde_json::from_str(&text).unwrap();
                if json["type"] == "agent_run.started" {
                    assert_eq!(json["run_id"].as_str().unwrap(), run_id);
                    assert_eq!(json["status"].as_str().unwrap(), "running");
                    saw_started = true;
                    break;
                }
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(e))) => panic!("ws error: {e}"),
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    assert!(
        saw_started,
        "late subscriber must learn the run is running without polling"
    );
}
