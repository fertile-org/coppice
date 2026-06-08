use crate::api::auth::pool_from_state;
use crate::domain::job::{job_status_to_str, AgentJob};
use crate::middleware::admin::AdminUser;
use crate::services::job_service::{JobError, JobService};
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/agent-jobs", get(list_jobs))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobResponse {
    id: Uuid,
    run_id: Uuid,
    job_type: String,
    status: String,
    attempts: i32,
    max_attempts: i32,
    available_at: String,
    locked_at: Option<String>,
    locked_by: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobsListResponse {
    jobs: Vec<JobResponse>,
}

fn job_to_response(job: AgentJob) -> JobResponse {
    JobResponse {
        id: job.id,
        run_id: job.run_id,
        job_type: job.job_type,
        status: job_status_to_str(job.status).to_string(),
        attempts: job.attempts,
        max_attempts: job.max_attempts,
        available_at: job.available_at.format(&Rfc3339).unwrap_or_default(),
        locked_at: job.locked_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
        locked_by: job.locked_by,
        created_at: job.created_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn map_error(err: JobError) -> StatusCode {
    match err {
        JobError::NotFound => StatusCode::NOT_FOUND,
        JobError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_jobs(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
) -> Result<Json<JobsListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = JobService::new(pool);
    let jobs = service.list_all().await.map_err(map_error)?;
    Ok(Json(JobsListResponse {
        jobs: jobs.into_iter().map(job_to_response).collect(),
    }))
}
