mod common;

use axum::{http::StatusCode, Router};
use tower::ServiceExt;

async fn send(
    app: &Router,
    method: &str,
    body: serde_json::Value,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(common::json_request(
            method,
            "/api/settings/knowledge",
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    let status = response.status();
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

async fn get_settings(app: &Router, cookie: &str, csrf: &str) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/settings/knowledge",
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    common::json_body(response).await
}

#[tokio::test]
async fn compaction_agent_defaults_to_off_and_round_trips() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let initial = get_settings(&app, &cookie, &csrf).await;
    assert!(initial["compactionAgentId"].is_null());
    assert!(initial["compactionAgent"].is_null());
    assert!(initial["readOnlyConnectors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|connector| connector == "mock"));

    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Compactor", &cookie, &csrf)
            .await;
    let (status, saved) = send(
        &app,
        "PUT",
        serde_json::json!({ "compactionAgentId": agent_id }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["compactionAgentId"], agent_id);
    assert_eq!(saved["compactionAgent"]["name"], "Compactor");
    assert_eq!(saved["compactionAgent"]["enabled"], true);
    assert_eq!(saved["compactionAgent"]["connector"], "mock");
    assert_eq!(
        get_settings(&app, &cookie, &csrf).await["compactionAgentId"],
        agent_id
    );

    let (status, cleared) = send(
        &app,
        "PUT",
        serde_json::json!({ "compactionAgentId": null }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(cleared["compactionAgentId"].is_null());
    assert!(get_settings(&app, &cookie, &csrf).await["compactionAgentId"].is_null());
}

#[tokio::test]
async fn compaction_agent_rejects_unknown_and_non_read_only_agents() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let (status, _) = send(
        &app,
        "PUT",
        serde_json::json!({ "compactionAgentId": uuid::Uuid::new_v4() }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Writer", &cookie, &csrf)
            .await;
    sqlx::query("UPDATE agents SET connector = 'codex' WHERE id = $1::uuid")
        .bind(&agent_id)
        .execute(state.db.as_ref().unwrap())
        .await
        .unwrap();
    let (status, body) = send(
        &app,
        "PUT",
        serde_json::json!({ "compactionAgentId": agent_id }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].as_str().unwrap().contains("read-only"));
    assert!(get_settings(&app, &cookie, &csrf).await["compactionAgentId"].is_null());
}

#[tokio::test]
async fn compaction_agent_requires_admin_and_csrf() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Compactor", &cookie, &csrf)
            .await;

    let (status, _) = send(
        &app,
        "PUT",
        serde_json::json!({ "compactionAgentId": agent_id }),
        &cookie,
        "wrong-csrf",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    sqlx::query("UPDATE users SET role = 'member' WHERE email = 'admin@localhost'")
        .execute(state.db.as_ref().unwrap())
        .await
        .unwrap();
    let (status, _) = send(
        &app,
        "PUT",
        serde_json::json!({ "compactionAgentId": agent_id }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(get_settings(&app, &cookie, &csrf).await["compactionAgentId"].is_null());
}

#[tokio::test]
async fn deleting_the_compaction_agent_turns_compaction_off() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Compactor", &cookie, &csrf)
            .await;
    let (status, _) = send(
        &app,
        "PUT",
        serde_json::json!({ "compactionAgentId": agent_id }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let response = app
        .clone()
        .oneshot(common::json_request(
            "DELETE",
            &format!("/api/agents/{agent_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", response.status());

    let settings = get_settings(&app, &cookie, &csrf).await;
    assert!(settings["compactionAgentId"].is_null());
    assert!(settings["compactionAgent"].is_null());
}
