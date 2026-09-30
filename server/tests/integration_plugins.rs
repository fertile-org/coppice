mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use coppice_server::middleware::session::parse_session_cookie;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::path::Path;
use tower::ServiceExt;

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/plugins")
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Temp dir holding copies of the named fixture plugins as children.
fn plugin_dir_with(fixture_names: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in fixture_names {
        copy_tree(&fixtures().join(name), &dir.path().join(name));
    }
    dir
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, Value) {
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

async fn get(app: &Router, uri: &str, cookie: &str, csrf: &str) -> Value {
    let (status, body) = send(app, "GET", uri, Value::Null, cookie, csrf).await;
    assert_eq!(status, StatusCode::OK, "GET {uri}: {body}");
    body
}

async fn add_dir(app: &Router, path: &Path, cookie: &str, csrf: &str) -> Value {
    let (status, body) = send(
        app,
        "POST",
        "/api/plugin-dirs",
        json!({ "path": path.to_string_lossy() }),
        cookie,
        csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "add dir: {body}");
    body
}

fn plugins_in_dir<'a>(plugins: &'a Value, dir_id: &str) -> Vec<&'a Value> {
    plugins
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["pluginDirId"] == dir_id)
        .collect()
}

fn find<'a>(plugins: &'a Value, dir_id: &str, name: &str) -> &'a Value {
    plugins_in_dir(plugins, dir_id)
        .into_iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("plugin {name} in dir {dir_id} not found: {plugins}"))
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

macro_rules! require_db {
    () => {
        if !common::db_available().await {
            eprintln!("skipping: postgres not available");
            return;
        }
    };
}

#[tokio::test]
async fn default_dir_exists_after_bootstrap_and_cannot_be_removed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let dirs = get(&app, "/api/plugin-dirs", &cookie, &csrf).await;
    let dirs = dirs.as_array().unwrap();
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0]["isDefault"], true);
    assert_eq!(dirs[0]["position"], 0);
    assert_eq!(
        dirs[0]["path"],
        canonical(Path::new(&state.config.plugins.dir))
    );

    let id = dirs[0]["id"].as_str().unwrap();
    let (status, body) = send(
        &app,
        "DELETE",
        &format!("/api/plugin-dirs/{id}"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn add_dir_scans_plugins_disabled_by_default() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = plugin_dir_with(&["sample-plugin", "skills-only", "superpowers-like"]);

    let added = add_dir(&app, dir.path(), &cookie, &csrf).await;
    assert_eq!(added["path"], canonical(dir.path()));
    assert_eq!(added["position"], 1);
    assert_eq!(added["isDefault"], false);
    let dir_id = added["id"].as_str().unwrap();

    let plugins = get(&app, "/api/plugins", &cookie, &csrf).await;
    let rows = plugins_in_dir(&plugins, dir_id);
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|p| p["enabled"] == false));

    let sample = find(&plugins, dir_id, "sample-plugin");
    assert_eq!(sample["status"], "ok");
    assert_eq!(sample["version"], "1.2.0");
    assert_eq!(sample["source"], "local");
    assert_eq!(sample["relPath"], "sample-plugin");
    assert_eq!(sample["unsupported"], json!(["commands", "hooks"]));
    let skills = sample["skills"].as_array().unwrap();
    let hello = skills.iter().find(|s| s["name"] == "hello").unwrap();
    assert!(hello["error"].is_null());
    let broken = skills
        .iter()
        .find(|s| s["relPath"].as_str().unwrap().contains("broken"))
        .unwrap();
    assert!(broken["error"].is_string());
    assert!(sample["mcpServers"].is_array());

    let id = sample["id"].as_str().unwrap();
    let single = get(&app, &format!("/api/plugins/{id}"), &cookie, &csrf).await;
    assert_eq!(single["name"], "sample-plugin");
}

#[tokio::test]
async fn add_dir_rejects_relative_missing_and_duplicate() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/plugin-dirs",
        json!({ "path": "relative/x" }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string());

    let (status, _) = send(
        &app,
        "POST",
        "/api/plugin-dirs",
        json!({ "path": format!("/nonexistent-{}", uuid::Uuid::new_v4()) }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let dir = tempfile::tempdir().unwrap();
    add_dir(&app, dir.path(), &cookie, &csrf).await;
    let (status, _) = send(
        &app,
        "POST",
        "/api/plugin-dirs",
        json!({ "path": dir.path().to_string_lossy() }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn invalid_manifest_is_listed_with_error() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("broken/.claude-plugin/plugin.json");
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(&manifest, "{").unwrap();

    let added = add_dir(&app, dir.path(), &cookie, &csrf).await;
    let dir_id = added["id"].as_str().unwrap();
    let plugins = get(&app, "/api/plugins", &cookie, &csrf).await;
    let broken = find(&plugins, dir_id, "broken");
    assert_eq!(broken["status"], "invalid");
    assert!(broken["error"].as_str().unwrap().contains("plugin.json"));
    assert_eq!(broken["enabled"], false);

    let id = broken["id"].as_str().unwrap();
    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugins/{id}"),
        json!({ "enabled": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn rescan_marks_deleted_plugin_missing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = plugin_dir_with(&["sample-plugin", "skills-only"]);
    let added = add_dir(&app, dir.path(), &cookie, &csrf).await;
    let dir_id = added["id"].as_str().unwrap();

    std::fs::remove_dir_all(dir.path().join("skills-only")).unwrap();
    let (status, plugins) = send(
        &app,
        "POST",
        "/api/plugins/rescan",
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(find(&plugins, dir_id, "skills-only")["status"], "missing");
    assert_eq!(find(&plugins, dir_id, "sample-plugin")["status"], "ok");
}

#[tokio::test]
async fn same_name_in_two_dirs_second_is_shadowed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let a = plugin_dir_with(&["sample-plugin"]);
    let b = plugin_dir_with(&["sample-plugin"]);
    let a_id = add_dir(&app, a.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let b_id = add_dir(&app, b.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let plugins = get(&app, "/api/plugins", &cookie, &csrf).await;
    assert_eq!(find(&plugins, &a_id, "sample-plugin")["status"], "ok");
    let shadowed = find(&plugins, &b_id, "sample-plugin");
    assert_eq!(shadowed["status"], "shadowed");

    let id = shadowed["id"].as_str().unwrap();
    let (status, _) = send(
        &app,
        "PATCH",
        &format!("/api/plugins/{id}"),
        json!({ "enabled": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn reorder_then_rescan_flips_winner_and_unserves_loser() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let a = plugin_dir_with(&["sample-plugin"]);
    let b = plugin_dir_with(&["sample-plugin"]);
    let a_id = add_dir(&app, a.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let b_id = add_dir(&app, b.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let plugins = get(&app, "/api/plugins", &cookie, &csrf).await;
    let a_plugin = find(&plugins, &a_id, "sample-plugin")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, enabled) = send(
        &app,
        "PATCH",
        &format!("/api/plugins/{a_plugin}"),
        json!({ "enabled": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(enabled["enabled"], true);

    let (status, dirs) = send(
        &app,
        "PATCH",
        &format!("/api/plugin-dirs/{b_id}"),
        json!({ "position": 0 }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let order: Vec<(&str, i64)> = dirs
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["id"].as_str().unwrap(), d["position"].as_i64().unwrap()))
        .collect();
    assert_eq!(order[0], (b_id.as_str(), 0));
    assert_eq!(order[2], (a_id.as_str(), 2));

    let plugins = get(&app, "/api/plugins", &cookie, &csrf).await;
    assert_eq!(find(&plugins, &b_id, "sample-plugin")["status"], "ok");
    let loser = find(&plugins, &a_id, "sample-plugin");
    assert_eq!(loser["status"], "shadowed");
    assert_eq!(loser["enabled"], true);
}

#[tokio::test]
async fn plugin_mutations_require_admin_and_csrf() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = plugin_dir_with(&["sample-plugin"]);
    add_dir(&app, dir.path(), &cookie, &csrf).await;
    let plugins = get(&app, "/api/plugins", &cookie, &csrf).await;
    let plugin_id = plugins[0]["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &app,
        "POST",
        "/api/users",
        json!({ "email": "member@localhost", "password": "secret123" }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (member_cookie, member_csrf) = login_as(&app, "member@localhost", "secret123").await;

    get(&app, "/api/plugins", &member_cookie, &member_csrf).await;
    let other = tempfile::tempdir().unwrap();
    let attempts = [
        (
            "POST",
            "/api/plugin-dirs".to_string(),
            json!({ "path": other.path().to_string_lossy() }),
        ),
        (
            "PATCH",
            format!("/api/plugins/{plugin_id}"),
            json!({ "enabled": true }),
        ),
        ("POST", "/api/plugins/rescan".to_string(), Value::Null),
    ];
    for (method, uri, body) in &attempts {
        let (status, _) = send(
            &app,
            method,
            uri,
            body.clone(),
            &member_cookie,
            &member_csrf,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri} as member");
    }

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plugins/rescan")
                .header("content-type", "application/json")
                .header(header::COOKIE, &cookie)
                .body(Body::from("null"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
