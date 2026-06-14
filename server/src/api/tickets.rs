use crate::api::agent_runs::{
    runs_list_response, single_run_response, RunsListResponse, SingleRunResponse,
};
use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::substatus::{Substatus, SubstatusDisplay, TicketStatus};
use crate::domain::ticket::{
    priority_from_str, status_from_str, substatus_from_str, TicketPriority,
};
use crate::domain::workflow::{PendingRecommendation, PendingSplitRecommendation};
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::events::bus::AppEvent;
use crate::services::comment_service::CommentService;
use crate::services::workflow_service::WorkflowService;
use crate::domain::agent_health::AgentHealthStatus;
use crate::services::run_service::{RunError, RunService};
use crate::services::split_service::{SplitError, SplitService};
use crate::services::ticket_git_service::{TicketGitError, TicketGitInfo, TicketGitService};
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
        .route(
            "/api/tickets/{ticket_id}/final-approve",
            post(final_approve),
        )
        .route(
            "/api/tickets/{ticket_id}/git-info",
            get(ticket_git_info),
        )
        .route(
            "/api/tickets/{ticket_id}/merge-branch",
            post(merge_ticket_branch),
        )
        .route(
            "/api/tickets/{ticket_id}/remove-worktree",
            post(remove_ticket_worktree),
        )
        .route(
            "/api/tickets/{ticket_id}/resolve-blocker",
            post(resolve_blocker),
        )
        .route(
            "/api/tickets/{ticket_id}/approve-splits",
            post(approve_splits),
        )
        .route(
            "/api/tickets/{ticket_id}/dismiss-splits",
            post(dismiss_splits),
        )
        .route("/api/tickets/{ticket_id}/children", get(list_children))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TicketResponse {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pending_assign_recommendation: Option<PendingRecommendation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_ticket_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pending_split_recommendation: Option<PendingSplitRecommendation>,
    clarification_round: i32,
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

pub(crate) fn ticket_to_response(item: TicketWithDisplay) -> TicketResponse {
    let ticket = item.ticket;
    let pending_assign_recommendation = ticket
        .pending_assign_recommendation
        .as_ref()
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    let pending_split_recommendation = ticket
        .pending_split_recommendation
        .as_ref()
        .and_then(|value| serde_json::from_value(value.clone()).ok());
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
        pending_assign_recommendation,
        parent_ticket_id: ticket.parent_ticket_id,
        pending_split_recommendation,
        clarification_round: ticket.clarification_round,
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

pub(crate) fn map_error(err: TicketError) -> StatusCode {
    match err {
        TicketError::TicketNotFound | TicketError::ProjectNotFound => StatusCode::NOT_FOUND,
        TicketError::InvalidStatus
        | TicketError::InvalidSubstatus
        | TicketError::InvalidPriority
        | TicketError::Validation(_) => StatusCode::BAD_REQUEST,
        TicketError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn map_split_error(err: SplitError) -> StatusCode {
    match err {
        SplitError::Validation(_) => StatusCode::BAD_REQUEST,
        SplitError::Ticket(e) => map_error(e),
        SplitError::Agent(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn map_ticket_git_error_response(err: TicketGitError) -> TicketGitApiError {
    let (status, message) = match err {
        TicketGitError::TicketNotFound => (StatusCode::NOT_FOUND, "Ticket not found.".into()),
        TicketGitError::NoRepo => (
            StatusCode::BAD_REQUEST,
            "Ticket has no linked repository.".into(),
        ),
        TicketGitError::RepoNotFound => (StatusCode::NOT_FOUND, "Repository not found.".into()),
        TicketGitError::RepoNotReady => (
            StatusCode::BAD_REQUEST,
            "Repository is not ready. Verify the path in Settings → Repositories.".into(),
        ),
        TicketGitError::TicketBranchMissing(branch) => (
            StatusCode::BAD_REQUEST,
            format!("Ticket branch `{branch}` was not found in the repository."),
        ),
        TicketGitError::WorktreeAlreadyRemoved => (
            StatusCode::BAD_REQUEST,
            "Worktree has already been removed.".into(),
        ),
        TicketGitError::InvalidBranchName => (
            StatusCode::BAD_REQUEST,
            "Invalid base branch name.".into(),
        ),
        TicketGitError::Git(msg) => (StatusCode::BAD_REQUEST, msg),
        TicketGitError::Ticket(e) => {
            let message = ticket_error_message(&e);
            (map_error(e), message)
        }
        TicketGitError::Worktree(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        TicketGitError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        TicketGitError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "An internal error occurred.".into(),
        ),
    };
    TicketGitApiError::Message(status, message)
}

fn ticket_error_message(err: &TicketError) -> String {
    match err {
        TicketError::Validation(message) => message.clone(),
        _ => err.to_string(),
    }
}

enum TicketGitApiError {
    Message(StatusCode, String),
}

impl IntoResponse for TicketGitApiError {
    fn into_response(self) -> Response {
        match self {
            TicketGitApiError::Message(code, message) => {
                (code, Json(ApiMessageResponse { message })).into_response()
            }
        }
    }
}

fn ticket_git_service<'a>(state: &'a AppState, pool: &'a sqlx::PgPool) -> TicketGitService<'a> {
    TicketGitService::new(pool, state.config.agent.worktrees_path.clone().into())
}

async fn create_git_action_comment(
    pool: &sqlx::PgPool,
    state: &AppState,
    ticket_id: Uuid,
    user_id: Uuid,
    body: &str,
) -> Result<(), StatusCode> {
    let comment = CommentService::new(pool)
        .create(
            ticket_id,
            AuthorType::Human,
            Some(user_id),
            body,
            CommentIntent::SystemEvent,
            &[],
            &[],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state.event_bus.publish(AppEvent::CommentCreated {
        comment_id: comment.id,
        ticket_id,
        author_type: "human".into(),
    });
    Ok(())
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
    let ticket_svc = TicketService::new(pool);
    ticket_svc
        .assign_agent(ticket_id, body.agent_id)
        .await
        .map_err(map_error)?;
    let mut ticket = ticket_svc
        .clear_pending_recommendation(ticket_id)
        .await
        .map_err(map_error)?;

    if state.config.workflow.auto_start_runs
        && ticket.ticket.assignee_agent_id.is_some()
        && ticket.ticket.repo_id.is_some()
        && RunService::new(pool).start_run(ticket_id).await.is_ok()
    {
        ticket = ticket_svc.get(ticket_id).await.map_err(map_error)?;
        crate::events::publish_ticket_updated(&state.event_bus, &ticket);
    }

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
    if let Ok(updated) = TicketService::new(pool).get(ticket_id).await {
        if updated.ticket.status != ticket.ticket.status {
            crate::events::publish_ticket_updated(&state.event_bus, &updated);
        }
    }
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

async fn final_approve(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let ticket_svc = TicketService::new(pool);
    let ticket = ticket_svc.get(ticket_id).await.map_err(map_error)?;
    let next = WorkflowService::final_approve(ticket.ticket.status)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let updated = ticket_svc
        .update_status(ticket_id, next, Some(None), Some(None))
        .await
        .map_err(map_error)?;

    create_git_action_comment(
        pool,
        &state,
        ticket_id,
        user.id,
        "Final approval: ticket moved to **Done**.",
    )
    .await?;

    crate::events::publish_ticket_updated(&state.event_bus, &updated);
    Ok(Json(ticket_to_response(updated)))
}

async fn ticket_git_info(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketGitInfo>, TicketGitApiError> {
    let pool = pool_from_state(&state).map_err(|code| {
        TicketGitApiError::Message(code, "Database unavailable.".into())
    })?;
    let info = ticket_git_service(&state, pool)
        .git_info(ticket_id)
        .await
        .map_err(map_ticket_git_error_response)?;
    Ok(Json(info))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeBranchBody {
    base_branch: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeBranchResponse {
    merge: crate::services::ticket_git_service::MergeBranchResult,
}

async fn merge_ticket_branch(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<MergeBranchBody>,
) -> Result<Json<MergeBranchResponse>, TicketGitApiError> {
    let pool = pool_from_state(&state).map_err(|code| {
        TicketGitApiError::Message(code, "Database unavailable.".into())
    })?;
    let merge = ticket_git_service(&state, pool)
        .merge_ticket_branch(ticket_id, body.base_branch.trim())
        .await
        .map_err(map_ticket_git_error_response)?;

    let short_sha = merge.head_sha.get(..7).unwrap_or(&merge.head_sha);
    let comment_body = format!(
        "**Merge:** {} (`{short_sha}` on `{}`)",
        merge.message, merge.base_branch
    );
    create_git_action_comment(pool, &state, ticket_id, user.id, &comment_body)
        .await
        .map_err(|code| TicketGitApiError::Message(code, "Unable to record merge comment.".into()))?;

    Ok(Json(MergeBranchResponse { merge }))
}

async fn remove_ticket_worktree(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketGitInfo>, TicketGitApiError> {
    let pool = pool_from_state(&state).map_err(|code| {
        TicketGitApiError::Message(code, "Database unavailable.".into())
    })?;
    let svc = ticket_git_service(&state, pool);
    let info_before = svc
        .git_info(ticket_id)
        .await
        .map_err(map_ticket_git_error_response)?;
    svc.remove_worktree(ticket_id)
        .await
        .map_err(map_ticket_git_error_response)?;

    let comment_body = format!(
        "**Worktree removed:** `{}`",
        info_before.worktree_path
    );
    create_git_action_comment(pool, &state, ticket_id, user.id, &comment_body)
        .await
        .map_err(|code| {
            TicketGitApiError::Message(code, "Unable to record worktree removal comment.".into())
        })?;

    let info = svc
        .git_info(ticket_id)
        .await
        .map_err(map_ticket_git_error_response)?;
    Ok(Json(info))
}

async fn approve_splits(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<TicketResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let children = SplitService::new(pool, &state.config.workflow)
        .approve_splits(ticket_id)
        .await
        .map_err(map_split_error)?;

    let ticket_svc = TicketService::new(pool);
    let mut responses = Vec::with_capacity(children.len());
    for child in children {
        let enriched = ticket_svc.get(child.id).await.map_err(map_error)?;
        responses.push(ticket_to_response(enriched));
    }
    Ok(Json(responses))
}

async fn dismiss_splits(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let parent = SplitService::new(pool, &state.config.workflow)
        .dismiss_splits(ticket_id)
        .await
        .map_err(map_split_error)?;
    Ok(Json(ticket_to_response(parent)))
}

async fn list_children(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<TicketResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let children = SplitService::new(pool, &state.config.workflow)
        .list_children(ticket_id)
        .await
        .map_err(map_split_error)?;
    Ok(Json(
        children.into_iter().map(ticket_to_response).collect(),
    ))
}

async fn resolve_blocker(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let ticket_svc = TicketService::new(pool);
    let ticket = ticket_svc.get(ticket_id).await.map_err(map_error)?;

    if matches!(
        ticket.ticket.substatus,
        Some(
            Substatus::BlockedByMissingCapability
                | Substatus::BlockedByMissingSecret
                | Substatus::BlockedByPermission
        )
    ) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let updated = ticket_svc
        .update_status(
            ticket_id,
            ticket.ticket.status,
            Some(None),
            Some(None),
        )
        .await
        .map_err(map_error)?;
    Ok(Json(ticket_to_response(updated)))
}
