#![cfg(feature = "embedded-test-db")]

mod common;

use coppice_server::domain::Id;
use coppice_server::domain::job::job_status_to_str;
use coppice_server::domain::run::run_status_to_str;
use coppice_server::domain::run::RunStatus;
use coppice_server::domain::workflow::JOB_TYPE_PLAN_TICKET;
use coppice_server::services::job_recovery;

#[tokio::test]
async fn recover_succeeded_plan_without_ticket_plan() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, _app, _cookie, _csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");

    let board_id = Id::new();
    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
        .bind(board_id)
        .bind("recovery")
        .bind(format!("recovery-{board_id}"))
        .execute(pool)
        .await
        .expect("board");

    let agent_id = Id::new();
    sqlx::query(
        r#"
        INSERT INTO agents (id, name, role, responsibilities, system_prompt, connector)
        VALUES ($1, $2, 'PM', '{}', 'x', 'mock-provider')
        "#,
    )
    .bind(agent_id)
    .bind("pm")
    .execute(pool)
    .await
    .expect("agent");

    let ticket_id = Id::new();
    sqlx::query(
        r#"
        INSERT INTO tickets (id, board_id, title, description, status, created_by)
        VALUES ($1, $2, 't', '', 'backlog', 'user')
        "#,
    )
    .bind(ticket_id)
    .bind(board_id)
    .execute(pool)
    .await
    .expect("ticket");

    let run_id = Id::new();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, agent_id, ticket_id, job_type, status, sandbox_profile_id, context_profile)
        VALUES ($1, $2, $3, $4, $5, 'permissive', 'full')
        "#,
    )
    .bind(run_id)
    .bind(agent_id)
    .bind(ticket_id)
    .bind(JOB_TYPE_PLAN_TICKET)
    .bind(run_status_to_str(RunStatus::Succeeded))
    .execute(pool)
    .await
    .expect("run");

    let job_id = Id::new();
    sqlx::query(
        r#"
        INSERT INTO agent_jobs (id, run_id, job_type, status)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(job_id)
    .bind(run_id)
    .bind(JOB_TYPE_PLAN_TICKET)
    .bind(job_status_to_str(coppice_server::domain::job::JobStatus::Processing))
    .execute(pool)
    .await
    .expect("job");

    job_recovery::recover_orphaned_jobs(&state).await;

    let run_status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .expect("run status");
    assert_eq!(run_status, "failed");

    let job_status: String = sqlx::query_scalar("SELECT status FROM agent_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(pool)
        .await
        .expect("job status");
    assert_eq!(job_status, "failed");
}