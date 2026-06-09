mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn list_presets_has_ten_entries() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/agent-presets",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 10);
    let first = &body["items"][0];
    let template = first["systemPromptTemplate"].as_str().unwrap();
    assert!(
        template.contains("# SOUL"),
        "expected SOUL template, got: {template}"
    );
}

#[tokio::test]
async fn create_agent_from_preset() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let presets_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/agent-presets",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(presets_res.status(), StatusCode::OK);

    let presets: serde_json::Value = common::json_body(presets_res).await;
    let preset = &presets["items"][0];
    let preset_id = preset["id"].as_str().unwrap();
    let preset_role = preset["role"].as_str().unwrap();

    let create_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            &format!(r#"{{"name":"PM Bot","presetId":"{preset_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);

    let agent: serde_json::Value = common::json_body(create_res).await;
    assert_eq!(agent["role"].as_str().unwrap(), preset_role);
    assert_eq!(agent["presetSource"].as_str().unwrap(), preset["key"].as_str().unwrap());
    assert_eq!(agent["name"].as_str().unwrap(), "PM Bot");
    let template = preset["systemPromptTemplate"].as_str().unwrap();
    assert_eq!(agent["systemPrompt"].as_str().unwrap(), template);
}

#[tokio::test]
async fn agent_with_unknown_provider_gets_missing_config_health() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let create_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            r#"{"name":"OpenCode Bot","role":"Developer","systemPrompt":"You are a developer","connector":"opencode"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);

    coppice_server::workers::health_worker::run_health_pass_once(&state).await;

    let list_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/agents",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(list_res.status(), StatusCode::OK);

    let body: serde_json::Value = common::json_body(list_res).await;
    let agents = body["items"].as_array().unwrap();
    assert_eq!(agents.len(), 1);
    let agent = &agents[0];
    assert_eq!(agent["connector"].as_str().unwrap(), "opencode");
    assert_eq!(agent["health"].as_str().unwrap(), "missing_config");
    assert!(agent["healthDetail"]
        .as_str()
        .unwrap()
        .contains("not configured"));
}

#[tokio::test]
async fn list_connectors_returns_mock() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/connectors",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let ids: Vec<_> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"mock"));
}
