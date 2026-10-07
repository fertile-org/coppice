//! Ask the ticket's assignee to resolve a merge or rebase conflict.
//!
//! The run is a normal `work_on_ticket` run for that agent, so the job worker
//! launches their connector with the pinned run contract.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::copy::conflict::{
    ask_to_resolve_label, conflict_message, connector_not_ready_reason, resolve_prompt,
    INVALID_FILE_LIST, MERGE_ACTION, NO_ASSIGNEE, REBASE_ACTION, REREVIEW_NOTE,
};
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::context_profile::ContextProfile;
use crate::domain::run::AgentRun;
use crate::services::agent_service::AgentService;
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::run_service::{RunError, RunService, StartRunOptions};
use crate::services::ticket_git_service::{validate_branch_name, GitConflictKind};
use crate::services::ticket_service::{TicketError, TicketService};

const JOB_TYPE_WORK_ON_TICKET: &str = "work_on_ticket";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictOfferJson {
    pub operation: &'static str,
    pub base_branch: String,
    pub files: Vec<String>,
    pub can_ask_assignee: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    pub rereview_note: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitConflictHttp {
    pub message: String,
    pub conflict: ConflictOfferJson,
}

pub struct ResolveConflictOutcome {
    pub run: AgentRun,
    pub new_comment_id: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConflictResolveError {
    #[error("no assignee")]
    NoAssignee,
    #[error("connector not ready")]
    ConnectorNotReady { assignee: String },
    #[error("invalid branch")]
    InvalidBranch,
    #[error("invalid files")]
    InvalidFiles,
    #[error(transparent)]
    Ticket(#[from] TicketError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error(transparent)]
    Comment(#[from] CommentError),
}

pub fn invalid_file_list_message() -> &'static str {
    INVALID_FILE_LIST
}

pub async fn build_git_conflict_response(
    pool: &PgPool,
    ticket_id: Uuid,
    kind: GitConflictKind,
    base_branch: String,
    files: Vec<String>,
    connector_ready: &(dyn Fn(&str) -> bool + Sync),
) -> GitConflictHttp {
    let action = match kind {
        GitConflictKind::Merge => MERGE_ACTION,
        GitConflictKind::Rebase => REBASE_ACTION,
    };
    let message = conflict_message(action, &base_branch, &files);
    let (can_ask_assignee, ask_label, unavailable_reason) =
        ask_offer(pool, ticket_id, connector_ready).await;
    GitConflictHttp {
        message,
        conflict: ConflictOfferJson {
            operation: kind.as_str(),
            base_branch,
            files,
            can_ask_assignee,
            ask_label,
            unavailable_reason,
            rereview_note: REREVIEW_NOTE,
        },
    }
}

async fn ask_offer(
    pool: &PgPool,
    ticket_id: Uuid,
    connector_ready: &(dyn Fn(&str) -> bool + Sync),
) -> (bool, Option<String>, Option<String>) {
    let assignee_id = match TicketService::new(pool).get(ticket_id).await {
        Ok(ticket) => ticket.ticket.assignee_agent_id,
        Err(err) => {
            tracing::warn!(error = %err, "conflict offer could not load the ticket");
            return (false, None, Some(NO_ASSIGNEE.to_string()));
        }
    };
    let Some(assignee_id) = assignee_id else {
        return (false, None, Some(NO_ASSIGNEE.to_string()));
    };
    let agent = match AgentService::new(pool).get(assignee_id).await {
        Ok(agent) => agent,
        Err(err) => {
            tracing::warn!(error = %err, "conflict offer could not load the assignee");
            return (false, None, Some(NO_ASSIGNEE.to_string()));
        }
    };
    if !connector_ready(&agent.connector) {
        return (false, None, Some(connector_not_ready_reason(&agent.name)));
    }
    (true, Some(ask_to_resolve_label(&agent.name)), None)
}

pub async fn start_resolve(
    pool: &PgPool,
    ticket_id: Uuid,
    base_branch: &str,
    files: &[String],
    connector_ready: &(dyn Fn(&str) -> bool + Sync),
) -> Result<ResolveConflictOutcome, ConflictResolveError> {
    let base_branch = base_branch.trim();
    validate_branch_name(base_branch).map_err(|_| ConflictResolveError::InvalidBranch)?;
    let files = clean_conflict_files(files)?;

    let ticket = TicketService::new(pool).get(ticket_id).await?;
    TicketService::ensure_not_archived(&ticket.ticket)?;
    let Some(agent_id) = ticket.ticket.assignee_agent_id else {
        return Err(ConflictResolveError::NoAssignee);
    };
    let agent = AgentService::new(pool).get(agent_id).await.map_err(|err| {
        tracing::warn!(error = %err, "resolve conflict could not load the assignee");
        ConflictResolveError::NoAssignee
    })?;
    if !connector_ready(&agent.connector) {
        return Err(ConflictResolveError::ConnectorNotReady {
            assignee: agent.name,
        });
    }

    let run_svc = RunService::new(pool);
    if let Some(run) = run_svc.active_for_ticket_agent(ticket_id, agent_id).await? {
        return Ok(ResolveConflictOutcome {
            run,
            new_comment_id: None,
        });
    }

    let prompt = resolve_prompt(base_branch, &files);
    let comment = CommentService::new(pool)
        .create(
            ticket_id,
            AuthorType::System,
            None,
            &prompt,
            CommentIntent::SystemEvent,
            &[],
            &[],
        )
        .await?;

    match run_svc
        .start_run_for_agent(
            ticket_id,
            agent_id,
            JOB_TYPE_WORK_ON_TICKET,
            StartRunOptions {
                context_profile: ContextProfile::HumanAgent,
                trigger_comment_id: Some(comment.id),
            },
        )
        .await
    {
        Ok(run) => Ok(ResolveConflictOutcome {
            run,
            new_comment_id: Some(comment.id),
        }),
        Err(RunError::ActiveRunExists) => {
            let run = run_svc
                .active_for_ticket_agent(ticket_id, agent_id)
                .await?
                .ok_or(RunError::ActiveRunExists)?;
            Ok(ResolveConflictOutcome {
                run,
                new_comment_id: None,
            })
        }
        Err(err) => Err(err.into()),
    }
}

fn clean_conflict_files(files: &[String]) -> Result<Vec<String>, ConflictResolveError> {
    let mut cleaned = Vec::new();
    for file in files {
        let file = file.trim();
        if file.is_empty() {
            continue;
        }
        if file.len() > 500 || file.chars().any(char::is_control) {
            return Err(ConflictResolveError::InvalidFiles);
        }
        cleaned.push(file.to_string());
    }
    if cleaned.len() > 100 {
        return Err(ConflictResolveError::InvalidFiles);
    }
    cleaned.sort();
    cleaned.dedup();
    Ok(cleaned)
}
