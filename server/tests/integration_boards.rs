mod common;

use axum::http::StatusCode;
use coppice_server::copy::review::{
    BOARD_NAME_CONFLICT, BOARD_NAME_INVALID, BOARD_NAME_LABEL,
};
use tower::ServiceExt;

#[tokio::test]
async fn create_board_and_repo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let create_board = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/boards",
            r#"{"name":"Coppice Demo"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_board.status(), StatusCode::CREATED);

    let body: serde_json::Value = common::json_body(create_board).await;
    let _board_id = body["id"].as_str().unwrap();

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let create_repo = app
        .oneshot(common::json_request(
            "POST",
            "/api/repos",
            &format!(
                r#"{{"name":"main-repo","localPath":"{}","defaultBranch":"main"}}"#,
                local_path.display()
            ),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_repo.status(), StatusCode::CREATED);
}

async fn create_named_board(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    name: &str,
) -> (String, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/boards",
            &serde_json::json!({ "name": name }).to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value = common::json_body(response).await;
    let id = body["id"].as_str().unwrap().to_string();
    (id, body)
}

async fn patch_board_name(
    app: &axum::Router,
    board_id: &str,
    name: &str,
    cookie: &str,
    csrf: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/boards/{board_id}"),
            &serde_json::json!({ "name": name }).to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap()
}

async fn get_board(
    app: &axum::Router,
    board_id: &str,
    cookie: &str,
    csrf: &str,
) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{board_id}"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    common::json_body(response).await
}

async fn message(response: axum::response::Response) -> String {
    let body: serde_json::Value = common::json_body(response).await;
    body["message"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn patch_rejects_blank_or_symbols_only_name() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let (board_id, created) = create_named_board(&app, &cookie, &csrf, "Keep Me").await;
    let original_name = created["name"].as_str().unwrap().to_string();
    let original_slug = created["slug"].as_str().unwrap().to_string();

    for name in ["", "   ", "!!!"] {
        let response = patch_board_name(&app, &board_id, name, &cookie, &csrf).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(message(response).await, BOARD_NAME_INVALID);
        let current = get_board(&app, &board_id, &cookie, &csrf).await;
        assert_eq!(current["name"], original_name);
        assert_eq!(current["slug"], original_slug);
    }
}

#[tokio::test]
async fn patch_rejects_slug_collision_with_another_board() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let (taken_id, taken) = create_named_board(&app, &cookie, &csrf, "FooBar").await;
    let (board_id, created) = create_named_board(&app, &cookie, &csrf, "Keep Me").await;
    let original_name = created["name"].as_str().unwrap().to_string();
    let original_slug = created["slug"].as_str().unwrap().to_string();
    assert_eq!(taken["slug"], "foobar");
    assert_eq!(created["slug"], "keep-me");

    // slugify drops hyphens, so "foo-bar" collides with "FooBar".
    for name in ["FooBar", "foo-bar"] {
        let response = patch_board_name(&app, &board_id, name, &cookie, &csrf).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(message(response).await, BOARD_NAME_CONFLICT);
        let current = get_board(&app, &board_id, &cookie, &csrf).await;
        assert_eq!(current["name"], original_name);
        assert_eq!(current["slug"], original_slug);
    }

    let taken_now = get_board(&app, &taken_id, &cookie, &csrf).await;
    assert_eq!(taken_now["name"], taken["name"]);
    assert_eq!(taken_now["slug"], taken["slug"]);
}

#[tokio::test]
async fn patch_accepts_own_name_and_case_variant() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let (board_id, _) = create_named_board(&app, &cookie, &csrf, "Test Board").await;

    let same = patch_board_name(&app, &board_id, "Test Board", &cookie, &csrf).await;
    assert_eq!(same.status(), StatusCode::OK);
    let same_body: serde_json::Value = common::json_body(same).await;
    assert_eq!(same_body["name"], "Test Board");
    assert_eq!(same_body["slug"], "test-board");
    assert_eq!(same_body["nameLabel"], BOARD_NAME_LABEL);

    let variant = patch_board_name(&app, &board_id, "test board", &cookie, &csrf).await;
    assert_eq!(variant.status(), StatusCode::OK);
    let variant_body: serde_json::Value = common::json_body(variant).await;
    assert_eq!(variant_body["name"], "test board");
    assert_eq!(variant_body["slug"], "test-board");
}
