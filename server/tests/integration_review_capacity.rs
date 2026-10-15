mod common;

use axum::http::StatusCode;
use coppice_server::copy::review::{
    ready_held, review_entry_held, waiting_on_review, REVIEW_CAP_HINT, REVIEW_CAP_INVALID,
    REVIEW_CAP_LABEL,
};
use coppice_server::services::ticket_service::TicketService;
use tower::ServiceExt;

async fn message(response: axum::response::Response) -> String {
    let body: serde_json::Value = common::json_body(response).await;
    body["message"].as_str().unwrap_or_default().to_string()
}

async fn comments(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> Vec<serde_json::Value> {
    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/comments"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(response).await;
    body.as_array().cloned().unwrap_or_default()
}

fn put_capacity(cap: Option<i32>, count_in_review: bool) -> String {
    serde_json::json!({
        "reviewCap": cap,
        "countInReview": count_in_review,
    })
    .to_string()
}

#[tokio::test]
async fn full_cap_keeps_ready_tickets_queued_until_review_clears() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    common::create_agent_with_preset_key(&app, "tech_lead", "Technical Lead", &cookie, &csrf).await;

    let saved = app
        .clone()
        .oneshot(common::json_request(
            "PUT",
            &format!("/api/boards/{board_id}/review-capacity"),
            &put_capacity(Some(1), false),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let filler = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let filler_review = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{filler}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(filler_review.status(), StatusCode::OK);

    let board = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{board_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(board.status(), StatusCode::OK);
    let board_body: serde_json::Value = common::json_body(board).await;
    assert_eq!(board_body["waitingOnReview"], waiting_on_review(1));
    assert_eq!(board_body["reviewCapLabel"], REVIEW_CAP_LABEL);
    assert_eq!(board_body["reviewCap"], 1);

    let queued = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    // The description is a plan-shaped prompt. The ready check must ignore it.
    let described = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{queued}"),
            r#"{"description":"Send a prompt and follow the plan: do everything.","skipPlanning":true}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(described.status(), StatusCode::OK);
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{queued}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let queued_body: serde_json::Value = common::json_body(ready).await;
    assert_eq!(queued_body["status"], "ready");
    assert!(queued_body["assigneeAgentId"].is_null());
    let notes = comments(&app, &queued, &cookie, &csrf).await;
    assert!(notes.iter().any(|note| note["body"] == ready_held(1)));

    let dragged = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{queued}/status"),
            r#"{"status":"in_progress"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(dragged.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(dragged).await, ready_held(1));

    let approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{filler}/final-approve"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);

    let started = common::get_ticket(&app, &queued, &cookie, &csrf).await;
    assert_eq!(started["status"], "ready");
    assert!(started["assigneeAgentId"].is_string());
}

#[tokio::test]
async fn in_review_counts_when_the_board_asks() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let saved = app
        .clone()
        .oneshot(common::json_request(
            "PUT",
            &format!("/api/boards/{board_id}/review-capacity"),
            &put_capacity(Some(1), true),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let filler = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let in_review = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{filler}/status"),
            r#"{"status":"in_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(in_review.status(), StatusCode::OK);

    let blocked = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let denied = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{blocked}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(denied).await, review_entry_held(1));

    let board = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{board_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let board_body: serde_json::Value = common::json_body(board).await;
    assert_eq!(board_body["waitingOnReview"], waiting_on_review(1));
    assert_eq!(board_body["countInReview"], true);
}

#[tokio::test]
async fn merge_conflict_reopen_bypasses_a_full_review_cap() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;

    let reopened = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let to_review = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{reopened}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(to_review.status(), StatusCode::OK);
    let approved = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{reopened}/final-approve"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);

    let reopened_id = uuid::Uuid::parse_str(&reopened).unwrap();
    TicketService::new(pool)
        .reopen_after_merge_conflict(reopened_id)
        .await
        .expect("merge conflict reopen");
    let exempt: bool = sqlx::query_scalar("SELECT review_cap_exempt FROM tickets WHERE id = $1")
        .bind(reopened_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(exempt, "a merge-conflict reopen is exempt from the cap");

    let saved = app
        .clone()
        .oneshot(common::json_request(
            "PUT",
            &format!("/api/boards/{board_id}/review-capacity"),
            &put_capacity(Some(1), false),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let filler = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let filler_review = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{filler}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(filler_review.status(), StatusCode::OK);

    let ordinary = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let denied = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ordinary}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(denied).await, review_entry_held(1));

    let allowed = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{reopened}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(allowed).await;
    assert_eq!(body["status"], "wait_for_final_review");
    let still_exempt: bool =
        sqlx::query_scalar("SELECT review_cap_exempt FROM tickets WHERE id = $1")
            .bind(reopened_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(
        !still_exempt,
        "the exemption is spent once the ticket is back in review"
    );
}

#[tokio::test]
async fn review_cap_defaults_to_empty_on_new_and_existing_boards() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");

    let column_default: Option<String> = sqlx::query_scalar(
        r#"
        SELECT column_default
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'boards'
          AND column_name = 'review_cap'
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(
        column_default.is_none(),
        "the migration must not default review_cap to a number"
    );

    let existing_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
        .bind(existing_id)
        .bind("Already here")
        .bind(format!("already-here-{existing_id}"))
        .execute(pool)
        .await
        .unwrap();
    let existing_cap: Option<i32> =
        sqlx::query_scalar("SELECT review_cap FROM boards WHERE id = $1")
            .bind(existing_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(existing_cap.is_none(), "an existing board has no cap");

    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/boards",
            r#"{"name":"Fresh board"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body: serde_json::Value = common::json_body(created).await;
    assert!(created_body["reviewCap"].is_null());
    assert_eq!(created_body["reviewCapHint"], REVIEW_CAP_HINT);
    assert_eq!(created_body["countInReview"], false);

    let existing = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{existing_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(existing.status(), StatusCode::OK);
    let existing_body: serde_json::Value = common::json_body(existing).await;
    assert!(existing_body["reviewCap"].is_null());
}

#[tokio::test]
async fn zero_and_negative_caps_are_rejected_with_the_copy() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    for cap in [0, -1] {
        let response = app
            .clone()
            .oneshot(common::json_request(
                "PUT",
                &format!("/api/boards/{board_id}/review-capacity"),
                &put_capacity(Some(cap), false),
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(message(response).await, REVIEW_CAP_INVALID);
    }

    let cleared = app
        .clone()
        .oneshot(common::json_request(
            "PUT",
            &format!("/api/boards/{board_id}/review-capacity"),
            &put_capacity(None, false),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(cleared.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(cleared).await;
    assert!(body["reviewCap"].is_null());
}
