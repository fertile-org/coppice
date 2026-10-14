mod common;

use axum::http::StatusCode;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn implementation_brief_is_on_the_ticket_and_does_not_block_accept() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_gateway("mcp/reviewer_brief").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;

    let with_brief = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &with_brief, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &with_brief, &agent_id, &cookie, &csrf).await;
    let run_id = start_run(&app, &with_brief, &cookie, &csrf).await;
    let pool = state.db.clone().unwrap();
    wait_succeeded(&pool, run_id).await;

    let ticket = get_ticket(&app, &with_brief, &cookie, &csrf).await;
    assert_eq!(
        ticket["reviewerBrief"]["howItWorks"],
        "The parser reads the header first."
    );
    assert!(ticket["reviewerBrief"].get("changesFromPlan").is_none());
    assert_eq!(ticket["reviewerBrief"]["lookHereFirst"], "src/parser.rs");
    let commit_sha = ticket["reviewerBrief"]["commitSha"]
        .as_str()
        .expect("brief stores the commit it was written for")
        .to_string();
    assert!(!commit_sha.is_empty());

    set_status(&app, &with_brief, "wait_for_final_review", &cookie, &csrf).await;
    let in_review = get_ticket(&app, &with_brief, &cookie, &csrf).await;
    assert_eq!(in_review["reviewerBrief"]["commitSha"], commit_sha);

    let other_sha = "0000000000000000000000000000000000000000";
    sqlx::query("UPDATE ticket_reviewer_briefs SET commit_sha = $2 WHERE ticket_id = $1")
        .bind(Uuid::parse_str(&with_brief).unwrap())
        .bind(other_sha)
        .execute(&pool)
        .await
        .unwrap();
    let hidden = get_ticket(&app, &with_brief, &cookie, &csrf).await;
    assert!(hidden.get("reviewerBrief").is_none());

    sqlx::query("UPDATE ticket_reviewer_briefs SET commit_sha = $2 WHERE ticket_id = $1")
        .bind(Uuid::parse_str(&with_brief).unwrap())
        .bind(&commit_sha)
        .execute(&pool)
        .await
        .unwrap();
    let shown = get_ticket(&app, &with_brief, &cookie, &csrf).await;
    assert_eq!(shown["reviewerBrief"]["howItWorks"], "The parser reads the header first.");

    std::env::set_var("MOCK_AGENT_RESPONSE", "done");
    let without_brief = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &without_brief, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &without_brief, &agent_id, &cookie, &csrf).await;
    let run_id = start_run(&app, &without_brief, &cookie, &csrf).await;
    wait_succeeded(&pool, run_id).await;
    let ticket = get_ticket(&app, &without_brief, &cookie, &csrf).await;
    assert!(ticket.get("reviewerBrief").is_none());

    let approved = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{with_brief}/final-approve"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);
    let approved = common::json_body(approved).await;
    assert_eq!(approved["status"], "done");
    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

async fn start_run(app: &axum::Router, ticket_id: &str, cookie: &str, csrf: &str) -> Uuid {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/run-agent"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = common::json_body(res).await;
    body["run"]["id"].as_str().unwrap().parse().unwrap()
}

async fn wait_succeeded(pool: &sqlx::PgPool, run_id: Uuid) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(pool)
            .await
            .unwrap();
        if status == "succeeded" {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "run {run_id} stayed {status}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

async fn get_ticket(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    common::json_body(res).await
}

async fn set_status(app: &axum::Router, ticket_id: &str, status: &str, cookie: &str, csrf: &str) {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            &format!(r#"{{"status":"{status}"}}"#),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
