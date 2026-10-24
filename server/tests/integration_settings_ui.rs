mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

async fn get_ui(app: &axum::Router, cookie: &str, csrf: &str) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/settings/config/ui",
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    common::json_body(response).await
}

async fn put_value(
    app: &axum::Router,
    body: serde_json::Value,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(common::json_request(
            "PUT",
            "/api/settings/config/value",
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    let status = response.status();
    (status, common::json_body(response).await)
}

fn setting<'a>(ui: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    ui["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|setting| setting["key"] == key)
        .unwrap_or_else(|| panic!("no setting {key}"))
}

#[tokio::test]
async fn settings_ui_lists_defaults_and_writes_only_what_changes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "# mine\n[git]\nauthor_name = \"Me\"\n").unwrap();
    state.connectors.set_config_path(Some(path.clone()));

    let ui = get_ui(&app, &cookie, &csrf).await;
    assert!(ui["fileError"].is_null());
    let retention = setting(&ui, "logs.retention_days");
    assert_eq!(retention["default"], 14);
    assert!(retention["value"].is_null(), "not in the file: {retention}");
    assert_eq!(retention["kind"]["type"], "integerChoice");
    assert_eq!(setting(&ui, "git.author_name")["value"], "Me");
    let enabled = setting(&ui, "agent.connectors.opencode.enabled");
    assert_eq!(enabled["defaultLabel"], "Automatic");
    assert_eq!(enabled["kind"]["unsetLabel"], "Automatic");
    assert_eq!(
        setting(&ui, "keybindings.palette.open")["default"][0],
        "Mod+K"
    );

    let (status, saved) = put_value(
        &app,
        serde_json::json!({
            "path": ["logs", "retention_days"],
            "value": 7,
            "baseRevision": ui["revision"],
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        "# mine\n[git]\nauthor_name = \"Me\"\n\n[logs]\nretention_days = 7\n"
    );
    let ui = get_ui(&app, &cookie, &csrf).await;
    assert_eq!(setting(&ui, "logs.retention_days")["value"], 7);
    assert_eq!(ui["revision"], saved["revision"]);

    let (status, _) = put_value(
        &app,
        serde_json::json!({
            "path": ["git", "author_name"],
            "value": null,
            "baseRevision": ui["revision"],
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!std::fs::read_to_string(&path)
        .unwrap()
        .contains("author_name"));

    let (status, body) = put_value(
        &app,
        serde_json::json!({
            "path": ["logs", "retention_days"],
            "value": 5,
            "baseRevision": get_ui(&app, &cookie, &csrf).await["revision"],
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["message"],
        "Set logs.retention_days to 3, 7, 14, or 30."
    );

    let (status, body) = put_value(
        &app,
        serde_json::json!({
            "path": ["git", "push_enabled"],
            "value": true,
            "baseRevision": "stale",
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["text"]
        .as_str()
        .unwrap()
        .contains("retention_days = 7"));
}

#[tokio::test]
async fn settings_ui_reports_a_file_it_cannot_read() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[git\n").unwrap();
    state.connectors.set_config_path(Some(path));

    let ui = get_ui(&app, &cookie, &csrf).await;
    assert!(ui["fileError"].is_string());
    assert!(setting(&ui, "git.author_name")["value"].is_null());
}
