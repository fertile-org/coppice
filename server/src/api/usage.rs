//! Token usage for one run, for a ticket's runs, and for the workspace.

use crate::api::auth::{pool_from_state, AuthUser};
use crate::middleware::admin::AdminUser;
use crate::services::chat_service::{ChatError, ChatService};
use crate::services::usage_service::{RunUsageDetail, TicketUsage, UsageGroup, UsageService};
use crate::services::usage_summary::{self, GroupBy, SummaryQuery, UsageSummary};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/agent-runs/{run_id}/usage", get(run_usage))
        .route("/api/tickets/{ticket_id}/usage", get(ticket_usage))
        .route("/api/usage/summary", get(summary))
        .route("/api/chat/sessions/{session_id}/usage", get(chat_session_usage))
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryParams {
    from: Option<String>,
    to: Option<String>,
    tz: Option<String>,
    group_by: Option<String>,
    board: Option<Uuid>,
    connector: Option<String>,
}

fn parse_day(value: Option<String>, message: &'static str) -> Result<Option<String>, UsageApiError> {
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let format = time::format_description::parse("[year]-[month]-[day]")
        .map_err(|_| UsageApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?;
    time::Date::parse(&value, &format)
        .map(|_| Some(value))
        .map_err(|_| UsageApiError(StatusCode::BAD_REQUEST, message))
}

/// Settings → Usage: workspace token usage, grouped, per day, with coverage.
/// Admins only, like the other Settings tabs.
async fn summary(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Query(params): Query<SummaryParams>,
) -> Result<Json<UsageSummary>, UsageApiError> {
    let pool = pool_from_state(&state)?;
    let from = parse_day(params.from, "from must be a date (YYYY-MM-DD)")?;
    let to = parse_day(params.to, "to must be a date (YYYY-MM-DD)")?;
    if let (Some(from), Some(to)) = (&from, &to) {
        if from > to {
            return Err(UsageApiError(StatusCode::BAD_REQUEST, "from must not be after to"));
        }
    }
    let group_by = match params.group_by.as_deref() {
        None | Some("") => GroupBy::Model,
        Some(value) => GroupBy::parse(value).ok_or(UsageApiError(
            StatusCode::BAD_REQUEST,
            "groupBy must be model, agent, board, connector or jobType",
        ))?,
    };
    let tz = params.tz.filter(|tz| !tz.is_empty()).unwrap_or_else(|| "UTC".into());
    if !usage_summary::timezone_exists(pool, &tz).await? {
        return Err(UsageApiError(StatusCode::BAD_REQUEST, "unknown time zone"));
    }
    let query = SummaryQuery {
        from,
        to,
        tz,
        group_by,
        board: params.board,
        connector: params.connector.filter(|c| !c.is_empty()),
    };
    Ok(Json(usage_summary::usage_summary(pool, &query).await?))
}

/// A chat session's total, for its owner (anyone else gets 404, as for the
/// session itself).
async fn chat_session_usage(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<UsageGroup>, UsageApiError> {
    let pool = pool_from_state(&state)?;
    match ChatService::new(pool).get_session(session_id, user.id).await {
        Ok(_) => {}
        Err(ChatError::NotFound) => {
            return Err(UsageApiError(StatusCode::NOT_FOUND, "chat session not found"))
        }
        Err(err) => {
            tracing::error!(error = %err, "chat session lookup failed");
            return Err(UsageApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal error"));
        }
    }
    Ok(Json(UsageService::new(pool).chat_session_usage(session_id).await?))
}
