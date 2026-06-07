mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn create_project_and_repo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let create_project = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/projects",
            r#"{"name":"Coppice Demo"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_project.status(), StatusCode::CREATED);

    let body: serde_json::Value = common::json_body(create_project).await;
    let project_id = body["id"].as_str().unwrap();

    let create_repo = app
        .oneshot(common::json_request(
            "POST",
            &format!("/api/projects/{project_id}/repos"),
            r#"{"name":"main-repo","defaultBranch":"main"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_repo.status(), StatusCode::CREATED);
}
