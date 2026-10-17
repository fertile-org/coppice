mod common;

use axum::http::StatusCode;
use coppice_server::copy::review::{RECONSIDER_APPROACH, REVIEW_ROUND_NO_DIFF};
use coppice_server::domain::substatus::TicketStatus;
use coppice_server::domain::workflow::TransitionAction;
use coppice_server::providers::AgentRunResult;
use coppice_server::services::review_rounds::{FinishedReview, ReviewRoundService};
use tower::ServiceExt;
use uuid::Uuid;

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

fn done(summary: &str, blockers: Vec<String>) -> AgentRunResult {
    AgentRunResult::Done {
        summary: summary.into(),
        changed_files: vec![],
        tests_run: vec![],
        next_status: None,
        assign_to: None,
        updated_description: None,
        acceptance_criteria: None,
        mention_agents: vec!["backend_engineer".into()],
        agent_requests: vec![],
        blockers,
        split_tickets: vec![],
        knowledge_candidates: Vec::new(),
        how_it_works: None,
        changes_from_plan: None,
        look_here_first: None,
    }
}

#[tokio::test]
async fn a_capped_review_posts_one_comment_and_waits_for_a_person() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "reviewer", "Reviewer", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let agent_uuid = Uuid::parse_str(&agent_id).unwrap();

    sqlx::query(
        r#"
        UPDATE tickets
        SET status = 'in_review',
            review_round = 3,
            review_findings = 'Round 1: Check the parser.'
        WHERE id = $1
        "#,
    )
    .bind(ticket_uuid)
    .execute(pool)
    .await
    .unwrap();

    let service = ReviewRoundService::new(pool);
    assert!(service
        .stop_capped_review(
            ticket_uuid,
            TicketStatus::InReview,
            agent_uuid,
            "reviewer",
            "Reviewer",
            "work_on_ticket",
        )
        .await
        .unwrap());
    assert!(!service
        .stop_capped_review(
            ticket_uuid,
            TicketStatus::WaitForFinalReview,
            agent_uuid,
            "reviewer",
            "Reviewer",
            "work_on_ticket",
        )
        .await
        .unwrap());

    let notes = comments(&app, &ticket_id, &cookie, &csrf).await;
    let reconsider: Vec<_> = notes
        .iter()
        .filter(|note| note["body"] == RECONSIDER_APPROACH)
        .collect();
    assert_eq!(reconsider.len(), 1);

    let status: String = sqlx::query_scalar("SELECT status FROM tickets WHERE id = $1")
        .bind(ticket_uuid)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(status, "wait_for_final_review");
}

#[tokio::test]
async fn round_two_context_is_the_new_diff_and_earlier_findings() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    sqlx::query(
        r#"
        UPDATE tickets
        SET status = 'in_review',
            review_round = 1,
            review_anchor_sha = 'abc1234dead',
            review_findings = 'Round 1: Check the parser.'
        WHERE id = $1
        "#,
    )
    .bind(ticket_uuid)
    .execute(pool)
    .await
    .unwrap();

    let section = ReviewRoundService::new(pool)
        .context_section(
            ticket_uuid,
            TicketStatus::InReview,
            "work_on_ticket",
            "reviewer",
            "Reviewer",
            std::path::Path::new("/tmp"),
        )
        .await
        .unwrap()
        .expect("round 2 section");
    assert!(section.contains("Round 1: Check the parser."));
    assert!(section.contains(REVIEW_ROUND_NO_DIFF));
    assert!(!section.contains("ticket description"));
    assert!(ReviewRoundService::new(pool)
        .context_section(
            ticket_uuid,
            TicketStatus::InReview,
            "work_on_ticket",
            "backend_engineer",
            "Backend Engineer",
            std::path::Path::new("/tmp"),
        )
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn the_third_change_request_stops_the_loop() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    sqlx::query("UPDATE tickets SET status = 'in_review', review_round = 2 WHERE id = $1")
        .bind(ticket_uuid)
        .execute(pool)
        .await
        .unwrap();

    let mut action = TransitionAction {
        new_status: Some(TicketStatus::InProgress),
        enqueue_jobs: vec![coppice_server::domain::workflow::JobRequest {
            job_type: "work_on_ticket".into(),
            agent_id: Uuid::nil(),
            resume_agent_id: None,
        }],
        ..TransitionAction::default()
    };
    let mut body = "Rename the helper.".into();
    let result = done("Still broken.", vec!["Missing a test".into()]);
    let capped = ReviewRoundService::new(pool)
        .constrain_finished_review(
            &FinishedReview {
                ticket_id: ticket_uuid,
                status: TicketStatus::InReview,
                agent_key: "reviewer",
                agent_role: "Reviewer",
                result: &result,
                worktree: None,
            },
            &mut action,
            &mut body,
        )
        .await
        .unwrap();
    assert!(capped);
    assert_eq!(body, RECONSIDER_APPROACH);
    assert_eq!(action.new_status, Some(TicketStatus::WaitForFinalReview));
    assert!(action.enqueue_jobs.is_empty());

    let escalated: bool = sqlx::query_scalar("SELECT review_escalated FROM tickets WHERE id = $1")
        .bind(ticket_uuid)
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(escalated);
}
