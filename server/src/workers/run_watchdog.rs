use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::run::RunStatus;
use crate::events::mark_run_interrupted;
use crate::services::run_orchestrator::RunOrchestrator;
use crate::services::run_service::RunService;
use crate::sessions::live_message::LiveMessage;
use crate::sessions::opencode_client::OpenCodeClient;
use crate::AppState;

const WATCHDOG_INTERVAL_SECS: u64 = 30;

pub fn spawn_run_watchdog(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(WATCHDOG_INTERVAL_SECS));
        loop {
            interval.tick().await;
            run_watchdog_pass_once(&state).await;
        }
    });
}

pub async fn run_watchdog_pass_once(state: &AppState) {
    let Some(pool) = state.db.as_ref() else {
        return;
    };

    let run_svc = RunService::new(pool);
    let Ok(runs) = run_svc.list_active_runs().await else {
        return;
    };

    for run in runs {
        let elapsed_secs = run_elapsed_secs(&run.started_at, &run.created_at);

        if run.status == RunStatus::Queued && elapsed_secs > 300 {
            tracing::warn!(
                run_id = %run.id,
                ticket_id = ?run.ticket_id,
                elapsed_secs,
                "agent run queued for over 5 minutes"
            );
            continue;
        }

        if run.status != RunStatus::Running {
            continue;
        }

        let Ok(connector) = run_svc.agent_connector_for_run(run.agent_id).await else {
            continue;
        };

        let run_server = connector
            .as_deref()
            .and_then(crate::providers::descriptor)
            .is_some_and(|d| d.caps.run_server);
        if !run_server {
            tracing::debug!(
                run_id = %run.id,
                ticket_id = ?run.ticket_id,
                elapsed_secs,
                connector = ?connector,
                "agent run in progress"
            );
            publish_heartbeat(state, run.id, None, elapsed_secs);
            continue;
        }

        let (Some(session_id), Some(worktree)) = (&run.session_id, &run.worktree_path) else {
            tracing::debug!(
                run_id = %run.id,
                ticket_id = ?run.ticket_id,
                elapsed_secs,
                "opencode run waiting for session attachment"
            );
            publish_heartbeat(state, run.id, None, elapsed_secs);
            continue;
        };

        let Some(base_url) = state.opencode_runs.base_url(&run.id.to_string()) else {
            // The worker stops the server before recording the final status and
            // drops the stream only afterwards; while it owns the run, a missing
            // server is not a lost one.
            if state.run_streams.get(run.id).is_some() {
                publish_heartbeat(state, run.id, None, elapsed_secs);
                continue;
            }
            tracing::warn!(
                run_id = %run.id,
                ticket_id = ?run.ticket_id,
                "opencode server missing while run is active; marking interrupted"
            );
            interrupt_run(state, pool, run.id, "opencode server lost during run").await;
            continue;
        };

        let client = OpenCodeClient::new(&base_url);
        let directory = std::path::Path::new(worktree);
        match client.session_status(directory, session_id).await {
            Ok(Some(session_status)) => {
                tracing::info!(
                    run_id = %run.id,
                    ticket_id = ?run.ticket_id,
                    session_id = %session_id,
                    %session_status,
                    elapsed_secs,
                    "opencode run heartbeat"
                );
                publish_heartbeat(state, run.id, Some(session_status), elapsed_secs);
            }
            Ok(None) => {
                tracing::warn!(
                    run_id = %run.id,
                    ticket_id = ?run.ticket_id,
                    session_id = %session_id,
                    elapsed_secs,
                    "opencode session missing while run is active; marking interrupted"
                );
                interrupt_run(state, pool, run.id, "opencode session lost during run").await;
            }
            Err(err) => {
                tracing::warn!(
                    run_id = %run.id,
                    ticket_id = ?run.ticket_id,
                    error = %err,
                    elapsed_secs,
                    "failed to poll opencode session status"
                );
            }
        }
    }
}

async fn interrupt_run(state: &AppState, pool: &PgPool, run_id: Uuid, reason: &str) {
    match mark_run_interrupted(state, run_id, reason).await {
        Ok(interrupted) => {
            RunOrchestrator::new(pool, &state.config.workflow)
                .handle_terminal_run(&interrupted)
                .await;
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                %run_id,
                "failed to mark watchdog-observed run interrupted"
            );
        }
    }
}

fn run_elapsed_secs(started_at: &Option<OffsetDateTime>, created_at: &OffsetDateTime) -> u64 {
    let anchor = started_at.unwrap_or(*created_at);
    OffsetDateTime::now_utc()
        .unix_timestamp()
        .saturating_sub(anchor.unix_timestamp())
        .max(0) as u64
}

fn publish_heartbeat(
    state: &AppState,
    run_id: Uuid,
    session_status: Option<String>,
    elapsed_secs: u64,
) {
    let Some(handle) = state.run_streams.get(run_id) else {
        return;
    };
    handle.publish(LiveMessage::Heartbeat {
        session_status,
        elapsed_secs,
    });
}
