use crate::domain::comment::{intent_to_str, Comment};
use crate::domain::context_profile::ContextProfile;
use crate::domain::run::run_status_to_str;
use crate::domain::slug::slugify;
use crate::domain::substatus::TicketStatus;
use crate::domain::ticket::{status_from_str, status_to_str, substatus_to_str};
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::agent_service::AgentService;
use crate::services::board_service::BoardService;
use crate::services::comment_service::CommentService;
use crate::services::design_service::DesignService;
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

/// Splits at the `## Acceptance criteria` heading line. A `###` subsection or
/// the header quoted inside a line (as in an approved plan) does not split.
pub(crate) fn split_ticket_description(description: &str) -> (String, Option<String>) {
    let mut offset = 0;
    for line in description.split_inclusive('\n') {
        if line.trim() == ACCEPTANCE_CRITERIA_HEADER {
            let body = description[..offset].trim_end().to_string();
            let acceptance = description[offset..].trim().to_string();
            return (body, Some(acceptance));
        }
        offset += line.len();
    }
    (description.to_string(), None)
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

    if ctx.scope.profile == ContextProfile::ConnectorCheck {
        return Err(ToolError::Denied(
            "connection checks cannot read tickets".into(),
        ));
    }
    if ctx.scope.profile == ContextProfile::KnowledgeCompaction
        && !ctx.scope.compaction_ticket_ids.contains(&ticket_id)
    {
        return Err(ToolError::Denied(
            "ticket is not part of this compaction batch".into(),
        ));
    }
    // A board-less scope may only read its own ticket. Conversation chat may
    // read any ticket. Compaction is bounded by its batch above.
    if ctx.scope.board_id.is_none()
        && !ctx.scope.workspace_chat()
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
    if !ctx.scope.workspace_chat()
        && ctx
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
    if ctx.scope.profile == ContextProfile::ConnectorCheck {
        return Ok(json!({
            "title": "Coppice connection check",
            "description": "Submit a done result with summary 'connection ok'.",
        }));
    }
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

    let mut body = json!({
        "id": ticket.ticket.id,
        "title": ticket.ticket.title,
        "description": description,
        "acceptanceCriteria": acceptance_criteria,
        "status": status_to_str(ticket.ticket.status),
        "substatus": ticket.ticket.substatus.as_ref().map(|s| substatus_to_str(*s)),
        "assignee": assignee_key(&agents, ticket.ticket.assignee_agent_id),
        "repo": repo,
        "branch": ticket.ticket.branch_name,
    });
    if ctx.scope.workspace_chat() {
        let names = board_names(ctx).await?;
        body["boardName"] = json!(names.get(&ticket.ticket.board_id));
    }
    let designs = DesignService::new(ctx.pool, &ctx.state.config.storage.artifacts_dir)
        .list_for_ticket(ticket.ticket.id)
        .await
        .map_err(|err| ToolError::Internal(err.into()))?;
    body["designs"] = json!(
        designs
            .iter()
            .map(|set| {
                json!({
                    "title": set.title,
                    "options": set.options.iter().map(|option| json!({
                        "id": option.id,
                        "title": option.title,
                    })).collect::<Vec<_>>(),
                    "chosen": set.chosen.as_ref().map(|chosen| json!({
                        "id": chosen.id,
                        "title": chosen.title,
                    })),
                })
            })
            .collect::<Vec<_>>()
    );
    Ok(body)
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

    // Newest first, paged in SQL.
    let (page, has_more): (Vec<Comment>, bool) = CommentService::new(ctx.pool)
        .list_page(
            ticket.ticket.id,
            before,
            i64::try_from(limit).unwrap_or(i64::MAX),
        )
        .await
        .map_err(|e| match e {
            crate::services::comment_service::CommentError::Validation(message) => {
                ToolError::InvalidArgs(message)
            }
            other => ToolError::Internal(other.into()),
        })?;
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
        .list_recent_for_ticket(ticket.ticket.id, i64::try_from(limit).unwrap_or(i64::MAX))
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
    let workspace = ctx.scope.workspace_chat();
    if ctx.scope.board_id.is_none() && !workspace {
        return Ok(json!({ "tickets": [] }));
    }

    // The session board picks the focus repo. Conversation search ignores it.
    let search_board = if workspace {
        None
    } else {
        ctx.scope.board_id
    };
    let tickets = TicketService::new(ctx.pool)
        .search(search_board, query, status, limit as i64)
        .await
        .map_err(|e| ToolError::Internal(e.into()))?;
    let agents = list_agents(ctx).await?;
    let names = if workspace {
        board_names(ctx).await?
    } else {
        HashMap::new()
    };
    let items: Vec<Value> = tickets
        .iter()
        .map(|t| {
            let mut item = json!({
                "id": t.id,
                "title": t.title,
                "status": status_to_str(t.status),
                "assignee": assignee_key(&agents, t.assignee_agent_id),
            });
            if workspace {
                item["boardName"] = json!(names.get(&t.board_id));
            }
            item
        })
        .collect();
    Ok(json!({ "tickets": items }))
}

async fn board_names(ctx: &ToolCtx<'_>) -> Result<HashMap<Uuid, String>, ToolError> {
    let boards = BoardService::new(ctx.pool)
        .list_boards()
        .await
        .map_err(|e| ToolError::Internal(e.into()))?;
    Ok(boards
        .into_iter()
        .map(|board| (board.id, board.name))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_the_acceptance_criteria_heading_line() {
        let (body, criteria) =
            split_ticket_description("Fix it.\n\n## Acceptance criteria\n- [ ] Works\n");
        assert_eq!(body, "Fix it.");
        assert_eq!(criteria.as_deref(), Some("## Acceptance criteria\n- [ ] Works"));
    }

    #[test]
    fn an_approved_plan_keeps_its_criteria_subsection() {
        let description = "Fix it.\n\n## Approved plan\n\n### Steps\n- [ ] Do it\n\n### Acceptance criteria\n- [ ] Works\n\n### Risks\nNone.";
        let (body, criteria) = split_ticket_description(description);
        assert_eq!(body, description);
        assert_eq!(criteria, None);
    }

    #[test]
    fn the_heading_quoted_inside_a_line_does_not_split() {
        let description = "Fix it.\n- [ ] Split on `## Acceptance criteria` only as a line\n\n## Acceptance criteria\n- [ ] Works";
        let (body, criteria) = split_ticket_description(description);
        assert!(body.ends_with("only as a line"), "{body}");
        assert_eq!(criteria.as_deref(), Some("## Acceptance criteria\n- [ ] Works"));
    }
}
