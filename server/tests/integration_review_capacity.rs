mod common;

use axum::http::StatusCode;
use coppice_server::copy::review::{
    self, ready_held, review_entry_held, waiting_on_review, DIFF_BUDGET_INVALID, REVIEW_CAP_INVALID,
};
use coppice_server::services::review_capacity::ReviewCapacityService;
use coppice_server::services::ticket_service::TicketService;
use sqlx::Row;
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

fn put_capacity(cap: Option<i32>, count_in_review: bool, budget: Option<i32>) -> String {
    serde_json::json!({
        "reviewCap": cap,
        "countInReview": count_in_review,
        "diffBudget": budget,
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
            &put_capacity(Some(1), false, None),
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
    assert_eq!(board_body["reviewCapLabel"], review::REVIEW_CAP_LABEL);
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
            &put_capacity(Some(1), true, None),
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
            &put_capacity(Some(1), false, None),
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
async fn plan_approval_stores_the_diff_budget_without_reading_the_plan() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let saved = app
        .clone()
        .oneshot(common::json_request(
            "PUT",
            &format!("/api/boards/{board_id}/review-capacity"),
            &put_capacity(None, false, Some(1)),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let ticket_uuid = uuid::Uuid::parse_str(&ticket_id).unwrap();
    sqlx::query(
        r#"
        UPDATE tickets SET status = 'plan_review' WHERE id = $1
        "#,
    )
    .bind(ticket_uuid)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO ticket_comments (
            id, ticket_id, author_type, body, intent, plan_content_version
        )
        VALUES ($1, $2, 'system', $3, 'plan', 1)
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(ticket_uuid)
    .bind("## Plan\n\nSend a prompt. Budget in this plan text is 999 lines.")
    .execute(pool)
    .await
    .unwrap();

    let approved = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/approve-plan"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);

    let row =
        sqlx::query("SELECT agreed_diff_budget, diff_budget_agreed FROM tickets WHERE id = $1")
            .bind(ticket_uuid)
            .fetch_one(pool)
            .await
            .unwrap();
    let agreed: Option<i32> = row.get("agreed_diff_budget");
    let frozen: bool = row.get("diff_budget_agreed");
    assert_eq!(agreed, Some(1));
    assert!(frozen);

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init"]);
    git(repo, &["config", "user.email", "test@localhost"]);
    git(repo, &["config", "user.name", "test"]);
    std::fs::write(repo.join("notes.txt"), "one\n").unwrap();
    git(repo, &["add", "notes.txt"]);
    git(repo, &["commit", "-m", "start"]);
    let sha = git_stdout(repo, &["rev-parse", "HEAD"]);
    std::fs::write(repo.join("notes.txt"), "one\ntwo\nthree\nfour\n").unwrap();

    let agent_id = uuid::Uuid::parse_str(
        &common::create_agent_with_preset_key(&app, "backend_engineer", "Backend", &cookie, &csrf)
            .await,
    )
    .unwrap();
    let run_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id, start_sha)
        VALUES ($1, $2, $3, 'work_on_ticket', 'succeeded', 'permissive', $4)
        "#,
    )
    .bind(run_id)
    .bind(ticket_uuid)
    .bind(agent_id)
    .bind(sha.trim())
    .execute(pool)
    .await
    .unwrap();

    let over = ReviewCapacityService::new(pool)
        .diff_budget_message(ticket_uuid, run_id, Some(repo.to_str().unwrap()))
        .await
        .unwrap();
    assert_eq!(
        over.as_deref(),
        Some(review::diff_over_budget(3, 1).as_str())
    );
}

#[tokio::test]
async fn negative_limits_are_rejected_with_the_copy() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let cap = app
        .clone()
        .oneshot(common::json_request(
            "PUT",
            &format!("/api/boards/{board_id}/review-capacity"),
            &put_capacity(Some(-1), false, None),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(cap.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(cap).await, REVIEW_CAP_INVALID);

    let budget = app
        .clone()
        .oneshot(common::json_request(
            "PUT",
            &format!("/api/boards/{board_id}/review-capacity"),
            &put_capacity(None, false, Some(-2)),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(budget.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(budget).await, DIFF_BUDGET_INVALID);
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(dir: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
