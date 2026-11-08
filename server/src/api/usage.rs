//! Token usage for one run and for a ticket's runs.

use crate::api::auth::{pool_from_state, AuthUser};
use crate::services::usage_service::{RunUsageDetail, TicketUsage, UsageService};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/agent-runs/{run_id}/usage", get(run_usage))
        .route("/api/tickets/{ticket_id}/usage", get(ticket_usage))
}

struct UsageApiError(StatusCode, &'static str);

impl IntoResponse for UsageApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "message": self.1 }))).into_response()
    }
}

impl From<StatusCode> for UsageApiError {
    fn from(status: StatusCode) -> Self {
        UsageApiError(status, "internal error")
    }
}

impl From<sqlx::Error> for UsageApiError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(error = %err, "usage query failed");
        UsageApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}

async fn run_usage(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RunUsageDetail>, UsageApiError> {
    let service = UsageService::new(pool_from_state(&state)?);
    if !service.run_exists(run_id).await? {
        return Err(UsageApiError(StatusCode::NOT_FOUND, "agent run not found"));
    }
    Ok(Json(service.run_usage(run_id).await?))
}

async fn ticket_usage(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketUsage>, UsageApiError> {
    let service = UsageService::new(pool_from_state(&state)?);
    if !service.ticket_exists(ticket_id).await? {
        return Err(UsageApiError(StatusCode::NOT_FOUND, "ticket not found"));
    }
    Ok(Json(service.ticket_usage(ticket_id).await?))
}
