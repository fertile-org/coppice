mod common;

use axum::http::StatusCode;
use std::time::Duration;
use tower::ServiceExt;

async fn latest_run(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> Option<serde_json::Value> {
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
    body["runs"]
        .as_array()
        .and_then(|runs| runs.first())
        .cloned()
}

async fn run_status(app: &axum::Router, ticket_id: &str, cookie: &str, csrf: &str) -> String {
    latest_run(app, ticket_id, cookie, csrf)
        .await
        .and_then(|run| run["status"].as_str().map(str::to_string))
        .unwrap_or_else(|| "missing".into())
}

async fn wait_status(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
    wanted: &[&str],
    timeout: Duration,
) -> String {
    let runs = common::poll_runs_until_count(
        app,
        ticket_id,
        cookie,
        csrf,
        &format!("status in {wanted:?}"),
        timeout,
        |runs| {
            runs.first()
                .and_then(|run| run["status"].as_str())
                .is_some_and(|status| wanted.contains(&status))
        },
    )
    .await;
    runs[0]["status"].as_str().unwrap().to_string()
}

async fn setup_ticket(
    app: &axum::Router,
    board_id: &str,
    repo_id: &str,
    agent_id: &str,
    cookie: &str,
    csrf: &str,
) -> String {
    let ticket_id = common::create_test_ticket(app, board_id, cookie, csrf).await;
    common::set_ticket_repo(app, &ticket_id, repo_id, cookie, csrf).await;
    common::assign_agent_to_ticket(app, &ticket_id, agent_id, cookie, csrf).await;
    ticket_id
}

fn enable_second_connector(config: &mut coppice_server::AppConfig) {
    config.agent.connectors.cursor.enabled = Some(true);
    config.agent.connectors.cursor.command = "coppice-test-missing-cursor-cli".into();
}

#[tokio::test]
async fn other_connector_starts_when_mock_is_at_its_limit() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_worker_config(3, enable_second_connector).await;
    std::env::set_var("MOCK_AGENT_RESPONSE", "hold_slot");

    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let mock_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Mock A", &cookie, &csrf)
            .await;
    let other_id =
        common::create_agent_with_preset_key(&app, "pm", "Cursor B", &cookie, &csrf).await;
    sqlx::query("UPDATE agents SET connector = 'cursor' WHERE id = $1::uuid")
        .bind(&other_id)
        .execute(pool)
        .await
        .expect("set cursor connector");

    let mock_one = setup_ticket(&app, &board_id, &repo_id, &mock_id, &cookie, &csrf).await;
    let mock_two = setup_ticket(&app, &board_id, &repo_id, &mock_id, &cookie, &csrf).await;
    wait_status(
        &app,
        &mock_one,
        &cookie,
        &csrf,
        &["running"],
        Duration::from_secs(10),
    )
    .await;
    wait_status(
        &app,
        &mock_two,
        &cookie,
        &csrf,
        &["running"],
        Duration::from_secs(10),
    )
    .await;

    let mock_extra = setup_ticket(&app, &board_id, &repo_id, &mock_id, &cookie, &csrf).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        run_status(&app, &mock_extra, &cookie, &csrf).await,
        "queued"
    );

    let other = setup_ticket(&app, &board_id, &repo_id, &other_id, &cookie, &csrf).await;
    let other_status = wait_status(
        &app,
        &other,
        &cookie,
        &csrf,
        &["running", "failed"],
        Duration::from_secs(10),
    )
    .await;
    assert_ne!(other_status, "queued");
    assert_eq!(
        run_status(&app, &mock_extra, &cookie, &csrf).await,
        "queued"
    );
    assert_eq!(run_status(&app, &mock_one, &cookie, &csrf).await, "running");
    assert_eq!(run_status(&app, &mock_two, &cookie, &csrf).await, "running");

    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn worker_count_caps_total_across_connectors() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_worker_config(1, enable_second_connector).await;
    std::env::set_var("MOCK_AGENT_RESPONSE", "hold_slot");

    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let mock_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Mock A", &cookie, &csrf)
            .await;
    let other_id =
        common::create_agent_with_preset_key(&app, "pm", "Cursor B", &cookie, &csrf).await;
    sqlx::query("UPDATE agents SET connector = 'cursor' WHERE id = $1::uuid")
        .bind(&other_id)
        .execute(pool)
        .await
        .expect("set cursor connector");

    let mock_ticket = setup_ticket(&app, &board_id, &repo_id, &mock_id, &cookie, &csrf).await;
    wait_status(
        &app,
        &mock_ticket,
        &cookie,
        &csrf,
        &["running"],
        Duration::from_secs(10),
    )
    .await;

    let other = setup_ticket(&app, &board_id, &repo_id, &other_id, &cookie, &csrf).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(run_status(&app, &other, &cookie, &csrf).await, "queued");
    assert_eq!(
        run_status(&app, &mock_ticket, &cookie, &csrf).await,
        "running"
    );

    std::env::remove_var("MOCK_AGENT_RESPONSE");
}

#[tokio::test]
async fn cancel_releases_connector_slot_for_the_next_job() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_worker_config(3, |_| {}).await;
    std::env::set_var("MOCK_AGENT_RESPONSE", "hold_slot");

    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let mock_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Mock A", &cookie, &csrf)
            .await;

    let first = setup_ticket(&app, &board_id, &repo_id, &mock_id, &cookie, &csrf).await;
    let second = setup_ticket(&app, &board_id, &repo_id, &mock_id, &cookie, &csrf).await;
    wait_status(
        &app,
        &first,
        &cookie,
        &csrf,
        &["running"],
        Duration::from_secs(10),
    )
    .await;
    wait_status(
        &app,
        &second,
        &cookie,
        &csrf,
        &["running"],
        Duration::from_secs(10),
    )
    .await;

    let queued = setup_ticket(&app, &board_id, &repo_id, &mock_id, &cookie, &csrf).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(run_status(&app, &queued, &cookie, &csrf).await, "queued");

    let first_run = latest_run(&app, &first, &cookie, &csrf)
        .await
        .expect("first run");
    let run_id = first_run["id"].as_str().expect("run id");
    let stop = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/agent-runs/{run_id}/stop"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(stop.status(), StatusCode::OK);

    wait_status(
        &app,
        &queued,
        &cookie,
        &csrf,
        &["running"],
        Duration::from_secs(10),
    )
    .await;

    std::env::remove_var("MOCK_AGENT_RESPONSE");
}
