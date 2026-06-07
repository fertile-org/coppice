mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn create_ticket_and_update_status() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/projects/{project_id}/tickets"),
            r#"{"title":"First ticket","description":"hello"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let ticket: serde_json::Value = common::json_body(res).await;
    let ticket_id = ticket["id"].as_str().unwrap();
    assert_eq!(ticket["status"], "backlog");

    let patch = app
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
}

#[tokio::test]
async fn reject_invalid_status() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/projects/{project_id}/tickets"),
            r#"{"title":"Ticket","description":""}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let ticket: serde_json::Value = common::json_body(res).await;
    let ticket_id = ticket["id"].as_str().unwrap();

    let patch = app
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"not_a_column"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reject_done_with_waiting_substatus() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/projects/{project_id}/tickets"),
            r#"{"title":"Ticket","description":""}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let ticket: serde_json::Value = common::json_body(res).await;
    let ticket_id = ticket["id"].as_str().unwrap();

    let set_substatus = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"in_progress","substatus":"waiting_for_human"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(set_substatus.status(), StatusCode::OK);

    let patch = app
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"done"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::BAD_REQUEST);
}
