//! A ticket run is announced on the event bus as soon as it is queued, so the
//! drawer shows Stop while every worker is busy. These states spawn no
//! workers, so runs stay `queued` like they do behind a full worker pool.

mod common;

use coppice_server::events::AppEvent;
use coppice_server::providers::AgentRunResult;
use coppice_server::services::result_contract::apply_agent_result;
use coppice_server::services::run_orchestrator::RunOrchestrator;
use coppice_server::services::run_service::RunService;
use std::time::Duration;
use tokio::sync::broadcast::Receiver;
use uuid::Uuid;

/// The first `agent_run.started` for `ticket_id`, skipping other events.
async fn next_run_started(events: &mut Receiver<AppEvent>, ticket_id: Uuid) -> (Uuid, Uuid, String) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let event = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("timed out waiting for agent_run.started")
            .expect("event bus closed");
        if let AppEvent::AgentRunStarted {
            run_id,
            ticket_id: event_ticket_id,
            agent_id,
            status,
        } = event
        {
            if event_ticket_id == ticket_id {
                return (run_id, agent_id, status);
            }
        }
    }
}

#[tokio::test]
async fn assign_with_auto_start_publishes_queued_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) =
        common::bootstrap_and_login_with_state_config(|config| {
            config.workflow.auto_start_runs = true;
        })
        .await;
    let pool = state.db.as_ref().expect("db pool");

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;

    let mut events = state.event_bus.subscribe();
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;

    let ticket_id: Uuid = ticket_id.parse().unwrap();
    let (run_id, event_agent_id, status) = next_run_started(&mut events, ticket_id).await;
    assert_eq!(status, "queued");
    assert_eq!(event_agent_id.to_string(), agent_id);

    // Published after commit: the run the event names is already readable.
    let run = RunService::new(pool).get(run_id).await.expect("queued run");
    assert_eq!(run.ticket_id, Some(ticket_id));
}

#[tokio::test]
async fn orchestrator_hand_off_publishes_queued_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) =
        common::bootstrap_and_login_with_state_config(|config| {
            config.workflow.auto_start_runs = true;
        })
        .await;
    let pool = state.db.as_ref().expect("db pool");

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let qc_id = common::create_agent_with_preset_key(&app, "qc", "QC Agent", &cookie, &csrf).await;

    let ticket_id: Uuid = ticket_id.parse().unwrap();
    let engineer_id: Uuid = engineer_id.parse().unwrap();
    let qc_id: Uuid = qc_id.parse().unwrap();
    sqlx::query("UPDATE tickets SET status = 'in_qa', assignee_agent_id = $2 WHERE id = $1")
        .bind(ticket_id)
        .bind(qc_id)
        .execute(pool)
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
        VALUES ($1, $2, $3, 'work_on_ticket', 'running', 'permissive-default')
        "#,
    )
    .bind(run_id)
    .bind(ticket_id)
    .bind(qc_id)
    .execute(pool)
    .await
    .unwrap();

    // QC reports a defect and hands the ticket back to the engineer.
    let contract: AgentRunResult = serde_json::from_str(include_str!(
        "../../fixtures/agent-responses/qc/work_on_ticket.json"
    ))
    .unwrap();
    let apply = apply_agent_result(&contract).unwrap();
    let run = RunService::new(pool).get(run_id).await.unwrap();

    let mut events = state.event_bus.subscribe();
    RunOrchestrator::new(pool, &state.config.workflow)
        .with_event_bus(&state.event_bus)
        .finish_run(&run, &contract, apply, None, None)
        .await
        .unwrap();

    let (queued_id, agent_id, status) = next_run_started(&mut events, ticket_id).await;
    assert_eq!(status, "queued");
    assert_eq!(agent_id, engineer_id);
    let queued = RunService::new(pool).get(queued_id).await.expect("queued run");
    assert_eq!(queued.job_type, "work_on_ticket");
}
