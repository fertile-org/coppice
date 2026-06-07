mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn upload_attachment_and_create_comment() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;

    let upload = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "notes.txt",
            "text/plain",
            "hello attachment",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::CREATED);

    let attachment: serde_json::Value = common::json_body(upload).await;
    let attachment_id = attachment["id"].as_str().unwrap();
    assert_eq!(attachment["filename"], "notes.txt");
    assert_eq!(attachment["contentType"], "text/plain");
    assert_eq!(attachment["sizeBytes"], 16);

    let create_comment = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            &format!(
                r#"{{"body":"See attached","attachmentIds":["{attachment_id}"]}}"#
            ),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_comment.status(), StatusCode::CREATED);

    let list = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/comments"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);

    let comments: serde_json::Value = common::json_body(list).await;
    assert_eq!(comments.as_array().unwrap().len(), 1);
    assert_eq!(comments[0]["body"], "See attached");
    assert_eq!(comments[0]["authorType"], "human");
    assert_eq!(
        comments[0]["attachmentIds"][0].as_str().unwrap(),
        attachment_id
    );
}
