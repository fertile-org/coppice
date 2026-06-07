mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn full_workspace_happy_path() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let _repo_id = common::create_test_repo(&app, &project_id, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "PM Bot", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;

    let assign = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/assign"),
            &format!(r#"{{"agentId":"{agent_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(assign.status(), StatusCode::OK);

    let comment = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"Agent, please take this ticket."}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(comment.status(), StatusCode::CREATED);

    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            &format!(
                r#"{{"status":"in_progress","substatus":"waiting_for_agent","substatusMetadata":{{"agentId":"{agent_id}"}}}}"#
            ),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let get = app
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
    assert_eq!(get.status(), StatusCode::OK);

    let ticket: serde_json::Value = common::json_body(get).await;
    assert_eq!(ticket["substatus"], "waiting_for_agent");
    assert_eq!(ticket["assigneeAgentId"].as_str().unwrap(), agent_id);
    assert_eq!(
        ticket["substatusDisplay"]["detail"].as_str().unwrap(),
        "PM Bot"
    );
}

#[tokio::test]
async fn projects_without_session_returns_401() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, _cookie, _csrf) = common::bootstrap_and_login().await;

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/projects")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
