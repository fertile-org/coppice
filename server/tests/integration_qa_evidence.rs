use coppice_server::domain::Id;
mod common;

use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

const MINI_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[tokio::test]
async fn qc_mock_records_screenshots_and_serves_them_to_signed_in_users() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let _git_keep = git_dir;
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_gateway("qc_evidence/work_on_ticket").await;
    let pool = state.db.clone().expect("pool");
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let qc_id =
        common::create_agent_with_preset_key(&app, "qc", "QC Agent", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree = common::setup_persisted_worktree(
        &pool,
        &local_path,
        env.worktrees.path(),
        "test-repo",
        &ticket_id,
    )
    .await;

    let evidence = worktree.join(".agent/qa-evidence");
    std::fs::create_dir_all(&evidence).unwrap();
    std::fs::write(evidence.join("save-disabled.png"), MINI_PNG).unwrap();

    seed_approved_plan(
        &pool,
        &ticket_id,
        "## Plan\n\n### Steps\n- [ ] Disable Save\n\n### Acceptance criteria\n- [ ] Save button is disabled while saving\n- [ ] Unit tests pass\n",
    )
    .await;

    sqlx::query(
        "UPDATE tickets SET status = 'in_qa', assignee_agent_id = $2 WHERE id = $1",
    )
    .bind(Id::parse_str(&ticket_id).unwrap())
    .bind(Id::parse_str(&qc_id).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let run_id = start_run(&app, &ticket_id, &cookie, &csrf).await;
    wait_succeeded(&pool, run_id).await;

    let ticket = get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    let plan = &ticket["approvedPlan"];
    assert!(
        plan["criteriaCommitSha"].as_str().is_some_and(|s| !s.is_empty()),
        "{plan}"
    );
    let criteria = plan["criteria"].as_array().expect("criteria");
    assert_eq!(criteria.len(), 2);
    assert_eq!(criteria[0]["status"], "met");
    assert_eq!(criteria[0]["screenshots"], serde_json::json!(["c0-s0.png"]));
    assert_eq!(
        criteria[0]["steps"],
        serde_json::json!([
            "Open the ticket drawer",
            "Click Save while a save is in flight"
        ])
    );
    assert_eq!(criteria[1]["status"], "met");
    assert!(criteria[1].get("screenshots").is_none() || criteria[1]["screenshots"] == serde_json::json!([]));

    let img = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/qa-evidence/c0-s0.png"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(img.status(), StatusCode::OK);
    let bytes = img.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), MINI_PNG);

    let unauth = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri(format!("/api/tickets/{ticket_id}/qa-evidence/c0-s0.png"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        unauth.status() == StatusCode::UNAUTHORIZED || unauth.status() == StatusCode::FORBIDDEN,
        "{}",
        unauth.status()
    );

    let bad_path = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/qa-evidence/../c0-s0.png"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(bad_path.status(), StatusCode::BAD_REQUEST);

    let non_image = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/qa-evidence/notes.txt"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(non_image.status(), StatusCode::BAD_REQUEST);

    // Stale commit → not_checked, no screenshots; criteriaCommitSha still present.
    sqlx::query("UPDATE ticket_criteria_results SET commit_sha = $2 WHERE ticket_id = $1")
        .bind(Id::parse_str(&ticket_id).unwrap())
        .bind("0000000000000000000000000000000000000000")
        .execute(&pool)
        .await
        .unwrap();
    let stale = get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(
        stale["approvedPlan"]["criteriaCommitSha"],
        "0000000000000000000000000000000000000000"
    );
    let criteria = stale["approvedPlan"]["criteria"].as_array().unwrap();
    assert!(criteria.iter().all(|c| c["status"] == "not_checked"));
    assert!(criteria.iter().all(|c| {
        c.get("screenshots").is_none() || c["screenshots"] == serde_json::json!([])
    }));
}

async fn seed_approved_plan(pool: &sqlx::PgPool, ticket_id: &str, body: &str) {
    let ticket_id = Id::parse_str(ticket_id).unwrap();
    let version: i32 = sqlx::query_scalar("SELECT content_version FROM tickets WHERE id = $1")
        .bind(ticket_id)
        .fetch_one(pool)
        .await
        .unwrap();
    let plan_id = Id::new();
    sqlx::query(
        r#"
        INSERT INTO ticket_plans (id, ticket_id, revision, body, content_version)
        VALUES ($1, $2, 1, $3, $4)
        "#,
    )
    .bind(plan_id)
    .bind(ticket_id)
    .bind(body)
    .bind(version)
    .execute(pool)
    .await
    .expect("plan");
    sqlx::query(
        "UPDATE tickets SET approved_plan_id = $2, approved_plan_content_version = $3 WHERE id = $1",
    )
    .bind(ticket_id)
    .bind(plan_id)
    .bind(version)
    .execute(pool)
    .await
    .expect("approve");
}

async fn start_run(app: &axum::Router, ticket_id: &str, cookie: &str, csrf: &str) -> Id {
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
    assert_eq!(res.status(), StatusCode::CREATED,);
    let body = common::json_body(res).await;
    body["run"]["id"].as_str().unwrap().parse().unwrap()
}

async fn wait_succeeded(pool: &sqlx::PgPool, run_id: Id) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(pool)
            .await
            .unwrap();
        if status == "succeeded" {
            return;
        }
        if status == "failed" {
            let err: Option<String> =
                sqlx::query_scalar("SELECT error_message FROM agent_runs WHERE id = $1")
                    .bind(run_id)
                    .fetch_one(pool)
                    .await
                    .unwrap();
            panic!("run {run_id} failed: {err:?}");
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
