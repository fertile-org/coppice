//! Reconcile `agent_jobs` left in `processing` after a crash, panic, or partial finish.
use crate::domain::Id;
use crate::domain::run::{run_status_from_str, run_status_to_str, AgentRun, RunStatus};
use crate::domain::workflow::JOB_TYPE_PLAN_TICKET;
use crate::events::{publish_run_finished, publish_ticket_updated};
use crate::services::job_service::JobService;
use crate::services::run_service::RunService;
use crate::services::ticket_service::TicketService;
use crate::AppState;

const ORPHAN_PLAN_MSG: &str =
    "Planning finished without storing a plan. Run planning again on this ticket.";

async fn fail_succeeded_plan_run(
    pool: &sqlx::PgPool,
    run_id: Id,
    message: &str,
) -> Option<AgentRun> {
    let updated = sqlx::query(
        r#"
        UPDATE agent_runs
        SET status = $2,
            error_message = $3,
            ended_at = COALESCE(ended_at, now()),
            submitted_result = NULL
        WHERE id = $1 AND status = $4
        "#,
    )
    .bind(run_id)
    .bind(run_status_to_str(RunStatus::Failed))
    .bind(message)
    .bind(run_status_to_str(RunStatus::Succeeded))
    .execute(pool)
    .await
    .ok()?;
    if updated.rows_affected() == 0 {
        return None;
    }
    RunService::new(pool).get(run_id).await.ok()
}

#[derive(sqlx::FromRow)]
struct OrphanJob {
    job_id: Id,
    run_id: Id,
    job_type: String,
    run_status: String,
}

fn run_is_terminal(status: RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
    )
}

async fn plan_row_exists(pool: &sqlx::PgPool, run_id: Id) -> bool {
    sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM ticket_plans WHERE run_id = $1 LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .is_some()
}

/// Jobs stuck in `processing` while their run is already terminal.
pub async fn recover_orphaned_jobs(state: &AppState) {
    let Some(pool) = state.db.as_ref() else {
        return;
    };
    let Ok(rows) = sqlx::query_as::<_, OrphanJob>(
        r#"
        SELECT j.id AS job_id, j.run_id, j.job_type, r.status AS run_status
        FROM agent_jobs j
        INNER JOIN agent_runs r ON r.id = j.run_id
        WHERE j.status = 'processing'
        "#,
    )
    .fetch_all(pool)
    .await
    else {
        return;
    };

    let job_svc = JobService::new(pool);

    for row in rows {
        let Some(run_status) = run_status_from_str(&row.run_status) else {
            continue;
        };
        if !run_is_terminal(run_status) {
            continue;
        }

        if row.job_type == JOB_TYPE_PLAN_TICKET && run_status == RunStatus::Succeeded {
            if plan_row_exists(pool, row.run_id).await {
                let _ = job_svc.mark_done(row.job_id).await;
            } else if let Some(failed) = fail_succeeded_plan_run(pool, row.run_id, ORPHAN_PLAN_MSG).await
            {
                let _ = job_svc.mark_failed(row.job_id, ORPHAN_PLAN_MSG).await;
                if let Some(ticket_id) = failed.ticket_id {
                    if let Ok(ticket) = TicketService::new(pool).get(ticket_id).await {
                        publish_ticket_updated(&state.event_bus, &ticket);
                    }
                }
                publish_run_finished(
                    state,
                    pool,
                    failed.id,
                    failed.ticket_id,
                    failed.agent_id,
                    failed.status,
                    failed.error_message.clone(),
                )
                .await;
            }
            continue;
        }

        match run_status {
            RunStatus::Succeeded => {
                let _ = job_svc.mark_done(row.job_id).await;
            }
            RunStatus::Cancelled => {
                let _ = job_svc.mark_cancelled(row.job_id).await;
            }
            RunStatus::Failed => {
                let _ = job_svc
                    .mark_failed(row.job_id, "orphaned processing job for a failed run")
                    .await;
            }
            _ => {}
        }
    }
}