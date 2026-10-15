mod common;

use axum::http::{header, StatusCode};
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
};
use tower::ServiceExt;

async fn login_as(app: &axum::Router, email: &str, password: &str) -> (String, String) {
    let login = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(format!(
                    r#"{{"email":"{email}","password":"{password}"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let set_cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .expect("session cookie");
    let cookie_header = set_cookie.to_str().unwrap();
    let session_token =
        coppice_server::middleware::session::parse_session_cookie(cookie_header).expect("token");
    let cookie = format!("coppice_session={session_token}");
    let body = http_body_util::BodyExt::collect(login.into_body())
        .await
        .unwrap()
        .to_bytes();
    let login_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let csrf = login_json["csrfToken"].as_str().unwrap().to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn admin_reads_logs_and_a_member_cannot() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let logs_dir = dir.path().to_path_buf();
    std::fs::write(
        logs_dir.join("coppice.2026-10-14.jsonl"),
        concat!(
            r#"{"timestamp":"2026-10-14T03:35:01Z","level":"INFO","target":"coppice_server::middleware::request_log","fields":{"message":"http request","source":"http","method":"GET","path":"/api/health","status":200,"duration_ms":4}}"#,
            "\n",
            r#"{"timestamp":"2026-10-14T03:35:02Z","level":"DEBUG","target":"t","fields":{"message":"hidden debug","source":"server"}}"#,
            "\n",
        ),
    )
    .expect("write logs");

    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.logs.dir = logs_dir.to_string_lossy().into_owned();
    })
    .await;

    let res = app
        .clone()
        .oneshot(common::json_request("GET", "/api/logs", "", &cookie, &csrf))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = common::json_body(res).await;
    let lines = body["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), 1, "{body}");
    assert_eq!(lines[0]["source"], "http");
    assert_eq!(lines[0]["message"], "http request");
    assert_eq!(lines[0]["fields"]["path"], "/api/health");
    assert!(lines[0]["runId"].is_null());

    let missing = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/logs")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let created = app
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
    assert_eq!(created.status(), StatusCode::CREATED);
    let (member_cookie, member_csrf) = login_as(&app, "member@localhost", "secret123").await;
    let forbidden = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/logs",
            "",
            &member_cookie,
            &member_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let bad = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/logs?source=nope",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    let meta = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/logs/meta",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(meta.status(), StatusCode::OK);
    let meta = common::json_body(meta).await;
    assert_eq!(meta["retentionDays"], 14);
    assert!(meta["version"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    assert!(meta["commit"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    assert!(meta["os"].as_str().is_some_and(|value| !value.is_empty()));
    assert!(meta["arch"].as_str().is_some_and(|value| !value.is_empty()));
    assert!(meta["osVersion"].is_string());
}

#[tokio::test]
async fn live_tail_sends_a_line_written_after_connect() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let logs_dir = dir.path().to_path_buf();
    let day = logs_dir.join("coppice.2026-10-14.jsonl");
    std::fs::write(
        &day,
        "{\"timestamp\":\"2026-10-14T01:00:00Z\",\"level\":\"INFO\",\"target\":\"t\",\"fields\":{\"message\":\"already there\",\"source\":\"server\"}}\n",
    )
    .expect("write");
    std::os::unix::fs::symlink("coppice.2026-10-14.jsonl", logs_dir.join("current.jsonl"))
        .expect("symlink");

    let (_state, app, cookie, _csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.logs.dir = logs_dir.to_string_lossy().into_owned();
    })
    .await;
    let addr = common::spawn_test_server(app).await;
    let mut request = format!("ws://{addr}/ws/logs")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
    let (mut socket, _) = connect_async(request).await.expect("ws connect");
    socket
        .send(Message::Text(r#"{"level":"info"}"#.into()))
        .await
        .expect("send filter");

    tokio::time::sleep(Duration::from_millis(300)).await;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&day)
        .expect("append");
    writeln!(
        file,
        r#"{{"timestamp":"2026-10-14T02:00:00Z","level":"INFO","target":"t","fields":{{"message":"arrived later","source":"cli","stream":"stdout","run_id":"run-9"}}}}"#
    )
    .expect("append line");
    drop(file);

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut saw = false;
    while Instant::now() < deadline {
        let frame = tokio::time::timeout(Duration::from_millis(700), socket.next()).await;
        let Ok(Some(Ok(Message::Text(text)))) = frame else {
            continue;
        };
        let value: serde_json::Value = serde_json::from_str(&text).expect("json frame");
        assert_ne!(value["message"], "already there");
        if value["message"] == "arrived later" {
            assert_eq!(value["source"], "cli");
            assert_eq!(value["runId"], "run-9");
            saw = true;
            break;
        }
    }
    assert!(saw, "live tail did not send the appended line");
}
