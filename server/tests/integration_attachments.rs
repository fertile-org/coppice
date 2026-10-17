mod common;

use axum::http::StatusCode;
use tower::ServiceExt;
use uuid::Uuid;

async fn upload(app: &axum::Router, name: &str, contents: &str, cookie: &str, csrf: &str) -> Uuid {
    let res = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            name,
            "text/plain",
            contents,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "upload {name}");
    let body = common::json_body(res).await;
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

async fn set_ticket_attachments(app: &axum::Router, ticket_id: &str, ids: &[Uuid], cookie: &str, csrf: &str) {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            &serde_json::json!({ "attachmentIds": ids }).to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

async fn row_exists(pool: &sqlx::PgPool, id: Uuid) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachments WHERE id = $1)")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn attachments_are_removed_once_nothing_references_them() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let artifacts = tempfile::tempdir().unwrap();
    let artifacts_path = artifacts.path().to_path_buf();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.storage.artifacts_dir = artifacts_path.to_string_lossy().into_owned();
    })
    .await;
    let pool = state.db.clone().unwrap();
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;

    // Uploads up to the 10 MiB cap are accepted, not cut off at axum's 2 MB default.
    let big = upload(&app, "big.txt", &"x".repeat(3 * 1024 * 1024), &cookie, &csrf).await;
    let kept = upload(&app, "kept.txt", "keep me", &cookie, &csrf).await;
    let dropped = upload(&app, "dropped.txt", "drop me", &cookie, &csrf).await;
    set_ticket_attachments(&app, &ticket_id, &[big, kept, dropped], &cookie, &csrf).await;

    // Removing an attachment from the ticket deletes it right away.
    set_ticket_attachments(&app, &ticket_id, &[big, kept], &cookie, &csrf).await;
    assert!(!row_exists(&pool, dropped).await);
    assert!(!artifacts.path().join(dropped.to_string()).exists());
    assert!(row_exists(&pool, kept).await);
    assert!(artifacts.path().join(kept.to_string()).exists());

    // A cancelled upload is swept after the grace period; a fresh one is not.
    let abandoned = upload(&app, "abandoned.txt", "never attached", &cookie, &csrf).await;
    let fresh = upload(&app, "fresh.txt", "just uploaded", &cookie, &csrf).await;
    sqlx::query("UPDATE attachments SET created_at = now() - interval '2 days' WHERE id = ANY($1)")
        .bind(vec![abandoned, kept])
        .execute(&pool)
        .await
        .unwrap();
    let removed = coppice_server::services::attachment_cleanup::sweep(&pool, &state.attachments)
        .await
        .unwrap();
    assert_eq!(removed, 1);
    assert!(!row_exists(&pool, abandoned).await);
    assert!(!artifacts.path().join(abandoned.to_string()).exists());
    assert!(row_exists(&pool, fresh).await);
    assert!(row_exists(&pool, kept).await, "a referenced attachment stays");
}

#[tokio::test]
async fn an_upload_over_the_cap_is_refused() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.storage.max_upload_bytes = 1024;
    })
    .await;
    let res = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "big.txt",
            "text/plain",
            &"x".repeat(4096),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
