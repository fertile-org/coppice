mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::services::plugin_build_service::PluginBuildService;
use serde_json::{json, Value};
use std::path::Path;
use tower::ServiceExt;

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
