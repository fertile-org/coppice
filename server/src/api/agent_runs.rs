use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::run::{run_status_to_str, AgentRun};
use crate::services::run_service::{RunError, RunService};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/agent-runs/{run_id}", get(get_run))
        .route("/api/agent-runs/{run_id}/stop", post(stop_run))
        .route("/api/agent-runs/{run_id}/retry", post(retry_run))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunResponse {
    id: Uuid,
    ticket_id: Uuid,
    agent_id: Uuid,
    job_type: String,
    status: String,
    sandbox_profile_id: String,
    worktree_path: Option<String>,
    branch_name: Option<String>,
    error_message: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SingleRunResponse {
    run: RunResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunsListResponse {
    runs: Vec<RunResponse>,
}

pub(crate) fn single_run_response(run: AgentRun) -> SingleRunResponse {
    SingleRunResponse {
        run: run_to_response(run),
    }
}

pub(crate) fn runs_list_response(runs: Vec<AgentRun>) -> RunsListResponse {
    RunsListResponse {
        runs: runs.into_iter().map(run_to_response).collect(),
    }
}

pub(crate) fn run_to_response(run: AgentRun) -> RunResponse {
    RunResponse {
        id: run.id,
        ticket_id: run.ticket_id,
        agent_id: run.agent_id,
        job_type: run.job_type,
        status: run_status_to_str(run.status).to_string(),
        sandbox_profile_id: run.sandbox_profile_id,
        worktree_path: run.worktree_path,
        branch_name: run.branch_name,
        error_message: run.error_message,
        started_at: run.started_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
        ended_at: run.ended_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
        created_at: run.created_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn map_error(err: RunError) -> StatusCode {
    match err {
        RunError::ActiveRunExists => StatusCode::CONFLICT,
        RunError::NotFound => StatusCode::NOT_FOUND,
        RunError::Validation(_) => StatusCode::BAD_REQUEST,
        RunError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(run_id): Path<Uuid>,
) -> Result<Json<SingleRunResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RunService::new(pool);
    let run = service.get(run_id).await.map_err(map_error)?;
    Ok(Json(single_run_response(run)))
}

async fn stop_run(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(run_id): Path<Uuid>,
) -> Result<Json<SingleRunResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RunService::new(pool);
    let run = service.stop(run_id).await.map_err(map_error)?;
    Ok(Json(single_run_response(run)))
}

async fn retry_run(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(run_id): Path<Uuid>,
) -> Result<(StatusCode, Json<SingleRunResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RunService::new(pool);
    let run = service.retry(run_id).await.map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(single_run_response(run))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::run::RunStatus;

    #[test]
    fn run_status_serializes_as_snake_case() {
        let run = AgentRun {
            id: Uuid::new_v4(),
            ticket_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            job_type: "work_on_ticket".into(),
            status: RunStatus::Running,
            sandbox_profile_id: "permissive-default".into(),
            worktree_path: None,
            branch_name: None,
            error_message: None,
            started_at: None,
            ended_at: None,
            created_at: time::OffsetDateTime::now_utc(),
        };
        let response = run_to_response(run);
        assert_eq!(response.status, "running");
    }
}
