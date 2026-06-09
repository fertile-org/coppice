use crate::api::agent_runs::{
    runs_list_response, single_run_response, RunsListResponse, SingleRunResponse,
};
use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::substatus::{Substatus, SubstatusDisplay, TicketStatus};
use crate::domain::ticket::{
    priority_from_str, status_from_str, substatus_from_str, TicketPriority,
};
use crate::domain::agent_health::AgentHealthStatus;
use crate::services::run_service::{RunError, RunService};
use crate::services::ticket_service::{TicketError, TicketFilters, TicketService, TicketWithDisplay};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/projects/{project_id}/tickets",
            get(list_tickets).post(create_ticket),
        )
        .route(
            "/api/tickets/{ticket_id}",
            get(get_ticket).patch(update_ticket),
        )
        .route("/api/tickets/{ticket_id}/status", axum::routing::patch(update_status))
        .route("/api/tickets/{ticket_id}/assign", post(assign_agent))
        .route("/api/tickets/{ticket_id}/run-agent", post(run_agent))
        .route("/api/tickets/{ticket_id}/runs", get(list_runs))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TicketResponse {
    id: Uuid,
    project_id: Uuid,
    repo_id: Option<Uuid>,
    title: String,
    description: String,
    status: String,
    substatus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    substatus_metadata: Option<Value>,
    priority: Option<String>,
    assignee_agent_id: Option<Uuid>,
    owner_user_id: Option<Uuid>,
    branch_name: Option<String>,
    created_by: String,
    created_by_id: Option<Uuid>,
    created_at: String,
    updated_at: String,
    last_activity_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    substatus_display: Option<SubstatusDisplay>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTicketBody {
    title: String,
    #[serde(default)]
    description: String,
    repo_id: Option<Uuid>,
    priority: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTicketBody {
    title: Option<String>,
    description: Option<String>,
    repo_id: Option<Uuid>,
    priority: Option<String>,
    branch_name: Option<String>,
    owner_user_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStatusBody {
    status: String,
    substatus: Option<String>,
    substatus_metadata: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignAgentBody {
    agent_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListTicketsQuery {
    status: Option<String>,
    assignee_agent_id: Option<Uuid>,
}

fn ticket_to_response(item: TicketWithDisplay) -> TicketResponse {
    let ticket = item.ticket;
    TicketResponse {
        id: ticket.id,
        project_id: ticket.project_id,
        repo_id: ticket.repo_id,
        title: ticket.title,
        description: ticket.description,
        status: ticket.status.to_string_snake(),
        substatus: ticket.substatus.map(|s| s.to_string_snake()),
        substatus_metadata: ticket.substatus_metadata,
        priority: ticket.priority.map(|p| p.to_string_snake()),
        assignee_agent_id: ticket.assignee_agent_id,
        owner_user_id: ticket.owner_user_id,
        branch_name: ticket.branch_name,
        created_by: ticket.created_by,
        created_by_id: ticket.created_by_id,
        created_at: ticket.created_at.format(&Rfc3339).unwrap_or_default(),
        updated_at: ticket.updated_at.format(&Rfc3339).unwrap_or_default(),
        last_activity_at: item.last_activity_at.format(&Rfc3339).unwrap_or_default(),
        substatus_display: item.substatus_display,
    }
}

trait StatusSerde {
    fn to_string_snake(&self) -> String;
}

impl StatusSerde for TicketStatus {
    fn to_string_snake(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }
}

impl StatusSerde for Substatus {
    fn to_string_snake(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }
}

impl StatusSerde for TicketPriority {
    fn to_string_snake(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }
}

fn map_run_error(err: RunError) -> StatusCode {
    match err {
        RunError::ActiveRunExists => StatusCode::CONFLICT,
        RunError::NotFound => StatusCode::NOT_FOUND,
        RunError::Validation(_) => StatusCode::BAD_REQUEST,
        RunError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiMessageResponse {
    message: String,
}

enum RunAgentError {
    Status(StatusCode),
    Message(StatusCode, String),
}

impl IntoResponse for RunAgentError {
    fn into_response(self) -> Response {
        match self {
            RunAgentError::Status(code) => code.into_response(),
            RunAgentError::Message(code, message) => {
                (code, Json(ApiMessageResponse { message })).into_response()
            }
        }
    }
}

fn map_run_error_response(err: RunError) -> RunAgentError {
    RunAgentError::Status(map_run_error(err))
}

fn map_ticket_error_response(err: TicketError) -> RunAgentError {
    RunAgentError::Status(map_error(err))
}

fn map_error(err: TicketError) -> StatusCode {
    match err {
        TicketError::TicketNotFound | TicketError::ProjectNotFound => StatusCode::NOT_FOUND,
        TicketError::InvalidStatus
        | TicketError::InvalidSubstatus
        | TicketError::InvalidPriority
        | TicketError::Validation(_) => StatusCode::BAD_REQUEST,
        TicketError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn parse_status(status: &str) -> Result<TicketStatus, TicketError> {
    status_from_str(status).ok_or(TicketError::InvalidStatus)
}

fn parse_substatus(substatus: &str) -> Result<Substatus, TicketError> {
    substatus_from_str(substatus).ok_or(TicketError::InvalidSubstatus)
}

fn parse_priority(priority: &str) -> Result<TicketPriority, TicketError> {
    priority_from_str(priority).ok_or(TicketError::InvalidPriority)
}

fn build_filters(query: ListTicketsQuery) -> Result<TicketFilters, TicketError> {
    let status = match query.status {
        Some(s) => Some(parse_status(&s)?),
        None => None,
    };
    Ok(TicketFilters {
        status,
        assignee_agent_id: query.assignee_agent_id,
    })
}

async fn list_tickets(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(project_id): Path<Uuid>,
    Query(query): Query<ListTicketsQuery>,
) -> Result<Json<Vec<TicketResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketService::new(pool);
    let filters = build_filters(query).map_err(map_error)?;
    let tickets = service
        .list_by_project(project_id, &filters)
        .await
        .map_err(map_error)?;
    Ok(Json(
        tickets.into_iter().map(ticket_to_response).collect(),
    ))
}

async fn create_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(body): Json<CreateTicketBody>,
) -> Result<(StatusCode, Json<TicketResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketService::new(pool);
    let priority = match body.priority.as_deref() {
        Some(p) => Some(parse_priority(p).map_err(map_error)?),
        None => None,
    };
    let ticket = service
        .create(
            project_id,
            &body.title,
            &body.description,
            body.repo_id,
            priority,
            &user.email,
            user.id,
        )
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(ticket_to_response(ticket))))
}

async fn get_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketService::new(pool);
    let ticket = service.get(ticket_id).await.map_err(map_error)?;
    Ok(Json(ticket_to_response(ticket)))
}

async fn update_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<UpdateTicketBody>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketService::new(pool);
    let priority = match body.priority.as_deref() {
        Some(p) => Some(Some(parse_priority(p).map_err(map_error)?)),
        None => None,
    };
    let ticket = service
        .update_fields(
            ticket_id,
            body.title.as_deref(),
            body.description.as_deref(),
            body.repo_id.map(Some),
            priority,
            body.branch_name.as_deref().map(Some),
            body.owner_user_id.map(Some),
        )
        .await
        .map_err(map_error)?;
    Ok(Json(ticket_to_response(ticket)))
}

async fn update_status(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<UpdateStatusBody>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketService::new(pool);
    let status = parse_status(&body.status).map_err(map_error)?;
    let substatus = match body.substatus {
        Some(s) => Some(Some(parse_substatus(&s).map_err(map_error)?)),
        None => None,
    };
    let substatus_metadata = body.substatus_metadata.map(Some);
    let ticket = service
        .update_status(ticket_id, status, substatus, substatus_metadata)
        .await
        .map_err(map_error)?;
    Ok(Json(ticket_to_response(ticket)))
}

async fn assign_agent(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<AssignAgentBody>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketService::new(pool);
    let ticket = service
        .assign_agent(ticket_id, body.agent_id)
        .await
        .map_err(map_error)?;
    Ok(Json(ticket_to_response(ticket)))
}

async fn run_agent(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<(StatusCode, Json<SingleRunResponse>), RunAgentError> {
    let pool = pool_from_state(&state).map_err(RunAgentError::Status)?;
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .map_err(map_ticket_error_response)?;

    if let Some(agent_id) = ticket.ticket.assignee_agent_id {
        let health = state.agent_health.get(agent_id);
        if health.status == AgentHealthStatus::MissingConfig {
            return Err(RunAgentError::Message(
                StatusCode::BAD_REQUEST,
                health
                    .detail
                    .unwrap_or_else(|| "Agent connector is not configured".into()),
            ));
        }
    }

    let service = RunService::new(pool);
    let run = service
        .start_run(ticket_id)
        .await
        .map_err(map_run_error_response)?;
    let connector = service
        .agent_connector_for_run(run.agent_id)
        .await
        .map_err(map_run_error_response)?;
    Ok((StatusCode::CREATED, Json(single_run_response(run, connector))))
}

async fn list_runs(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<RunsListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RunService::new(pool);
    let runs = service
        .list_for_ticket(ticket_id)
        .await
        .map_err(map_run_error)?;
    Ok(Json(runs_list_response(runs)))
}
