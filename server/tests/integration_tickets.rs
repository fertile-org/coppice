mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn create_ticket_and_update_status() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/boards/{board_id}/tickets"),
            r#"{"title":"First ticket","description":"hello"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let ticket: serde_json::Value = common::json_body(res).await;
    let ticket_id = ticket["id"].as_str().unwrap();
    assert_eq!(ticket["status"], "backlog");
    common::skip_planning(&app, ticket_id, &cookie, &csrf).await;

    let patch = app
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
}

#[tokio::test]
async fn reject_invalid_status() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/boards/{board_id}/tickets"),
            r#"{"title":"Ticket","description":""}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let ticket: serde_json::Value = common::json_body(res).await;
    let ticket_id = ticket["id"].as_str().unwrap();

    let patch = app
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"not_a_column"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reject_done_with_waiting_substatus() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/boards/{board_id}/tickets"),
            r#"{"title":"Ticket","description":""}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let ticket: serde_json::Value = common::json_body(res).await;
    let ticket_id = ticket["id"].as_str().unwrap();
    common::skip_planning(&app, ticket_id, &cookie, &csrf).await;

    let set_substatus = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"in_progress","substatus":"waiting_for_human"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(set_substatus.status(), StatusCode::OK);

    let patch = app
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"done"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::BAD_REQUEST);
}

async fn create_repo_less_ticket(app: &axum::Router, board_id: &str, body: &str, cookie: &str, csrf: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/boards/{board_id}/tickets"),
            body,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    common::json_body(res).await
}

#[tokio::test]
async fn ticket_without_repo_defaults_to_the_only_registered_repo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;

    let ticket = create_repo_less_ticket(
        &app,
        &board_id,
        r#"{"title":"Quick add","description":""}"#,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(ticket["repoId"], repo_id);
}

#[tokio::test]
async fn ticket_without_repo_stays_repo_less_when_several_repos_exist() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let (_git_a, path_a) = common::create_temp_git_checkout();
    let (_git_b, path_b) = common::create_temp_git_checkout();
    common::register_test_repo(&app, &path_a.display().to_string(), &cookie, &csrf).await;
    let second =
        common::register_test_repo(&app, &path_b.display().to_string(), &cookie, &csrf).await;

    let ticket = create_repo_less_ticket(
        &app,
        &board_id,
        r#"{"title":"Ambiguous","description":""}"#,
        &cookie,
        &csrf,
    )
    .await;
    assert!(ticket["repoId"].is_null());

    let explicit = create_repo_less_ticket(
        &app,
        &board_id,
        &format!(r#"{{"title":"Explicit","description":"","repoId":"{second}"}}"#),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(explicit["repoId"], second);
}

#[tokio::test]
async fn ticket_without_repo_succeeds_when_no_repos_exist() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;

    let ticket = create_repo_less_ticket(
        &app,
        &board_id,
        r#"{"title":"No repos","description":""}"#,
        &cookie,
        &csrf,
    )
    .await;
    assert!(ticket["repoId"].is_null());
}

#[tokio::test]
async fn a_large_board_lists_quickly_without_descriptions() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().unwrap();
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let board = uuid::Uuid::parse_str(&board_id).unwrap();
    let agent_id = uuid::Uuid::parse_str(
        &common::create_agent_with_preset_key(&app, "backend_engineer", "Builder", &cookie, &csrf).await,
    )
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO tickets (id, board_id, title, description, status, created_by, assignee_agent_id)
        SELECT gen_random_uuid(), $1, 'Ticket ' || n, repeat('d', 2000), 'backlog', 'user', $2
        FROM generate_series(1, 2000) AS n
        "#,
    )
    .bind(board)
    .bind(agent_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO ticket_comments (id, ticket_id, author_type, body, intent)
        SELECT gen_random_uuid(), t.id, 'human', 'note ' || n, 'progress_update'
        FROM tickets t, generate_series(1, 10) AS n
        WHERE t.board_id = $1
        "#,
    )
    .bind(board)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
        SELECT gen_random_uuid(), t.id, $2, 'plan_ticket', 'succeeded', 'permissive'
        FROM tickets t WHERE t.board_id = $1
        "#,
    )
    .bind(board)
    .bind(agent_id)
    .execute(&pool)
    .await
    .unwrap();

    let started = std::time::Instant::now();
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{board_id}/tickets"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(res.status(), StatusCode::OK);
    let list = common::json_body(res).await;
    let tickets = list.as_array().unwrap();
    assert_eq!(tickets.len(), 2000);
    assert!(tickets.iter().all(|ticket| ticket.get("description").is_none()));
    assert_eq!(tickets[0]["latestPlanRunStatus"], "succeeded");
    assert_eq!(tickets[0]["substatusDisplay"], serde_json::Value::Null);
    assert!(tickets[0]["lastActivityAt"].is_string());
    // One set-based query instead of five per ticket; was ~0.4 s at this size.
    assert!(elapsed < std::time::Duration::from_secs(1), "board list took {elapsed:?}");
    println!("2000-ticket board list: {elapsed:?}");

    // The ticket view still gets the description.
    let one = common::get_ticket(&app, tickets[0]["id"].as_str().unwrap(), &cookie, &csrf).await;
    assert_eq!(one["description"].as_str().unwrap().len(), 2000);
}
