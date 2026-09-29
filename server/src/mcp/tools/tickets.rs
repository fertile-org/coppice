use crate::domain::comment::{intent_to_str, Comment};
use crate::domain::context_profile::ContextProfile;
use crate::domain::run::run_status_to_str;
use crate::domain::slug::slugify;
use crate::domain::substatus::TicketStatus;
use crate::domain::ticket::{status_from_str, status_to_str, substatus_to_str};
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::agent_service::AgentService;
use crate::services::comment_service::CommentService;
use crate::services::repo_service::{RepoError, RepoService};
use crate::services::result_contract::ACCEPTANCE_CRITERIA_HEADER;
use crate::services::run_service::{AgentRunWithConnector, RunService};
use crate::services::ticket_service::{TicketError, TicketService, TicketWithDisplay};
use crate::services::ticket_thread;
use serde_json::{json, Value};
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

const COMMENTS_DEFAULT_LIMIT: usize = 20;
const COMMENTS_MAX_LIMIT: usize = 50;
const RUNS_DEFAULT_LIMIT: usize = 10;
const RUNS_MAX_LIMIT: usize = 30;
const SEARCH_DEFAULT_LIMIT: usize = 20;
const SEARCH_MAX_LIMIT: usize = 50;

pub(crate) fn split_ticket_description(description: &str) -> (String, Option<String>) {
    if let Some(idx) = description.find(ACCEPTANCE_CRITERIA_HEADER) {
        let body = description[..idx].trim_end().to_string();
        let acceptance = description[idx..].trim().to_string();
        (body, Some(acceptance))
    } else {
        (description.to_string(), None)
    }
}

pub(crate) fn build_comments_json(
    comments: &[Comment],
    agent_names: &HashMap<Uuid, String>,
) -> Value {
    comments
        .iter()
        .map(|comment| {
            json!({
                "id": comment.id,
                "author": ticket_thread::author_label(comment, agent_names),
                "intent": intent_to_str(comment.intent),
                "body": comment.body,
                "created_at": comment.created_at.format(&Rfc3339).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>()
        .into()
}

pub(crate) fn build_runs_json(
    runs: &[AgentRunWithConnector],
    agent_names: &HashMap<Uuid, String>,
    limit: usize,
) -> Value {
    runs.iter()
        .take(limit)
        .map(|entry| {
            let agent = agent_names
                .get(&entry.run.agent_id)
                .map(String::as_str)
                .unwrap_or("Agent");
            json!({
                "id": entry.run.id,
                "agent": agent,
                "job_type": entry.run.job_type,
                "status": run_status_to_str(entry.run.status),
                "started_at": entry.run.started_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
                "ended_at": entry.run.ended_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
            })
        })
        .collect::<Vec<_>>()
        .into()
}

/// Resolves the ticket a call targets and enforces the token's scope:
/// `args.ticketId` or the run's own ticket; same board as the run; inside the
/// batch for knowledge compaction.
async fn resolve_ticket(ctx: &ToolCtx<'_>, args: &Value) -> Result<TicketWithDisplay, ToolError> {
    let ticket_id = match args.get("ticketId").filter(|v| !v.is_null()) {
        Some(v) => v
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| ToolError::InvalidArgs("ticketId must be a ticket id".into()))?,
        None => ctx.scope.ticket_id.ok_or_else(|| {
            ToolError::InvalidArgs("ticketId is required outside a ticket run".into())
        })?,
    };

    if ctx.scope.profile == ContextProfile::KnowledgeCompaction
        && !ctx.scope.compaction_ticket_ids.contains(&ticket_id)
    {
        return Err(ToolError::Denied(
            "ticket is not part of this compaction batch".into(),
        ));
    }
    // A board-less scope (e.g. a chat session without a board) may only read
    // its own ticket; compaction is bounded by its batch above.
    if ctx.scope.board_id.is_none()
        && ctx.scope.profile != ContextProfile::KnowledgeCompaction
        && ctx.scope.ticket_id != Some(ticket_id)
    {
        return Err(ToolError::NotFound("ticket not on this board".into()));
    }

    let ticket = TicketService::new(ctx.pool)
        .get(ticket_id)
        .await
        .map_err(|e| match e {
            TicketError::TicketNotFound => ToolError::NotFound("ticket not found".into()),
            other => ToolError::Internal(other.into()),
        })?;
    if ctx
        .scope
        .board_id
        .is_some_and(|board_id| board_id != ticket.ticket.board_id)
    {
        return Err(ToolError::NotFound("ticket not on this board".into()));
    }
    Ok(ticket)
}

/// Optional integer argument clamped to `1..=max`.
fn limit_arg(args: &Value, default: usize, max: usize) -> Result<usize, ToolError> {
    match args.get("limit").filter(|v| !v.is_null()) {
        None => Ok(default),
        Some(v) => {
            let n = v
                .as_i64()
                .ok_or_else(|| ToolError::InvalidArgs("limit must be an integer".into()))?;
            Ok(usize::try_from(n.clamp(1, max as i64)).unwrap_or(default))
        }
    }
}

async fn list_agents(ctx: &ToolCtx<'_>) -> Result<Vec<crate::domain::agent::Agent>, ToolError> {
    AgentService::new(ctx.pool)
        .list_agents()
        .await
        .map_err(|e| ToolError::Internal(e.into()))
}

fn agent_key(agent: &crate::domain::agent::Agent) -> String {
    agent
        .preset_source
        .clone()
        .unwrap_or_else(|| slugify(&agent.name))
}

fn agent_names(agents: &[crate::domain::agent::Agent]) -> HashMap<Uuid, String> {
    agents.iter().map(|a| (a.id, a.name.clone())).collect()
}

fn assignee_key(agents: &[crate::domain::agent::Agent], id: Option<Uuid>) -> Option<String> {
    let id = id?;
    agents.iter().find(|a| a.id == id).map(agent_key)
}

pub async fn call_ticket_get(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let ticket = resolve_ticket(ctx, &args).await?;
    let agents = list_agents(ctx).await?;
    let (description, acceptance_criteria) = split_ticket_description(&ticket.ticket.description);

    let repo = match ticket.ticket.repo_id {
        None => Value::Null,
        Some(repo_id) => match RepoService::new(ctx.pool).get(repo_id).await {
            Ok(repo) => json!({
                "id": repo.id,
                "name": repo.name,
                "defaultBranch": repo.default_branch,
            }),
            Err(RepoError::NotFound) => Value::Null,
            Err(e) => return Err(ToolError::Internal(e.into())),
        },
    };

    Ok(json!({
        "id": ticket.ticket.id,
        "title": ticket.ticket.title,
        "description": description,
        "acceptanceCriteria": acceptance_criteria,
        "status": status_to_str(ticket.ticket.status),
        "substatus": ticket.ticket.substatus.as_ref().map(|s| substatus_to_str(*s)),
        "assignee": assignee_key(&agents, ticket.ticket.assignee_agent_id),
        "repo": repo,
        "branch": ticket.ticket.branch_name,
    }))
}

pub async fn call_ticket_comments(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let limit = limit_arg(&args, COMMENTS_DEFAULT_LIMIT, COMMENTS_MAX_LIMIT)?;
    let before = match args.get("before").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => Some(
            v.as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| ToolError::InvalidArgs("before must be a comment id".into()))?,
        ),
    };
    let ticket = resolve_ticket(ctx, &args).await?;

    // Newest first.
    let comments = CommentService::new(ctx.pool)
        .list_by_ticket(ticket.ticket.id)
        .await
        .map_err(|e| ToolError::Internal(e.into()))?;
    let start = match before {
        None => 0,
        Some(id) => {
            comments.iter().position(|c| c.id == id).ok_or_else(|| {
                ToolError::InvalidArgs("before is not a comment on this ticket".into())
            })? + 1
        }
    };
    let page: Vec<Comment> = comments.iter().skip(start).take(limit).cloned().collect();
    let has_more = start + page.len() < comments.len();
    let next_before = if has_more {
        page.last().map(|c| c.id)
    } else {
        None
    };

    let agents = list_agents(ctx).await?;
    Ok(json!({
        "comments": build_comments_json(&page, &agent_names(&agents)),
        "nextBefore": next_before,
    }))
}

pub async fn call_ticket_runs(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let limit = limit_arg(&args, RUNS_DEFAULT_LIMIT, RUNS_MAX_LIMIT)?;
    let ticket = resolve_ticket(ctx, &args).await?;
    let runs = RunService::new(ctx.pool)
        .list_for_ticket(ticket.ticket.id)
        .await
        .map_err(|e| ToolError::Internal(e.into()))?;
    let agents = list_agents(ctx).await?;
    Ok(json!({ "runs": build_runs_json(&runs, &agent_names(&agents), limit) }))
}

pub async fn call_ticket_search(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let limit = limit_arg(&args, SEARCH_DEFAULT_LIMIT, SEARCH_MAX_LIMIT)?;
    let query = args.get("query").and_then(Value::as_str);
    let status: Option<TicketStatus> =
        match args.get("status").filter(|v| !v.is_null()) {
            None => None,
            Some(v) => Some(v.as_str().and_then(status_from_str).ok_or_else(|| {
                ToolError::InvalidArgs("status is not a valid ticket status".into())
            })?),
        };
    let Some(board_id) = ctx.scope.board_id else {
        return Ok(json!({ "tickets": [] }));
    };

    let tickets = TicketService::new(ctx.pool)
        .search(Some(board_id), query, status, limit as i64)
        .await
        .map_err(|e| ToolError::Internal(e.into()))?;
    let agents = list_agents(ctx).await?;
    let items: Vec<Value> = tickets
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "title": t.title,
                "status": status_to_str(t.status),
                "assignee": assignee_key(&agents, t.assignee_agent_id),
            })
        })
        .collect();
    Ok(json!({ "tickets": items }))
}
