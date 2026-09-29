use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::agent::Agent;
use crate::domain::knowledge_compaction::{BatchStatus, CompactionBatch};
use crate::services::knowledge_compaction_service::{
    CompactionError, KnowledgeCompactionService, QueueCounts,
};
use crate::services::workspace_settings_service::WorkspaceSettingsService;
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/knowledge/compaction", get(get_status))
        .route("/api/knowledge/compaction/run", post(run_now))
        .route("/api/knowledge/compaction/retry", post(retry))
        .route("/api/knowledge/compaction/cancel", post(cancel))
}

struct CompactionApiError {
    status: StatusCode,
    message: String,
}

impl From<CompactionError> for CompactionApiError {
    fn from(error: CompactionError) -> Self {
        match error {
            CompactionError::NotConfigured
            | CompactionError::AgentDisabled
            | CompactionError::ActiveBatch
            | CompactionError::NothingQueued
            | CompactionError::NothingToRetry
            | CompactionError::NoActiveBatch
            | CompactionError::BatchNotActive(_) => Self {
                status: StatusCode::CONFLICT,
                message: error.to_string(),
            },
            other => {
                tracing::error!(error = %other, "knowledge compaction error");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: "Knowledge compaction operation failed.".into(),
                }
            }
        }
    }
}

impl From<StatusCode> for CompactionApiError {
    fn from(status: StatusCode) -> Self {
        Self {
            status,
            message: status.canonical_reason().unwrap_or("error").into(),
        }
    }
}

impl IntoResponse for CompactionApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "message": self.message })),
        )
            .into_response()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum CompactionState {
    NotConfigured,
    AgentDisabled,
    Running,
    Failed,
    Idle,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentSummary {
    id: Uuid,
    name: String,
    enabled: bool,
    connector: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchResponse {
    id: Uuid,
    agent_id: Uuid,
    agent_name: Option<String>,
    run_id: Option<Uuid>,
    status: &'static str,
    trigger: &'static str,
    ticket_count: i32,
    candidate_count: Option<i32>,
    summary: Option<String>,
    error_message: Option<String>,
    created_at: String,
    started_at: Option<String>,
    ended_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusResponse {
    state: CompactionState,
    configured: bool,
    agent: Option<AgentSummary>,
    queued_count: i64,
    blocked_count: i64,
    oldest_queued_at: Option<String>,
    active_batch: Option<BatchResponse>,
    last_batch: Option<BatchResponse>,
    next_scheduled_at: Option<String>,
    interval_secs: u64,
    batch_max_tickets: usize,
}

fn format_time(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}

fn batch_response(batch: CompactionBatch) -> BatchResponse {
    BatchResponse {
        id: batch.id,
        agent_id: batch.agent_id,
        agent_name: batch.agent_name,
        run_id: batch.run_id,
        status: batch.status.as_str(),
        trigger: batch.trigger.as_str(),
        ticket_count: batch.ticket_count,
        candidate_count: batch.candidate_count,
        summary: batch.summary,
        error_message: batch.error_message,
        created_at: format_time(batch.created_at),
        started_at: batch.started_at.map(format_time),
        ended_at: batch.ended_at.map(format_time),
    }
}

fn agent_summary(agent: Agent) -> AgentSummary {
    AgentSummary {
        id: agent.id,
        name: agent.name,
        enabled: agent.enabled,
        connector: agent.connector,
    }
}

async fn get_status(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<StatusResponse>, CompactionApiError> {
    let pool = pool_from_state(&state)?;
    let service = KnowledgeCompactionService::new(pool, &state.config.knowledge);
    let agent = WorkspaceSettingsService::new(pool)
        .knowledge_compaction_agent()
        .await
        .map_err(CompactionError::from)?;
    let QueueCounts {
        queued,
        blocked,
        oldest_queued_at,
    } = service.queue_counts().await?;
    let active_batch = service.active_batch().await?;
    let last_batch = service.last_batch().await?;

    let compaction_state = match (&agent, &active_batch, &last_batch) {
        (None, _, _) => CompactionState::NotConfigured,
        (Some(agent), _, _) if !agent.enabled => CompactionState::AgentDisabled,
        (_, Some(_), _) => CompactionState::Running,
        (_, None, Some(last)) if last.status == BatchStatus::Failed => CompactionState::Failed,
        _ => CompactionState::Idle,
    };
    let next_scheduled_at = if matches!(
        compaction_state,
        CompactionState::Idle | CompactionState::Failed
    ) {
        service.next_scheduled_at().await?.map(format_time)
    } else {
        None
    };

    Ok(Json(StatusResponse {
        state: compaction_state,
        configured: agent.is_some(),
        agent: agent.map(agent_summary),
        queued_count: queued,
        blocked_count: blocked,
        oldest_queued_at: oldest_queued_at.map(format_time),
        active_batch: active_batch.map(batch_response),
        last_batch: last_batch.map(batch_response),
        next_scheduled_at,
        interval_secs: state.config.knowledge.compaction.interval_secs,
        batch_max_tickets: state.config.knowledge.compaction.batch_max_tickets,
    }))
}

async fn run_now(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
) -> Result<(StatusCode, Json<BatchResponse>), CompactionApiError> {
    let pool = pool_from_state(&state)?;
    let batch = KnowledgeCompactionService::new(pool, &state.config.knowledge)
        .start_manual(user.id)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(batch_response(batch))))
}

async fn retry(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
) -> Result<(StatusCode, Json<BatchResponse>), CompactionApiError> {
    let pool = pool_from_state(&state)?;
    let batch = KnowledgeCompactionService::new(pool, &state.config.knowledge)
        .retry(user.id)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(batch_response(batch))))
}

async fn cancel(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<(StatusCode, Json<BatchResponse>), CompactionApiError> {
    let pool = pool_from_state(&state)?;
    let (batch, run_id) = KnowledgeCompactionService::new(pool, &state.config.knowledge)
        .cancel()
        .await?;
    if let Some(handle) = state.run_streams.get(run_id) {
        handle.cancel();
    }
    Ok((StatusCode::ACCEPTED, Json(batch_response(batch))))
}
