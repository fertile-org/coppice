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
        (
            "POST",
            "/api/plugins/install".to_string(),
            json!({ "gitUrl": "https://github.com/a/b.git", "pluginDirId": uuid::Uuid::new_v4() }),
        ),
        (
            "POST",
            format!("/api/plugins/{plugin_id}/update"),
            Value::Null,
        ),
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

async fn sample_plugin_id(app: &Router, dir_id: &str, cookie: &str, csrf: &str) -> String {
    let plugins = get(app, "/api/plugins", cookie, csrf).await;
    find(&plugins, dir_id, "sample-plugin")["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn set_enabled(app: &Router, plugin_id: &str, enabled: bool, cookie: &str, csrf: &str) {
    let (status, body) = send(
        app,
        "PATCH",
        &format!("/api/plugins/{plugin_id}"),
        json!({ "enabled": enabled }),
        cookie,
        csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "set enabled: {body}");
}

async fn rescan(app: &Router, cookie: &str, csrf: &str) {
    let (status, body) = send(app, "POST", "/api/plugins/rescan", Value::Null, cookie, csrf).await;
    assert_eq!(status, StatusCode::OK, "rescan: {body}");
}

async fn run_plugin_ids(state: &coppice_server::AppState, agent_id: &str) -> Vec<uuid::Uuid> {
    let pool = state.db.as_ref().unwrap();
    coppice_server::services::plugin_service::PluginService::new(pool)
        .run_plugin_ids(agent_id.parse().unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn set_agent_plugins_accepts_only_enabled_ok_plugins() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = plugin_dir_with(&["sample-plugin"]);
    let dir_id = add_dir(&app, dir.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plugin_id = sample_plugin_id(&app, &dir_id, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let uri = format!("/api/agents/{agent_id}/plugins");

    assert_eq!(
        get(&app, &uri, &cookie, &csrf).await,
        json!({ "pluginIds": [] })
    );
    let (status, body) = send(
        &app,
        "PUT",
        &uri,
        json!({ "pluginIds": [plugin_id] }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"].is_string());

    set_enabled(&app, &plugin_id, true, &cookie, &csrf).await;
    let (status, body) = send(
        &app,
        "PUT",
        &uri,
        json!({ "pluginIds": [plugin_id] }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "pluginIds": [plugin_id] }));
    assert_eq!(
        get(&app, &uri, &cookie, &csrf).await,
        json!({ "pluginIds": [plugin_id] })
    );

    let missing = format!("/api/agents/{}/plugins", uuid::Uuid::new_v4());
    let (status, _) = send(&app, "GET", &missing, Value::Null, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &app,
        "PUT",
        &missing,
        json!({ "pluginIds": [] }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn missing_plugin_keeps_assignment_and_returns() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = plugin_dir_with(&["sample-plugin"]);
    let dir_id = add_dir(&app, dir.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plugin_id = sample_plugin_id(&app, &dir_id, &cookie, &csrf).await;
    set_enabled(&app, &plugin_id, true, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let uri = format!("/api/agents/{agent_id}/plugins");
    let (status, _) = send(
        &app,
        "PUT",
        &uri,
        json!({ "pluginIds": [plugin_id] }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let expected: uuid::Uuid = plugin_id.parse().unwrap();
    assert_eq!(run_plugin_ids(&state, &agent_id).await, vec![expected]);

    std::fs::remove_dir_all(dir.path().join("sample-plugin")).unwrap();
    rescan(&app, &cookie, &csrf).await;
    assert!(run_plugin_ids(&state, &agent_id).await.is_empty());
    assert!(state.skills.get(&[expected], "sample-plugin:hello").is_none());
    assert_eq!(
        get(&app, &uri, &cookie, &csrf).await,
        json!({ "pluginIds": [plugin_id] })
    );

    copy_tree(
        &fixtures().join("sample-plugin"),
        &dir.path().join("sample-plugin"),
    );
    rescan(&app, &cookie, &csrf).await;
    assert_eq!(run_plugin_ids(&state, &agent_id).await, vec![expected]);
    assert!(state.skills.get(&[expected], "sample-plugin:hello").is_some());
}

#[tokio::test]
async fn preset_default_plugins_applied_when_enabled() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().unwrap();
    let presets = get(&app, "/api/agent-presets", &cookie, &csrf).await;
    let preset_id: uuid::Uuid = presets["items"][0]["id"].as_str().unwrap().parse().unwrap();
    sqlx::query("UPDATE agent_presets SET default_plugins = '{sample-plugin}' WHERE id = $1")
        .bind(preset_id)
        .execute(&pool)
        .await
        .unwrap();

    let dir = plugin_dir_with(&["sample-plugin"]);
    let dir_id = add_dir(&app, dir.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plugin_id = sample_plugin_id(&app, &dir_id, &cookie, &csrf).await;
    set_enabled(&app, &plugin_id, true, &cookie, &csrf).await;
    let with_default = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let assigned = get(
        &app,
        &format!("/api/agents/{with_default}/plugins"),
        &cookie,
        &csrf,
    )
    .await;

    set_enabled(&app, &plugin_id, false, &cookie, &csrf).await;
    let without = common::create_test_agent_from_preset(&app, "Other", &cookie, &csrf).await;
    let unassigned = get(
        &app,
        &format!("/api/agents/{without}/plugins"),
        &cookie,
        &csrf,
    )
    .await;

    sqlx::query("UPDATE agent_presets SET default_plugins = '{}' WHERE id = $1")
        .bind(preset_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(assigned, json!({ "pluginIds": [plugin_id] }));
    assert_eq!(unassigned, json!({ "pluginIds": [] }));
}

#[tokio::test]
async fn disabling_assigned_plugin_stops_serving_its_skills() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = plugin_dir_with(&["sample-plugin"]);
    let dir_id = add_dir(&app, dir.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plugin_id = sample_plugin_id(&app, &dir_id, &cookie, &csrf).await;
    set_enabled(&app, &plugin_id, true, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let (status, _) = send(
        &app,
        "PUT",
        &format!("/api/agents/{agent_id}/plugins"),
        json!({ "pluginIds": [plugin_id] }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id: uuid::Uuid = plugin_id.parse().unwrap();
    let served = |state: &coppice_server::AppState| {
        state
            .skills
            .skills_for(&[id])
            .iter()
            .any(|s| s.id == "sample-plugin:hello")
    };
    assert!(served(&state));

    set_enabled(&app, &plugin_id, false, &cookie, &csrf).await;
    assert!(!served(&state));
    assert!(state.skills.get(&[id], "sample-plugin:hello").is_none());
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Coppice Test",
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

/// A working copy of `sample-plugin` pushed to a bare repo named `sample-plugin.git`.
struct PluginRepo {
    _tmp: tempfile::TempDir,
    work: std::path::PathBuf,
    bare: std::path::PathBuf,
}

impl PluginRepo {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        copy_tree(&fixtures().join("sample-plugin"), &work);
        git(&work, &["init", "-q"]);
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "-q", "-m", "initial"]);
        let bare = tmp.path().join("sample-plugin.git");
        git(
            tmp.path(),
            &["clone", "-q", "--bare", "work", "sample-plugin.git"],
        );
        git(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
        Self {
            _tmp: tmp,
            work,
            bare,
        }
    }

    fn url(&self) -> String {
        format!("file://{}", self.bare.display())
    }

    fn head(&self) -> String {
        git(&self.bare, &["rev-parse", "HEAD"])
    }

    fn push_new_skill(&self, name: &str) {
        let skill = self.work.join(format!("skills/{name}/SKILL.md"));
        std::fs::create_dir_all(skill.parent().unwrap()).unwrap();
        std::fs::write(
            &skill,
            format!("---\nname: {name}\ndescription: Says {name}\n---\nSay {name}.\n"),
        )
        .unwrap();
        git(&self.work, &["add", "-A"]);
        git(&self.work, &["commit", "-q", "-m", name]);
        git(
            &self.work,
            &["push", "-q", "origin", "HEAD:refs/heads/main"],
        );
    }
}

async fn install_app() -> (
    std::sync::Arc<coppice_server::AppState>,
    Router,
    String,
    String,
) {
    common::bootstrap_and_login_with_state_config(|config| {
        config.plugins.allow_file_git_urls = true;
    })
    .await
}

async fn default_dir(app: &Router, cookie: &str, csrf: &str) -> (String, std::path::PathBuf) {
    let dirs = get(app, "/api/plugin-dirs", cookie, csrf).await;
    let dir = dirs
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["isDefault"] == true)
        .unwrap();
    (
        dir["id"].as_str().unwrap().to_string(),
        std::path::PathBuf::from(dir["path"].as_str().unwrap()),
    )
}

async fn post_install(
    app: &Router,
    git_url: &str,
    dir_id: &str,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, Value) {
    send(
        app,
        "POST",
        "/api/plugins/install",
        json!({ "gitUrl": git_url, "pluginDirId": dir_id }),
        cookie,
        csrf,
    )
    .await
}

async fn wait_install(app: &Router, install_id: &str, cookie: &str, csrf: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let install = get(
            app,
            &format!("/api/plugin-installs/{install_id}"),
            cookie,
            csrf,
        )
        .await;
        if install["status"] != "running" {
            return install;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "install still running: {install}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn install_sample(
    app: &Router,
    repo: &PluginRepo,
    dir_id: &str,
    cookie: &str,
    csrf: &str,
) -> Value {
    let (status, install) = post_install(app, &repo.url(), dir_id, cookie, csrf).await;
    assert_eq!(status, StatusCode::ACCEPTED, "install: {install}");
    assert_eq!(install["status"], "running");
    assert_eq!(install["kind"], "install");
    let done = wait_install(app, install["id"].as_str().unwrap(), cookie, csrf).await;
    assert_eq!(done["status"], "succeeded", "install: {done}");
    done
}

#[tokio::test]
async fn install_from_git_clones_scans_and_records_commit() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = install_app().await;
    let repo = PluginRepo::new();
    let (dir_id, dir_path) = default_dir(&app, &cookie, &csrf).await;

    let done = install_sample(&app, &repo, &dir_id, &cookie, &csrf).await;
    assert!(done["error"].is_null());
    let plugin_id = done["pluginId"].as_str().unwrap();

    let plugin = get(&app, &format!("/api/plugins/{plugin_id}"), &cookie, &csrf).await;
    assert_eq!(plugin["name"], "sample-plugin");
    assert_eq!(plugin["relPath"], "sample-plugin");
    assert_eq!(plugin["pluginDirId"], dir_id.as_str());
    assert_eq!(plugin["status"], "ok");
    assert_eq!(plugin["source"], "git");
    assert_eq!(plugin["gitUrl"], repo.url());
    assert_eq!(plugin["gitCommit"], repo.head());
    assert_eq!(plugin["enabled"], false);
    assert!(dir_path
        .join("sample-plugin/.claude-plugin/plugin.json")
        .is_file());
}

#[tokio::test]
async fn install_into_existing_dest_is_conflict() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = install_app().await;
    let repo = PluginRepo::new();
    let (dir_id, _) = default_dir(&app, &cookie, &csrf).await;
    install_sample(&app, &repo, &dir_id, &cookie, &csrf).await;

    let (status, body) = post_install(&app, &repo.url(), &dir_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn install_rejects_invalid_url_and_file_urls_when_disallowed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let (dir_id, _) = default_dir(&app, &cookie, &csrf).await;
    for url in ["--upload-pack=touch x", "file:///tmp/x.git"] {
        let (status, body) = post_install(&app, url, &dir_id, &cookie, &csrf).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}: {body}");
    }
    let (status, body) = send(
        &app,
        "POST",
        "/api/plugins/install",
        json!({ "gitUrl": "https://github.com/a/b.git", "ref": "--output=x", "pluginDirId": dir_id }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn install_failure_is_reported() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = install_app().await;
    let (dir_id, dir_path) = default_dir(&app, &cookie, &csrf).await;

    let (status, install) = post_install(
        &app,
        "file:///nonexistent/repo.git",
        &dir_id,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{install}");
    let done = wait_install(&app, install["id"].as_str().unwrap(), &cookie, &csrf).await;
    assert_eq!(done["status"], "failed");
    assert!(!done["error"].as_str().unwrap().is_empty(), "{done}");
    assert!(done["pluginId"].is_null());

    let plugins = get(&app, "/api/plugins", &cookie, &csrf).await;
    assert!(plugins_in_dir(&plugins, &dir_id).is_empty());
    assert!(!dir_path.join("repo").exists());
}

#[tokio::test]
async fn update_pulls_new_commit() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = install_app().await;
    let repo = PluginRepo::new();
    let (dir_id, _) = default_dir(&app, &cookie, &csrf).await;
    let installed = install_sample(&app, &repo, &dir_id, &cookie, &csrf).await;
    let plugin_id = installed["pluginId"].as_str().unwrap().to_string();
    let first_commit = repo.head();
    set_enabled(&app, &plugin_id, true, &cookie, &csrf).await;
    let id: uuid::Uuid = plugin_id.parse().unwrap();
    assert!(state.skills.get(&[id], "sample-plugin:goodbye").is_none());

    repo.push_new_skill("goodbye");
    let (status, update) = send(
        &app,
        "POST",
        &format!("/api/plugins/{plugin_id}/update"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{update}");
    assert_eq!(update["kind"], "update");
    let done = wait_install(&app, update["id"].as_str().unwrap(), &cookie, &csrf).await;
    assert_eq!(done["status"], "succeeded", "{done}");
    assert_eq!(done["pluginId"], plugin_id.as_str());

    let plugin = get(&app, &format!("/api/plugins/{plugin_id}"), &cookie, &csrf).await;
    assert_ne!(plugin["gitCommit"], first_commit.as_str());
    assert_eq!(plugin["gitCommit"], repo.head());
    assert_eq!(plugin["enabled"], true);
    assert!(plugin["skills"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "goodbye"));
    assert!(state.skills.get(&[id], "sample-plugin:goodbye").is_some());
}

#[tokio::test]
async fn update_of_local_plugin_is_rejected() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = plugin_dir_with(&["sample-plugin"]);
    let dir_id = add_dir(&app, dir.path(), &cookie, &csrf).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let plugin_id = sample_plugin_id(&app, &dir_id, &cookie, &csrf).await;
    let (status, body) = send(
        &app,
        "POST",
        &format!("/api/plugins/{plugin_id}/update"),
        Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn stale_running_install_failed_on_startup() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().unwrap();
    let (dir_id, _) = default_dir(&app, &cookie, &csrf).await;
    let install_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO plugin_installs (id, plugin_dir_id, kind, git_url, status) VALUES ($1, $2, 'install', 'https://h/a/b.git', 'running')",
    )
    .bind(install_id)
    .bind(dir_id.parse::<uuid::Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let failed = coppice_server::services::plugin_service::PluginService::new(&pool)
        .fail_stale_installs()
        .await
        .unwrap();
    assert_eq!(failed, 1);
    let install = get(
        &app,
        &format!("/api/plugin-installs/{install_id}"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(install["status"], "failed");
    assert_eq!(install["error"], "server restarted");
}

#[tokio::test]
async fn install_of_non_plugin_repo_fails_and_removes_clone() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();
    let (_state, app, cookie, csrf) = install_app().await;
    let (dir_id, dir_path) = default_dir(&app, &cookie, &csrf).await;
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("README.md"), "not a plugin\n").unwrap();
    git(&work, &["init", "-q"]);
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "initial"]);
    git(tmp.path(), &["clone", "-q", "--bare", "work", "plain.git"]);
    let url = format!("file://{}", tmp.path().join("plain.git").display());

    let (status, install) = post_install(&app, &url, &dir_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{install}");
    let done = wait_install(&app, install["id"].as_str().unwrap(), &cookie, &csrf).await;
    assert_eq!(done["status"], "failed");
    assert_eq!(done["error"], "cloned repository is not a plugin");
    assert!(!dir_path.join("plain").exists());
}
