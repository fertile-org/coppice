mod common;

use axum::http::StatusCode;
use coppice_server::copy::plan::{
    NO_PLANNER, PLAN_ALREADY_RUNNING, PLAN_APPROVED_NOTE, PLAN_REQUIRED, PLAN_STALE,
};
use coppice_server::services::planning_service::PlanningService;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

async fn message(res: axum::response::Response) -> String {
    let body: serde_json::Value = common::json_body(res).await;
    body["message"].as_str().unwrap_or_default().to_string()
}

async fn comments(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> Vec<serde_json::Value> {
    let res = app
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
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body.as_array().cloned().unwrap_or_default()
}

async fn runs(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> Vec<serde_json::Value> {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/runs"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body["runs"].as_array().cloned().unwrap_or_default()
}

fn plan_runs(runs: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    runs.iter()
        .filter(|run| run["jobType"].as_str() == Some("plan_ticket"))
        .collect()
}

#[tokio::test]
async fn drag_and_api_refuse_in_progress_without_an_approved_plan() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;

    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"in_progress"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(patch).await, PLAN_REQUIRED);

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "backlog");
}

#[tokio::test]
async fn skip_planning_allows_in_progress_and_does_not_start_a_plan() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    common::skip_planning(&app, &ticket_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    assert!(plan_runs(&runs(&app, &ticket_id, &cookie, &csrf).await).is_empty());

    let progress = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"in_progress"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(progress.status(), StatusCode::OK);
    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "in_progress");
}

#[tokio::test]
async fn ready_starts_one_plan_run_for_the_pm() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let engineer_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Backend", &cookie, &csrf)
            .await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &engineer_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let again = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::OK);

    let listed = runs(&app, &ticket_id, &cookie, &csrf).await;
    let plans = plan_runs(&listed);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0]["agentId"], pm_id);
}

#[tokio::test]
async fn ready_without_a_planner_stays_ready_and_says_why() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "ready");
    let notes = comments(&app, &ticket_id, &cookie, &csrf).await;
    assert!(notes.iter().any(|note| note["body"] == NO_PLANNER));
    assert!(plan_runs(&runs(&app, &ticket_id, &cookie, &csrf).await).is_empty());
}

#[tokio::test]
async fn ready_uses_the_assignee_when_there_is_no_pm() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let engineer_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Backend", &cookie, &csrf)
            .await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &engineer_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let listed = runs(&app, &ticket_id, &cookie, &csrf).await;
    let plans = plan_runs(&listed);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0]["agentId"], engineer_id);
}

const CANONICAL_PLAN: &str = "## Plan\n\n### Approach\nShip the parser first.\n\n### Steps\n- [ ] Add the column\n- [ ] Gate In Progress\n\n### Risks\nNone.\n\n### Out of scope\nNone.\n";

async fn post_plan(
    app: &axum::Router,
    ticket_id: &str,
    agent_id: Uuid,
    pool: &sqlx::PgPool,
    cookie: &str,
    csrf: &str,
) {
    PlanningService::new(pool)
        .record_plan(Uuid::parse_str(ticket_id).unwrap(), agent_id, CANONICAL_PLAN)
        .await
        .expect("record plan");
    let moved = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"plan_review"}"#,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(moved.status(), StatusCode::OK);
}

#[tokio::test]
async fn ask_for_changes_revises_the_plan_and_refuses_a_second_request() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let pool = state.db.as_ref().expect("db");
    post_plan(
        &app,
        &ticket_id,
        Uuid::parse_str(&pm_id).unwrap(),
        pool,
        &cookie,
        &csrf,
    )
    .await;

    let ask = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/plan-changes"),
            r#"{"comment":"Drop the migration."}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ask.status(), StatusCode::OK);
    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "plan_review");
    let thread = comments(&app, &ticket_id, &cookie, &csrf).await;
    assert!(thread.iter().any(|note| note["body"] == "Drop the migration."));
    assert_eq!(plan_runs(&runs(&app, &ticket_id, &cookie, &csrf).await).len(), 1);

    let again = app
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/plan-changes"),
            r#"{"comment":"Again."}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(again).await, PLAN_ALREADY_RUNNING);
}

#[tokio::test]
async fn approve_posts_the_note_and_the_review_checklist() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let pool = state.db.as_ref().expect("db");
    let pm_uuid = Uuid::parse_str(&pm_id).unwrap();
    post_plan(&app, &ticket_id, pm_uuid, pool, &cookie, &csrf).await;

    let edited = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            r#"{"title":"Renamed after the plan"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(edited.status(), StatusCode::OK);
    let stale = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/approve-plan"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(stale).await, PLAN_STALE);

    post_plan(&app, &ticket_id, pm_uuid, pool, &cookie, &csrf).await;
    let approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/approve-plan"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);
    let approved = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(approved["status"], "in_progress");
    let notes = comments(&app, &ticket_id, &cookie, &csrf).await;
    assert!(notes.iter().any(|note| note["body"] == PLAN_APPROVED_NOTE));

    let review = app
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
    assert_eq!(review.status(), StatusCode::OK);
    let shown = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(
        shown["approvedPlan"]["steps"],
        serde_json::json!(["Add the column", "Gate In Progress"])
    );
}

#[tokio::test]
async fn a_planning_run_posts_the_plan_and_moves_to_plan_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let ticket = common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "plan review",
        Duration::from_secs(15),
        |ticket| ticket["status"] == "plan_review",
    )
    .await;
    assert_eq!(ticket["status"], "plan_review");
    let thread = comments(&app, &ticket_id, &cookie, &csrf).await;
    let plan = thread
        .iter()
        .find(|note| note["intent"] == "plan")
        .expect("plan comment");
    let body = plan["body"].as_str().unwrap();
    assert!(body.contains("## Plan"));
    assert!(body.contains("- [ ] Add the column"));
    assert_eq!(plan_runs(&runs(&app, &ticket_id, &cookie, &csrf).await).len(), 1);
}
