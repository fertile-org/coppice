use crate::api::agent_runs::{
    load_single_run_response, runs_list_response, RunsListResponse, SingleRunResponse,
};
use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::substatus::{Substatus, SubstatusDisplay, TicketStatus};
use crate::domain::ticket::{
    priority_from_str, status_from_str, substatus_from_str, TicketPriority,
};
use crate::domain::workflow::{PendingRecommendation, PendingSplitRecommendation};
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::events::bus::AppEvent;
use crate::services::comment_service::{CommentError, CommentService};
use crate::copy::conflict::DONE_COLUMN_HOLDS;
use crate::copy::plan::{NO_PM, PLAN_REQUIRED, READY_REQUIRES_PLAN};
use crate::copy::restart::{git_error, ERROR_NONE, ERROR_USE_CANCEL, ERROR_USE_DISCARD};
use crate::services::planning_service::{PlanningService, StartOutcome};
use crate::services::workflow_service::{WorkflowService, DONE_REQUIRES_ACCEPT};
use crate::domain::agent_health::AgentHealthStatus;
use crate::services::agent_health::missing_connector_detail;
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::restart_service::{RestartError, RestartService};
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
use std::collections::HashMap;
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
        .route("/api/tickets/{ticket_id}/approve-plan", post(approve_plan))
        .route(
            "/api/tickets/{ticket_id}/plan-changes",
            post(ask_for_plan_changes),
        )
        .route("/api/tickets/{ticket_id}/plan-answers", post(send_plan_answers))
        .route("/api/tickets/{ticket_id}/verify-parent", post(verify_parent))
        .route("/api/tickets/{ticket_id}/plan-runs", post(start_plan_run))
        .route("/api/tickets/{ticket_id}/assign", post(assign_agent))
        .route("/api/tickets/{ticket_id}/run-agent", post(run_agent))
        .route("/api/tickets/{ticket_id}/restart", get(get_restart))
        .route(
            "/api/tickets/{ticket_id}/restart/resume",
            post(resume_restart),
        )
        .route(
            "/api/tickets/{ticket_id}/restart/discard",
            post(discard_restart),
        )
        .route(
            "/api/tickets/{ticket_id}/restart/cancel",
            post(cancel_restart),
        )
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
    /// Left out of board lists; the ticket view loads it.
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
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
    skip_planning: bool,
    plan_approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_plan_run_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approved_plan: Option<ApprovedPlanResponse>,
    /// In Plan Review: the plan waiting for approval, shown under the
    /// description next to Approve and Ask for changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pending_plan: Option<PendingPlanResponse>,
    /// The planner's questions. `waiting` is the stored "waiting for your
    /// answers" state.
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_questions: Option<crate::services::planning_service::PlanQuestionsState>,
    /// In Plan Review: there is a plan to approve.
    has_plan: bool,
    /// Split into sub-tickets at Plan Review.
    split_parent: bool,
    /// Verify parent ticket can run now.
    verify_available: bool,
    has_active_run: bool,
    archived_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    human_review: Option<HumanReviewResponse>,
    /// Present only when an implementation run reported a reviewer brief.
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewer_brief: Option<crate::services::reviewer_brief::ReviewerBrief>,
    attachment_ids: Vec<Uuid>,
    attachments: Vec<AttachmentSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentSummary {
    id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingPlanResponse {
    comment_id: Uuid,
    markdown: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApprovedPlanResponse {
    comment_id: Uuid,
    markdown: String,
    steps: Vec<String>,
    /// The plan's acceptance criteria, each with the latest result for the
    /// reviewed commit (or not checked).
    criteria: Vec<crate::services::criteria_results::CriterionView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HumanReviewResponse {
    head_sha: String,
    short_sha: String,
    stale: bool,
    merged: bool,
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
    #[serde(default)]
    attachment_ids: Vec<Uuid>,
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
    skip_planning: Option<bool>,
    attachment_ids: Option<Vec<Uuid>>,
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
        description: Some(ticket.description),
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
        skip_planning: ticket.skip_planning,
        plan_approved: item.plan_approved,
        latest_plan_run_status: item.latest_plan_run_status,
        approved_plan: None,
        pending_plan: None,
        plan_questions: item.plan_questions,
        has_plan: item.has_plan,
        split_parent: item.split_parent,
        verify_available: item.verify_available,
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
                merged: review.merged_at.is_some(),
                comment_ids: review.comment_ids,
                run_ids: review.run_ids,
            }
        }),
        reviewer_brief: None,
        attachment_ids: ticket.attachment_ids,
        attachments: Vec::new(),
    }
}

async fn load_attachment_summaries(
    pool: &sqlx::PgPool,
    ids: &[Uuid],
) -> Result<Vec<AttachmentSummary>, CommentError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = CommentService::new(pool)
        .list_attachments_by_ids(ids)
        .await?;
    let by_id: HashMap<Uuid, _> = rows.into_iter().map(|row| (row.id, row)).collect();
    Ok(ids
        .iter()
        .filter_map(|id| by_id.get(id))
        .map(|row| AttachmentSummary {
            id: row.id,
            filename: row.filename.clone(),
            content_type: row.content_type.clone(),
            size_bytes: row.size_bytes,
        })
        .collect())
}

async fn store_attachment_ids(
    pool: &sqlx::PgPool,
    store: &crate::storage::attachment_store::AttachmentStore,
    ticket: TicketWithDisplay,
    attachment_ids: Option<Vec<Uuid>>,
) -> Result<TicketWithDisplay, StatusCode> {
    let Some(attachment_ids) = attachment_ids else {
        return Ok(ticket);
    };
    let mut unique = Vec::new();
    for id in attachment_ids {
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    if !unique.is_empty() {
        let found = CommentService::new(pool)
            .list_attachments_by_ids(&unique)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if found.len() != unique.len() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let dropped: Vec<Uuid> = ticket
        .ticket
        .attachment_ids
        .iter()
        .filter(|id| !unique.contains(id))
        .copied()
        .collect();
    let updated = TicketService::new(pool)
        .set_attachment_ids(ticket.ticket.id, &unique)
        .await
        .map_err(map_error)?;
    if let Err(err) =
        crate::services::attachment_cleanup::delete_if_unreferenced(pool, store, &dropped).await
    {
        tracing::warn!(error = %err, "could not remove dropped attachments");
    }
    Ok(updated)
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
        TicketGitError::ActiveRun => (
            StatusCode::CONFLICT,
            "Stop the agent run first. Coppice doesn't remove a worktree an agent is working in.".into(),
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
            if matches!(&kind, GitConflictKind::Merge) {
                match conflict_resolve::reopen_done_after_merge_conflict(
                    pool,
                    ticket_id,
                    &base_branch,
                    &paths,
                )
                .await
                {
                    Ok(Some(reopen)) => {
                        tracing::info!(
                            comment_id = %reopen.comment_id,
                            run_id = ?reopen.run_id,
                            "done ticket returned to in progress after a merge conflict"
                        );
                        state.event_bus.publish(AppEvent::CommentCreated {
                            comment_id: reopen.comment_id,
                            ticket_id,
                            author_type: "system".into(),
                        });
                        publish_ticket_state(state, pool, ticket_id).await;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::error!(
                            error = %err,
                            "merge conflict did not return the ticket to In Progress"
                        );
                        return TicketGitApiError::Message(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            crate::copy::conflict::MERGE_CONFLICT_REOPEN_FAILED.to_string(),
                        );
                    }
                }
            }
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
        tickets
            .into_iter()
            .map(|ticket| TicketResponse {
                description: None,
                ..ticket_to_response(ticket)
            })
            .collect(),
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
    let ticket = store_attachment_ids(pool, &state.attachments, ticket, Some(body.attachment_ids)).await?;
    let mut response = ticket_to_response(ticket);
    response.attachments = load_attachment_summaries(pool, &response.attachment_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn get_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketService::new(pool);
    let ticket = service.get(ticket_id).await.map_err(map_error)?;
    Ok(Json(
        with_approved_plan(&state, pool, ticket)
            .await
            .map_err(map_error)?,
    ))
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
    spawn_archived_ticket_cleanup(&state, ticket_id);
    Ok(Json(ticket_to_response(ticket)))
}

/// Off the request path: drop the archived ticket's worktree, branch and run
/// artifacts.
fn spawn_archived_ticket_cleanup(state: &Arc<AppState>, ticket_id: Uuid) {
    crate::services::worktree_cleanup::spawn_cleanup(state, ticket_id);
    let Some(pool) = state.db.clone() else {
        return;
    };
    let artifacts_dir = state.config.storage.artifacts_dir.clone();
    tokio::spawn(async move {
        if let Err(err) = crate::services::artifact_retention::remove_ticket_run_artifacts(
            &pool,
            std::path::Path::new(&artifacts_dir),
            ticket_id,
        )
        .await
        {
            tracing::warn!(%ticket_id, error = %err, "could not remove archived ticket's run artifacts");
        }
    });
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
    let has_field_update = body.title.is_some()
        || body.description.is_some()
        || body.repo_id.is_some()
        || body.priority.is_some()
        || body.branch_name.is_some()
        || body.owner_user_id.is_some()
        || body.skip_planning.is_some();
    let ticket = if has_field_update {
        service
            .update_fields(
                ticket_id,
                body.title.as_deref(),
                body.description.as_deref(),
                body.repo_id.map(Some),
                priority,
                body.branch_name.as_deref().map(Some),
                body.owner_user_id.map(Some),
                body.skip_planning,
                true,
            )
            .await
            .map_err(map_error)?
    } else {
        service.get(ticket_id).await.map_err(map_error)?
    };
    let ticket = store_attachment_ids(pool, &state.attachments, ticket, body.attachment_ids).await?;
    crate::events::publish_ticket_updated(&state.event_bus, &ticket);
    let mut response = ticket_to_response(ticket);
    response.attachments = load_attachment_summaries(pool, &response.attachment_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(response))
}

enum UpdateStatusError {
    Status(StatusCode),
    Message(StatusCode, String),
}

impl IntoResponse for UpdateStatusError {
    fn into_response(self) -> Response {
        match self {
            UpdateStatusError::Status(code) => code.into_response(),
            UpdateStatusError::Message(code, message) => {
                (code, Json(ApiMessageResponse { message })).into_response()
            }
        }
    }
}

fn map_update_status_error(err: TicketError) -> UpdateStatusError {
    match err {
        TicketError::Validation(message)
            if message == DONE_REQUIRES_ACCEPT
                || message == PLAN_REQUIRED
                || message == DONE_COLUMN_HOLDS
                || message == READY_REQUIRES_PLAN
                || crate::copy::review::is_ready_hold(&message)
                || crate::copy::review::is_entry_hold(&message) =>
        {
            UpdateStatusError::Message(StatusCode::BAD_REQUEST, message)
        }
        other => UpdateStatusError::Status(map_error(other)),
    }
}

async fn update_status(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<UpdateStatusBody>,
) -> Result<Json<TicketResponse>, UpdateStatusError> {
    let pool = pool_from_state(&state).map_err(UpdateStatusError::Status)?;
    let service = TicketService::new(pool);
    let status = parse_status(&body.status).map_err(map_update_status_error)?;
    let substatus = match body.substatus {
        Some(s) => Some(Some(parse_substatus(&s).map_err(map_update_status_error)?)),
        None => None,
    };
    let substatus_metadata = body.substatus_metadata.map(Some);
    let previous = service
        .get(ticket_id)
        .await
        .map_err(map_update_status_error)?;
    let mut ticket = service
        .update_status(ticket_id, status, substatus, substatus_metadata)
        .await
        .map_err(map_update_status_error)?;
    if previous.ticket.status != TicketStatus::Ready
        && ticket.ticket.status == TicketStatus::Ready
    {
        if let Err(err) = PlanningService::new(pool).on_entered_ready(ticket_id).await {
            tracing::warn!(error = %err, %ticket_id, "tech lead run did not start");
        }
        ticket = service
            .get(ticket_id)
            .await
            .map_err(map_update_status_error)?;
    }
    Ok(Json(ticket_to_response(ticket)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanChangesBody {
    comment: String,
}

async fn start_plan_run(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, UpdateStatusError> {
    let pool = pool_from_state(&state).map_err(UpdateStatusError::Status)?;
    let current = TicketService::new(pool)
        .get(ticket_id)
        .await
        .map_err(map_update_status_error)?;
    TicketService::ensure_not_archived(&current.ticket).map_err(map_update_status_error)?;
    let outcome = PlanningService::new(pool)
        .start_planning(ticket_id)
        .await
        .map_err(map_plan_error)?;
    if matches!(outcome, StartOutcome::NoPlanner) {
        return Err(UpdateStatusError::Message(
            StatusCode::BAD_REQUEST,
            NO_PM.to_string(),
        ));
    }
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .map_err(map_update_status_error)?;
    crate::events::publish_ticket_updated(&state.event_bus, &ticket);
    Ok(Json(ticket_to_response(ticket)))
}

async fn approve_plan(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, UpdateStatusError> {
    let pool = pool_from_state(&state).map_err(UpdateStatusError::Status)?;
    PlanningService::new(pool)
        .approve(ticket_id, &state.config.workflow)
        .await
        .map_err(map_plan_error)?;
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .map_err(map_update_status_error)?;
    Ok(Json(
        with_approved_plan(&state, pool, ticket)
            .await
            .map_err(map_update_status_error)?,
    ))
}

async fn ask_for_plan_changes(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<PlanChangesBody>,
) -> Result<Json<TicketResponse>, UpdateStatusError> {
    let pool = pool_from_state(&state).map_err(UpdateStatusError::Status)?;
    PlanningService::new(pool)
        .ask_for_changes(ticket_id, &body.comment)
        .await
        .map_err(map_plan_error)?;
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .map_err(map_update_status_error)?;
    Ok(Json(ticket_to_response(ticket)))
}

#[derive(Deserialize)]
struct PlanAnswersBody {
    answers: Vec<String>,
}

async fn send_plan_answers(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<PlanAnswersBody>,
) -> Result<Json<TicketResponse>, UpdateStatusError> {
    let pool = pool_from_state(&state).map_err(UpdateStatusError::Status)?;
    PlanningService::new(pool)
        .send_answers(ticket_id, &body.answers)
        .await
        .map_err(map_plan_error)?;
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .map_err(map_update_status_error)?;
    crate::events::publish_ticket_updated(&state.event_bus, &ticket);
    Ok(Json(ticket_to_response(ticket)))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct VerifyParentBody {
    #[serde(default)]
    agent_id: Option<Uuid>,
}

async fn verify_parent(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    body: Option<Json<VerifyParentBody>>,
) -> Result<Json<TicketResponse>, UpdateStatusError> {
    let pool = pool_from_state(&state).map_err(UpdateStatusError::Status)?;
    let agent_id = body.and_then(|Json(body)| body.agent_id);
    PlanningService::new(pool)
        .start_verification(ticket_id, agent_id)
        .await
        .map_err(map_plan_error)?;
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .map_err(map_update_status_error)?;
    crate::events::publish_ticket_updated(&state.event_bus, &ticket);
    Ok(Json(ticket_to_response(ticket)))
}

fn map_plan_error(err: crate::services::planning_service::PlanError) -> UpdateStatusError {
    match err {
        crate::services::planning_service::PlanError::Validation(message) => {
            UpdateStatusError::Message(StatusCode::BAD_REQUEST, message)
        }
        crate::services::planning_service::PlanError::Database(err) => {
            tracing::warn!(error = %err, "plan request failed");
            UpdateStatusError::Status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn with_approved_plan(
    state: &AppState,
    pool: &sqlx::PgPool,
    ticket: TicketWithDisplay,
) -> Result<TicketResponse, TicketError> {
    let plan = PlanningService::new(pool)
        .approved_plan(ticket.ticket.id)
        .await
        .map_err(|err| match err {
            crate::services::planning_service::PlanError::Database(err) => TicketError::Database(err),
            crate::services::planning_service::PlanError::Validation(message) => {
                TicketError::Validation(message)
            }
        })?;
    let ticket_id = ticket.ticket.id;
    let status = ticket.ticket.status;
    let mut response = ticket_to_response(ticket);
    let criteria = match &plan {
        Some(plan) => {
            let texts = crate::copy::plan::plan_criteria(&plan.markdown);
            if texts.is_empty() {
                Vec::new()
            } else {
                let stored = crate::services::criteria_results::CriteriaResultsService::new(pool)
                    .get(ticket_id)
                    .await?;
                let reviewed = if stored.is_none() {
                    None
                } else if PlanningService::new(pool)
                    .is_split_parent(ticket_id)
                    .await
                    .map_err(|err| TicketError::Validation(err.to_string()))?
                {
                    Some(crate::workers::job_worker::VERIFIED_RESULTS_SHA.to_string())
                } else {
                    reviewed_commit(state, pool, ticket_id).await?
                };
                crate::services::criteria_results::criteria_view(
                    &texts,
                    stored.as_ref(),
                    reviewed.as_deref(),
                )
            }
        }
        None => Vec::new(),
    };
    if status == TicketStatus::PlanReview && response.has_plan {
        response.pending_plan = PlanningService::new(pool)
            .latest_plan_comment(ticket_id)
            .await
            .map_err(|err| TicketError::Validation(err.to_string()))?
            .map(|comment| PendingPlanResponse {
                comment_id: comment.id,
                markdown: comment.body,
            });
    }
    response.approved_plan = plan.map(|plan| ApprovedPlanResponse {
        comment_id: plan.comment_id,
        markdown: plan.markdown,
        steps: plan.steps,
        criteria,
    });
    response.reviewer_brief = reviewer_brief_for_ticket(state, pool, ticket_id, status).await?;
    response.attachments = load_attachment_summaries(pool, &response.attachment_ids)
        .await
        .map_err(|err| match err {
            CommentError::Database(err) => TicketError::Database(err),
            other => TicketError::Validation(other.to_string()),
        })?;
    Ok(response)
}

/// The stored brief, hidden in Human Review when it was written for a different
/// commit than the one review is bound to.
/// The commit Human Review is bound to, or `None` when it can't be read.
async fn reviewed_commit(
    state: &AppState,
    pool: &sqlx::PgPool,
    ticket_id: Uuid,
) -> Result<Option<String>, TicketError> {
    match ticket_git_service(state, pool)
        .reviewed_head_sha(ticket_id)
        .await
    {
        Ok(sha) => Ok(sha),
        Err(crate::services::ticket_git_service::TicketGitError::Database(err)) => Err(err.into()),
        Err(crate::services::ticket_git_service::TicketGitError::Ticket(err)) => Err(err),
        Err(err) => {
            tracing::debug!(error = %err, "reviewed commit unavailable");
            Ok(None)
        }
    }
}

async fn reviewer_brief_for_ticket(
    state: &AppState,
    pool: &sqlx::PgPool,
    ticket_id: Uuid,
    status: TicketStatus,
) -> Result<Option<crate::services::reviewer_brief::ReviewerBrief>, TicketError> {
    let brief = crate::services::reviewer_brief::ReviewerBriefService::new(pool)
        .get(ticket_id)
        .await?;
    if status != TicketStatus::WaitForFinalReview {
        return Ok(brief);
    }
    let Some(brief) = brief else {
        return Ok(None);
    };
    let reviewed = reviewed_commit(state, pool, ticket_id).await?;
    if crate::services::reviewer_brief::brief_matches_reviewed_commit(
        &brief.commit_sha,
        reviewed.as_deref(),
    ) {
        Ok(Some(brief))
    } else {
        Ok(None)
    }
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
    if ticket.split_parent {
        return Err(RunAgentError::Message(
            StatusCode::BAD_REQUEST,
            crate::copy::plan::UMBRELLA_NO_WORK.into(),
        ));
    }

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
    let response = load_single_run_response(&service, run, connector)
        .await
        .map_err(map_run_error_response)?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartResponse {
    pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed_files: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resume_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    other_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirm: Option<String>,
}

fn restart_response(
    choice: Option<crate::services::restart_service::RestartChoice>,
) -> RestartResponse {
    match choice {
        None => RestartResponse {
            pending: false,
            run_id: None,
            changed_files: None,
            resume_label: None,
            other_label: None,
            confirm: None,
        },
        Some(choice) => RestartResponse {
            pending: true,
            run_id: Some(choice.run_id),
            changed_files: Some(choice.changed_files),
            resume_label: Some(choice.resume_label.to_string()),
            other_label: Some(choice.other_label.to_string()),
            confirm: choice.confirm,
        },
    }
}

enum RestartApiError {
    Status(StatusCode),
    Message(StatusCode, String),
}

impl IntoResponse for RestartApiError {
    fn into_response(self) -> Response {
        match self {
            RestartApiError::Status(code) => code.into_response(),
            RestartApiError::Message(code, message) => {
                (code, Json(ApiMessageResponse { message })).into_response()
            }
        }
    }
}

fn map_restart_error(err: RestartError) -> RestartApiError {
    match err {
        RestartError::NothingPending => {
            RestartApiError::Message(StatusCode::NOT_FOUND, ERROR_NONE.to_string())
        }
        RestartError::UseDiscard => {
            RestartApiError::Message(StatusCode::BAD_REQUEST, ERROR_USE_DISCARD.to_string())
        }
        RestartError::UseCancel => {
            RestartApiError::Message(StatusCode::BAD_REQUEST, ERROR_USE_CANCEL.to_string())
        }
        RestartError::Git(detail) => {
            tracing::warn!(error = %detail, "restart worktree action failed");
            RestartApiError::Message(StatusCode::INTERNAL_SERVER_ERROR, git_error(&detail))
        }
        RestartError::Run(RunError::ActiveRunExists)
        | RestartError::Ticket(TicketError::ActiveRunExists)
        | RestartError::Ticket(TicketError::Archived) => {
            RestartApiError::Status(StatusCode::CONFLICT)
        }
        RestartError::Run(RunError::NotFound)
        | RestartError::Ticket(TicketError::TicketNotFound)
        | RestartError::Ticket(TicketError::BoardNotFound) => {
            RestartApiError::Message(StatusCode::NOT_FOUND, ERROR_NONE.to_string())
        }
        RestartError::Run(RunError::Validation(message))
        | RestartError::Ticket(TicketError::Validation(message)) => {
            // Status-write backstops. The sentences stay off the client.
            if message == "ticket is not waiting after a restart"
                || message == "accepted work is not resumed"
            {
                RestartApiError::Message(StatusCode::NOT_FOUND, ERROR_NONE.to_string())
            } else {
                RestartApiError::Message(StatusCode::BAD_REQUEST, message)
            }
        }
        RestartError::Ticket(
            TicketError::InvalidStatus
            | TicketError::InvalidSubstatus
            | TicketError::InvalidPriority,
        ) => RestartApiError::Status(StatusCode::BAD_REQUEST),
        RestartError::Ticket(TicketError::Database(err))
        | RestartError::Run(RunError::Database(err))
        | RestartError::Database(err) => {
            tracing::warn!(error = %err, "restart action failed");
            RestartApiError::Status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_restart(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<RestartResponse>, RestartApiError> {
    let pool = pool_from_state(&state).map_err(RestartApiError::Status)?;
    let choice = RestartService::new(pool)
        .open(ticket_id)
        .await
        .map_err(map_restart_error)?;
    Ok(Json(restart_response(choice)))
}

async fn resume_restart(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<RestartResponse>, RestartApiError> {
    let pool = pool_from_state(&state).map_err(RestartApiError::Status)?;
    RestartService::new(pool)
        .resume(ticket_id)
        .await
        .map_err(map_restart_error)?;
    publish_ticket_state(&state, pool, ticket_id).await;
    Ok(Json(restart_response(None)))
}

async fn discard_restart(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<RestartResponse>, RestartApiError> {
    let pool = pool_from_state(&state).map_err(RestartApiError::Status)?;
    RestartService::new(pool)
        .discard(ticket_id)
        .await
        .map_err(map_restart_error)?;
    publish_ticket_state(&state, pool, ticket_id).await;
    Ok(Json(restart_response(None)))
}

async fn cancel_restart(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<RestartResponse>, RestartApiError> {
    let pool = pool_from_state(&state).map_err(RestartApiError::Status)?;
    RestartService::new(pool)
        .cancel(ticket_id)
        .await
        .map_err(map_restart_error)?;
    publish_ticket_state(&state, pool, ticket_id).await;
    Ok(Json(restart_response(None)))
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
    WorkflowService::final_approve(ticket.ticket.status)
        .map_err(|_| FinalApproveError::Status(StatusCode::BAD_REQUEST))?;

    // A split parent has no branch of its own: its work merged with the
    // sub-tickets, so there is no commit to bind the acceptance to.
    let reviewed = if ticket.split_parent {
        Ok(None)
    } else {
        ticket_git_service(&state, pool)
            .reviewed_head_sha(ticket_id)
            .await
    };
    match reviewed {
        Ok(Some(head_sha)) => {
            HumanReviewService::new(pool)
                .record_acceptance(ticket_id, user.id, &head_sha)
                .await
                .map_err(|_| FinalApproveError::Status(StatusCode::INTERNAL_SERVER_ERROR))?;
        }
        Ok(None) => {}
        Err(err) => return Err(map_ticket_git_error_response(err).into()),
    }

    // Staged attachment copies stay until a successful merge or ticket delete.
    // A failed merge sends this ticket back to In Progress.
    let updated = ticket_svc
        .accept_to_done(ticket_id)
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

    // Accept does not remove these copies. A failed merge (the Err arm above)
    // returns the ticket to In Progress and leaves them in place.
    if let Err(err) = crate::services::ticket_attachments::clear_if_ended(
        &state.config.storage.artifacts_dir,
        ticket_id,
        crate::services::ticket_attachments::AttachmentFolderEnd::MergeSucceeded,
    ) {
        tracing::warn!(
            error = %err,
            %ticket_id,
            "could not remove staged ticket attachments after merge"
        );
    }

    let short_sha = merge.head_sha.get(..7).unwrap_or(&merge.head_sha);
    let comment_body = format!(
        "**Merge:** {} (`{short_sha}` on `{}`)",
        merge.message, merge.base_branch
    );
    create_git_action_comment(pool, &state, ticket_id, user.id, &comment_body)
        .await
        .map_err(|code| TicketGitApiError::Message(code, "Unable to record merge comment.".into()))?;
    // The merged work is on the base branch now.
    crate::services::worktree_cleanup::spawn_cleanup(&state, ticket_id);

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

    let service = RunService::new(pool);
    let connector = service
        .agent_connector_for_run(outcome.run.agent_id)
        .await
        .map_err(|err| match map_run_error_response(err) {
            RunAgentError::Status(code) => ResolveConflictApiError::Status(code),
            RunAgentError::Message(code, message) => {
                ResolveConflictApiError::Message(code, message)
            }
        })?;
    let response = load_single_run_response(&service, outcome.run, connector)
        .await
        .map_err(|err| match map_run_error_response(err) {
            RunAgentError::Status(code) => ResolveConflictApiError::Status(code),
            RunAgentError::Message(code, message) => {
                ResolveConflictApiError::Message(code, message)
            }
        })?;
    Ok(Json(response))
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
