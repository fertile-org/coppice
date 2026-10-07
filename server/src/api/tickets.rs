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
use crate::services::agent_health::missing_connector_detail;
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::run_service::{RunError, RunService};
use crate::services::split_service::{SplitError, SplitService};
use crate::services::human_review_service::{short_commit_sha, HumanReviewService};
use crate::services::conflict_resolve::{
    self, ConflictResolveError, GitConflictHttp,
};
use crate::services::ticket_git_service::{
    GitConflictKind, TicketGitError, TicketGitInfo, TicketGitService,
};
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
            "/api/boards/{board_id}/tickets",
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
            "/api/tickets/{ticket_id}/rebase-branch",
            post(rebase_ticket_branch),
        )
        .route(
            "/api/tickets/{ticket_id}/resolve-conflict",
            post(resolve_ticket_conflict),
        )
        .route(
            "/api/tickets/{ticket_id}/remove-worktree",
            post(remove_ticket_worktree),
        )
        .route(
            "/api/tickets/{ticket_id}/push-branch",
            post(push_ticket_branch),
        )
        .route(
            "/api/tickets/{ticket_id}/create-pr",
            post(create_ticket_pr),
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
        .route("/api/tickets/{ticket_id}/archive", post(archive_ticket))
        .route("/api/tickets/{ticket_id}/unarchive", post(unarchive_ticket))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TicketResponse {
    id: Uuid,
    board_id: Uuid,
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
    has_active_run: bool,
    archived_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    human_review: Option<HumanReviewResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HumanReviewResponse {
    head_sha: String,
    short_sha: String,
    stale: bool,
    comment_ids: Vec<Uuid>,
    run_ids: Vec<Uuid>,
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
    #[serde(default)]
    include_archived: bool,
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
        board_id: ticket.board_id,
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
        has_active_run: item.has_active_run,
        archived_at: ticket
            .archived_at
            .map(|ts| ts.format(&Rfc3339).unwrap_or_default()),
        human_review: item.human_review.map(|review| {
            let short_sha = short_commit_sha(&review.head_sha).to_string();
            HumanReviewResponse {
                head_sha: review.head_sha,
                short_sha,
                stale: review.stale,
                comment_ids: review.comment_ids,
                run_ids: review.run_ids,
            }
        }),
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
    match &err {
        RunError::ActiveRunExists => RunAgentError::Message(
            StatusCode::CONFLICT,
            "An active run already exists for this ticket and agent.".into(),
        ),
        RunError::NotFound => RunAgentError::Status(StatusCode::NOT_FOUND),
        RunError::Validation(msg) => {
            RunAgentError::Message(StatusCode::BAD_REQUEST, msg.clone())
        }
        RunError::Database(db_err) => {
            tracing::error!(error = %db_err, "run-agent database error");
            RunAgentError::Message(
                StatusCode::INTERNAL_SERVER_ERROR,
                "An internal error occurred.".into(),
            )
        }
    }
}

fn map_ticket_error_response(err: TicketError) -> RunAgentError {
    match err {
        TicketError::Archived => RunAgentError::Message(
            StatusCode::CONFLICT,
            "ticket is archived; unarchive before continuing".into(),
        ),
        TicketError::ActiveRunExists => RunAgentError::Message(
            StatusCode::CONFLICT,
            "An active run already exists for this ticket.".into(),
        ),
        other => RunAgentError::Status(map_error(other)),
    }
}

pub(crate) fn map_error(err: TicketError) -> StatusCode {
    match err {
        TicketError::TicketNotFound | TicketError::BoardNotFound => StatusCode::NOT_FOUND,
        TicketError::InvalidStatus
        | TicketError::InvalidSubstatus
        | TicketError::InvalidPriority
        | TicketError::Validation(_) => StatusCode::BAD_REQUEST,
        TicketError::ActiveRunExists | TicketError::Archived => StatusCode::CONFLICT,
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
        TicketGitError::WorktreeMissing => (
            StatusCode::BAD_REQUEST,
            "Ticket worktree does not exist — rebase requires an existing worktree.".into(),
        ),
        TicketGitError::Conflict {
            kind,
            base_branch,
            paths,
            detail: _,
        } => (
            StatusCode::BAD_REQUEST,
            crate::copy::conflict::conflict_message(conflict_action(kind), &base_branch, &paths),
        ),
        TicketGitError::InvalidBranchName => (
            StatusCode::BAD_REQUEST,
            "Invalid base branch name.".into(),
        ),
        TicketGitError::PushDisabled => (
            StatusCode::FORBIDDEN,
            "Git push is disabled. Set git.push_enabled = true in server config.".into(),
        ),
        TicketGitError::NoRemoteUrl => (
            StatusCode::BAD_REQUEST,
            "Repository has no remote_url. Set one in Settings → Repositories.".into(),
        ),
        TicketGitError::NoForgeToken => (
            StatusCode::BAD_REQUEST,
            "Repository has no forge token. Set one in Settings → Repositories.".into(),
        ),
        TicketGitError::NotGitHub => (
            StatusCode::BAD_REQUEST,
            "Create PR via API requires a GitHub remote_url.".into(),
        ),
        TicketGitError::ReviewShaMismatch => (
            StatusCode::CONFLICT,
            "Not merged. This isn't the commit you reviewed.".into(),
        ),
        TicketGitError::ReviewAcceptanceMissing => (
            StatusCode::CONFLICT,
            "Not merged. Accept again so Coppice knows which commit you reviewed.".into(),
        ),
        TicketGitError::Git(msg) => (StatusCode::BAD_REQUEST, msg),
        TicketGitError::GitHubApi(msg) => (StatusCode::BAD_REQUEST, format!("GitHub API: {msg}")),
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
        TicketGitError::Secret(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to decrypt forge token.".into(),
        ),
    };
    TicketGitApiError::Message(status, message)
}

fn ticket_error_message(err: &TicketError) -> String {
    match err {
        TicketError::Validation(message) => message.clone(),
        TicketError::Archived => "ticket is archived; unarchive before continuing".into(),
        TicketError::ActiveRunExists => "An active run already exists for this ticket.".into(),
        _ => err.to_string(),
    }
}

fn conflict_action(kind: GitConflictKind) -> &'static str {
    match kind {
        GitConflictKind::Merge => crate::copy::conflict::MERGE_ACTION,
        GitConflictKind::Rebase => crate::copy::conflict::REBASE_ACTION,
    }
}

fn connector_is_ready(state: &AppState, connector: &str) -> bool {
    if connector == coppice_connectors::MOCK {
        return true;
    }
    matches!(
        state
            .connector_probes
            .get(connector)
            .map(|cached| cached.readiness),
        Some(coppice_connectors::sign_in::Readiness::Ready)
    )
}

async fn git_failure_response(
    state: &AppState,
    pool: &sqlx::PgPool,
    ticket_id: Uuid,
    err: TicketGitError,
) -> TicketGitApiError {
    match err {
        TicketGitError::Conflict {
            kind,
            base_branch,
            paths,
            detail,
        } => {
            tracing::info!(%detail, ?paths, "git conflict aborted");
            let ready = |connector: &str| connector_is_ready(state, connector);
            let body = conflict_resolve::build_git_conflict_response(
                pool,
                ticket_id,
                kind,
                base_branch,
                paths,
                &ready,
            )
            .await;
            TicketGitApiError::Conflict(Box::new(body))
        }
        other => map_ticket_git_error_response(other),
    }
}

enum TicketGitApiError {
    Message(StatusCode, String),
    // Boxed so the error type stays small enough for `Result`.
    Conflict(Box<GitConflictHttp>),
}

impl IntoResponse for TicketGitApiError {
    fn into_response(self) -> Response {
        match self {
            TicketGitApiError::Message(code, message) => {
                (code, Json(ApiMessageResponse { message })).into_response()
            }
            TicketGitApiError::Conflict(body) => {
                (StatusCode::BAD_REQUEST, Json(body)).into_response()
            }
        }
    }
}

fn ticket_git_service<'a>(state: &'a AppState, pool: &'a sqlx::PgPool) -> TicketGitService<'a> {
    TicketGitService::with_forge(
        pool,
        state.config.agent.worktrees_path.clone().into(),
        state.config.git.push_enabled,
        &state.secret_store,
        crate::services::worktree_service::GitAuthor {
            name: state.config.git.author_name.clone(),
            email: state.config.git.author_email.clone(),
        },
    )
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
        include_archived: query.include_archived,
    })
}

async fn list_tickets(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(board_id): Path<Uuid>,
    Query(query): Query<ListTicketsQuery>,
) -> Result<Json<Vec<TicketResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketService::new(pool);
    let filters = build_filters(query).map_err(map_error)?;
    let tickets = service
        .list_by_board(board_id, &filters)
        .await
        .map_err(map_error)?;
    Ok(Json(
        tickets.into_iter().map(ticket_to_response).collect(),
    ))
}

async fn create_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(board_id): Path<Uuid>,
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
            board_id,
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

async fn archive_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, RunAgentError> {
    let pool = pool_from_state(&state).map_err(RunAgentError::Status)?;
    let ticket = TicketService::new(pool)
        .archive(ticket_id)
        .await
        .map_err(map_ticket_error_response)?;
    Ok(Json(ticket_to_response(ticket)))
}

async fn unarchive_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let ticket = TicketService::new(pool)
        .unarchive(ticket_id)
        .await
        .map_err(map_error)?;
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
    TicketService::ensure_not_archived(&ticket.ticket).map_err(map_ticket_error_response)?;

    if let Some(agent_id) = ticket.ticket.assignee_agent_id {
        let agent = AgentService::new(pool).get(agent_id).await.map_err(|err| match err {
            AgentError::AgentNotFound => RunAgentError::Status(StatusCode::NOT_FOUND),
            other => {
                tracing::warn!(error = %other, "load assignee for run failed");
                RunAgentError::Status(StatusCode::INTERNAL_SERVER_ERROR)
            }
        })?;
        if !state.connectors.registry().has(&agent.connector) {
            return Err(RunAgentError::Message(
                StatusCode::BAD_REQUEST,
                missing_connector_detail(&agent.connector),
            ));
        }
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

enum FinalApproveError {
    Status(StatusCode),
    Message(StatusCode, String),
}

impl IntoResponse for FinalApproveError {
    fn into_response(self) -> Response {
        match self {
            FinalApproveError::Status(code) => code.into_response(),
            FinalApproveError::Message(code, message) => {
                (code, Json(ApiMessageResponse { message })).into_response()
            }
        }
    }
}

impl From<TicketGitApiError> for FinalApproveError {
    fn from(err: TicketGitApiError) -> Self {
        match err {
            TicketGitApiError::Message(code, message) => FinalApproveError::Message(code, message),
            TicketGitApiError::Conflict(body) => {
                FinalApproveError::Message(StatusCode::BAD_REQUEST, body.message)
            }
        }
    }
}

async fn final_approve(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, FinalApproveError> {
    let pool = pool_from_state(&state).map_err(FinalApproveError::Status)?;
    let ticket_svc = TicketService::new(pool);
    let ticket = ticket_svc
        .get(ticket_id)
        .await
        .map_err(map_error)
        .map_err(FinalApproveError::Status)?;
    let next = WorkflowService::final_approve(ticket.ticket.status)
        .map_err(|_| FinalApproveError::Status(StatusCode::BAD_REQUEST))?;

    match ticket_git_service(&state, pool)
        .reviewed_head_sha(ticket_id)
        .await
    {
        Ok(Some(head_sha)) => {
            HumanReviewService::new(pool)
                .record_acceptance(ticket_id, user.id, &head_sha)
                .await
                .map_err(|_| FinalApproveError::Status(StatusCode::INTERNAL_SERVER_ERROR))?;
        }
        Ok(None) => {}
        Err(err) => return Err(map_ticket_git_error_response(err).into()),
    }

    let updated = ticket_svc
        .update_status(ticket_id, next, Some(None), Some(None))
        .await
        .map_err(map_error)
        .map_err(FinalApproveError::Status)?;

    create_git_action_comment(
        pool,
        &state,
        ticket_id,
        user.id,
        "Accepted: ticket moved to **Done**.",
    )
    .await
    .map_err(FinalApproveError::Status)?;

    crate::events::publish_ticket_updated(&state.event_bus, &updated);
    Ok(Json(ticket_to_response(updated)))
}

async fn publish_ticket_state(state: &AppState, pool: &sqlx::PgPool, ticket_id: Uuid) {
    if let Ok(ticket) = TicketService::new(pool).get(ticket_id).await {
        crate::events::publish_ticket_updated(&state.event_bus, &ticket);
    }
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
        .await;
    publish_ticket_state(&state, pool, ticket_id).await;
    let merge = match merge {
        Ok(merge) => merge,
        Err(err) => return Err(git_failure_response(&state, pool, ticket_id, err).await),
    };

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RebaseBranchBody {
    #[serde(default)]
    base_branch: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RebaseBranchResponse {
    rebase: crate::services::ticket_git_service::RebaseBranchResult,
}

async fn rebase_ticket_branch(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<RebaseBranchBody>,
) -> Result<Json<RebaseBranchResponse>, TicketGitApiError> {
    let pool = pool_from_state(&state).map_err(|code| {
        TicketGitApiError::Message(code, "Database unavailable.".into())
    })?;
    let base = body
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let rebase = ticket_git_service(&state, pool)
        .rebase_ticket_branch(ticket_id, base)
        .await;
    publish_ticket_state(&state, pool, ticket_id).await;
    let rebase = match rebase {
        Ok(rebase) => rebase,
        Err(err) => return Err(git_failure_response(&state, pool, ticket_id, err).await),
    };

    let short_sha = rebase.head_sha.get(..7).unwrap_or(&rebase.head_sha);
    let comment_body = format!(
        "**Rebase:** {} (`{short_sha}` onto `{}`)",
        rebase.message, rebase.onto_ref
    );
    create_git_action_comment(pool, &state, ticket_id, user.id, &comment_body)
        .await
        .map_err(|code| {
            TicketGitApiError::Message(code, "Unable to record rebase comment.".into())
        })?;

    Ok(Json(RebaseBranchResponse { rebase }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveConflictBody {
    base_branch: String,
    #[serde(default)]
    files: Vec<String>,
}

enum ResolveConflictApiError {
    Status(StatusCode),
    Message(StatusCode, String),
}

impl IntoResponse for ResolveConflictApiError {
    fn into_response(self) -> Response {
        match self {
            ResolveConflictApiError::Status(code) => code.into_response(),
            ResolveConflictApiError::Message(code, message) => {
                (code, Json(ApiMessageResponse { message })).into_response()
            }
        }
    }
}

fn map_resolve_error(err: ConflictResolveError) -> ResolveConflictApiError {
    match err {
        ConflictResolveError::NoAssignee => ResolveConflictApiError::Message(
            StatusCode::BAD_REQUEST,
            crate::copy::conflict::NO_ASSIGNEE.to_string(),
        ),
        ConflictResolveError::ConnectorNotReady { assignee } => {
            ResolveConflictApiError::Message(
                StatusCode::BAD_REQUEST,
                crate::copy::conflict::connector_not_ready_reason(&assignee),
            )
        }
        ConflictResolveError::InvalidBranch => ResolveConflictApiError::Message(
            StatusCode::BAD_REQUEST,
            "Invalid base branch name.".into(),
        ),
        ConflictResolveError::InvalidFiles => ResolveConflictApiError::Message(
            StatusCode::BAD_REQUEST,
            conflict_resolve::invalid_file_list_message().to_string(),
        ),
        ConflictResolveError::Ticket(err) => match err {
            TicketError::TicketNotFound => ResolveConflictApiError::Status(StatusCode::NOT_FOUND),
            TicketError::Archived => ResolveConflictApiError::Message(
                StatusCode::CONFLICT,
                "ticket is archived; unarchive before continuing".into(),
            ),
            TicketError::Validation(message) => {
                ResolveConflictApiError::Message(StatusCode::BAD_REQUEST, message)
            }
            other => {
                tracing::error!(error = %other, "resolve conflict failed");
                ResolveConflictApiError::Status(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },
        ConflictResolveError::Run(err) => match map_run_error_response(err) {
            RunAgentError::Status(code) => ResolveConflictApiError::Status(code),
            RunAgentError::Message(code, message) => {
                ResolveConflictApiError::Message(code, message)
            }
        },
        ConflictResolveError::Comment(err) => {
            tracing::error!(error = %err, "resolve conflict comment failed");
            ResolveConflictApiError::Status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn resolve_ticket_conflict(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<ResolveConflictBody>,
) -> Result<Json<SingleRunResponse>, ResolveConflictApiError> {
    let pool = pool_from_state(&state).map_err(ResolveConflictApiError::Status)?;
    let ready = |connector: &str| connector_is_ready(&state, connector);
    let outcome = conflict_resolve::start_resolve(
        pool,
        ticket_id,
        &body.base_branch,
        &body.files,
        &ready,
    )
    .await
    .map_err(map_resolve_error)?;

    if let Some(comment_id) = outcome.new_comment_id {
        state.event_bus.publish(AppEvent::CommentCreated {
            comment_id,
            ticket_id,
            author_type: "system".into(),
        });
    }

    let connector = RunService::new(pool)
        .agent_connector_for_run(outcome.run.agent_id)
        .await
        .map_err(|err| match map_run_error_response(err) {
            RunAgentError::Status(code) => ResolveConflictApiError::Status(code),
            RunAgentError::Message(code, message) => {
                ResolveConflictApiError::Message(code, message)
            }
        })?;
    Ok(Json(single_run_response(outcome.run, connector)))
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePrBody {
    title: Option<String>,
    body: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PushBranchResponse {
    push: crate::services::ticket_git_service::PushBranchResult,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatePrResponse {
    pull_request: crate::services::ticket_git_service::CreatePrResult,
}

async fn push_ticket_branch(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<PushBranchResponse>, TicketGitApiError> {
    let pool = pool_from_state(&state).map_err(|code| {
        TicketGitApiError::Message(code, "Database unavailable.".into())
    })?;
    let push = ticket_git_service(&state, pool)
        .push_branch(ticket_id)
        .await;
    publish_ticket_state(&state, pool, ticket_id).await;
    let push = push.map_err(map_ticket_git_error_response)?;

    let comment_body = format!(
        "**Push:** branch `{}` → `{}`",
        push.ticket_branch, push.remote
    );
    create_git_action_comment(pool, &state, ticket_id, user.id, &comment_body)
        .await
        .map_err(|code| TicketGitApiError::Message(code, "Unable to record push comment.".into()))?;

    Ok(Json(PushBranchResponse { push }))
}

async fn create_ticket_pr(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<CreatePrBody>,
) -> Result<Json<CreatePrResponse>, TicketGitApiError> {
    let pool = pool_from_state(&state).map_err(|code| {
        TicketGitApiError::Message(code, "Database unavailable.".into())
    })?;
    let pull_request = ticket_git_service(&state, pool)
        .create_pr(
            ticket_id,
            body.title.as_deref(),
            body.body.as_deref(),
        )
        .await
        .map_err(map_ticket_git_error_response)?;

    let comment_body = format!(
        "**Pull request:** [#{}]({}) — {}",
        pull_request.number, pull_request.pr_url, pull_request.title
    );
    create_git_action_comment(pool, &state, ticket_id, user.id, &comment_body)
        .await
        .map_err(|code| {
            TicketGitApiError::Message(code, "Unable to record PR comment.".into())
        })?;

    Ok(Json(CreatePrResponse { pull_request }))
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
