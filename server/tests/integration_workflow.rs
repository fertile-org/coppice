mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

async fn create_agent_with_preset_key(
    app: &axum::Router,
    preset_key: &str,
    name: &str,
    cookie: &str,
    csrf: &str,
) -> String {
    let presets_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/agent-presets",
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(presets_res.status(), StatusCode::OK);

    let presets: serde_json::Value = common::json_body(presets_res).await;
    let preset_id = presets["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"].as_str() == Some(preset_key))
        .and_then(|item| item["id"].as_str())
        .unwrap_or_else(|| panic!("preset {preset_key} not found"));

    let create_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            &format!(r#"{{"name":"{name}","presetId":"{preset_id}"}}"#),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);

    let agent: serde_json::Value = common::json_body(create_res).await;
    agent["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn human_mention_does_not_change_ticket_status() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let _agent_id = create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;

    let before = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(before.status(), StatusCode::OK);
    let before_body: serde_json::Value = common::json_body(before).await;
    assert_eq!(before_body["status"], "backlog");

    let comment_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@pm please review the approach"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(comment_res.status(), StatusCode::CREATED);

    let after = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::OK);
    let after_body: serde_json::Value = common::json_body(after).await;
    assert_eq!(after_body["status"], "backlog");
    assert!(after_body["substatus"].is_null());

    let mention_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ticket_mentions WHERE ticket_id = $1 AND status = 'pending'",
    )
    .bind(uuid::Uuid::parse_str(&ticket_id).unwrap())
    .fetch_one(pool)
    .await
    .expect("count mentions");
    assert_eq!(mention_count, 1);
}

#[tokio::test]
async fn final_approve_requires_wait_for_final_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;

    let reject = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/final-approve"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(reject.status(), StatusCode::BAD_REQUEST);

    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/final-approve"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);

    let approved: serde_json::Value = common::json_body(approve).await;
    assert_eq!(approved["status"], "done");
    assert!(approved["substatus"].is_null());
}
