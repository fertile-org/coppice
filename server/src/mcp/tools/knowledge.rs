use crate::domain::context_profile::ContextProfile;
use crate::knowledge::freshness::{freshness_for, Freshness};
use crate::knowledge::retrieval::{fetch_visible, retrieve, RetrievalScope};
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::context_budget::{
    record_usage, render_knowledge, ByteTokenCounter, RenderedKnowledge, KNOWLEDGE_DATA_NOTE,
};
use serde_json::{json, Value};
use uuid::Uuid;

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 20;
const MAX_QUERY_CHARS: usize = 2_000;
const SNIPPET_CHARS: usize = 200;
const MAX_GET_IDS: usize = 5;
const VERIFY_NOTE: &str = "Cited code changed since this was written. Check it before relying on it, then call knowledge_flag.";

fn parse_limit(args: &Value) -> Result<usize, ToolError> {
    match args.get("limit").filter(|v| !v.is_null()) {
        None => Ok(DEFAULT_LIMIT),
        Some(v) => {
            let n = v
                .as_i64()
                .ok_or_else(|| ToolError::InvalidArgs("limit must be an integer".into()))?;
            Ok(usize::try_from(n.clamp(1, MAX_LIMIT as i64)).unwrap_or(DEFAULT_LIMIT))
        }
    }
}

/// Searches approved knowledge visible to the run's agent.
/// Conversation chat searches every board. Compaction runs search the boards of
/// their batch tickets. Other runs stay on their board, or return nothing when
/// they have none. Retrieval already restricts to approved,
/// live, in-scope entries, including agent-scoped notes for this agent only.
pub async fn call_knowledge_search(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| ToolError::InvalidArgs("query is required".into()))?;
    let limit = parse_limit(&args)?;

    if !ctx.state.config.knowledge.enabled {
        return Ok(response(Vec::new()));
    }

    let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
    let workspace = ctx.scope.workspace_chat();
    let Some(scope) = tool_scope(ctx).await? else {
        return Ok(response(Vec::new()));
    };
    let mut found = retrieve(
        ctx.pool,
        &scope,
        ctx.scope.agent_id,
        &query,
        "",
        &ctx.state.config.knowledge.retrieval,
    )
    .await?;
    // Retrieval returns the whole eligible set; non-matches carry score 0.
    found.retain(|k| k.score > 0.0);
    found.truncate(limit);

    // Rows carry a snippet and the token cost of reading the whole entry with
    // `knowledge_get`; usage is logged there, not here.
    let section = render_knowledge(&found, usize::MAX, &ByteTokenCounter);
    let revision_ids: Vec<Uuid> = found.iter().map(|k| k.revision_id).collect();
    let freshness = freshness_for(ctx.pool, &revision_ids).await;
    let items: Vec<Value> = found
        .iter()
        .zip(&section.entries)
        .map(|(k, entry)| {
            let mut item = json!({
                "id": k.item_id,
                "title": k.title,
                "type": k.knowledge_type,
                "snippet": k.content.chars().take(SNIPPET_CHARS).collect::<String>(),
                "tokens": entry.token_count,
                "freshness": freshness.get(&k.revision_id).map_or(Freshness::Unknown, |r| r.state),
            });
            if workspace {
                item["boardName"] = json!(k.board_name.as_deref());
            }
            item
        })
        .collect();
    Ok(response(items))
}

/// Returns whole entries for up to five ids from `knowledge_search`, and logs
/// each revision once per run. An id the run cannot see fails the call the
/// same way an unknown id does.
pub async fn call_knowledge_get(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let ids = parse_ids(&args)?;
    let not_found = || ToolError::NotFound("knowledge item not found".into());
    if !ctx.state.config.knowledge.enabled {
        return Err(not_found());
    }
    let Some(scope) = tool_scope(ctx).await? else {
        return Err(not_found());
    };
    let found = fetch_visible(
        ctx.pool,
        &scope,
        ctx.scope.agent_id,
        &ids,
        &ctx.state.config.knowledge.retrieval,
    )
    .await?;
    if found.len() != ids.len() {
        return Err(not_found());
    }
    let section = render_knowledge(&found, usize::MAX, &ByteTokenCounter);
    log_usage(ctx, &section.entries).await?;
    let workspace = ctx.scope.workspace_chat();
    let revision_ids: Vec<Uuid> = found.iter().map(|k| k.revision_id).collect();
    let freshness = freshness_for(ctx.pool, &revision_ids).await;
    let items: Vec<Value> = found
        .iter()
        .zip(&section.entries)
        .map(|(k, entry)| {
            let report = freshness.get(&k.revision_id);
            let state = report.map_or(Freshness::Unknown, |r| r.state);
            let mut item = json!({
                "id": k.item_id,
                "revisionId": k.revision_id,
                "title": k.title,
                "type": k.knowledge_type,
                "content": entry.rendered_content,
                "freshness": state,
                "changedPaths": report.map_or(&[][..], |r| r.paths.as_slice()),
            });
            if matches!(state, Freshness::Changed | Freshness::Missing) {
                item["verify"] = json!(VERIFY_NOTE);
            }
            if workspace {
                item["boardName"] = json!(k.board_name.as_deref());
            }
            item
        })
        .collect();
    Ok(response(items))
}

fn parse_ids(args: &Value) -> Result<Vec<Uuid>, ToolError> {
    let raw = args
        .get("ids")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ToolError::InvalidArgs("ids must be a non-empty array".into()))?;
    if raw.len() > MAX_GET_IDS {
        return Err(ToolError::InvalidArgs(format!(
            "ids takes at most {MAX_GET_IDS} entries"
        )));
    }
    let mut ids: Vec<Uuid> = Vec::with_capacity(raw.len());
    for value in raw {
        let id = value
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| ToolError::InvalidArgs("ids must be knowledge item ids".into()))?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// The boards a run's knowledge tools may read; `None` when it has no board.
pub(crate) async fn tool_scope(ctx: &ToolCtx<'_>) -> Result<Option<RetrievalScope>, ToolError> {
    if ctx.scope.workspace_chat() {
        return Ok(Some(RetrievalScope::AllBoards));
    }
    if ctx.scope.profile == ContextProfile::KnowledgeCompaction {
        let boards: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT board_id FROM tickets WHERE id = ANY($1) AND board_id IS NOT NULL",
        )
        .bind(&ctx.scope.compaction_ticket_ids)
        .fetch_all(ctx.pool)
        .await?;
        return Ok(Some(RetrievalScope::Boards(boards)));
    }
    Ok(ctx.scope.board_id.map(|id| RetrievalScope::Boards(vec![id])))
}

fn response(items: Vec<Value>) -> Value {
    json!({ "note": KNOWLEDGE_DATA_NOTE, "items": items })
}

/// Records each returned revision once per run (earlier searches may already
/// have logged some).
async fn log_usage(ctx: &ToolCtx<'_>, entries: &[RenderedKnowledge]) -> Result<(), ToolError> {
    if entries.is_empty() {
        return Ok(());
    }
    let logged: Vec<Uuid> =
        sqlx::query_scalar("SELECT revision_id FROM knowledge_usage_logs WHERE run_id = $1")
            .bind(ctx.scope.run_id)
            .fetch_all(ctx.pool)
            .await?;
    let existing = i32::try_from(logged.len()).unwrap_or(i32::MAX);
    let fresh: Vec<RenderedKnowledge> = entries
        .iter()
        .filter(|e| !logged.contains(&e.revision_id))
        .enumerate()
        .map(|(i, e)| RenderedKnowledge {
            rank: existing.saturating_add(i32::try_from(i + 1).unwrap_or(i32::MAX)),
            ..e.clone()
        })
        .collect();
    if fresh.is_empty() {
        return Ok(());
    }
    record_usage(ctx.pool, ctx.scope.run_id, &fresh).await?;
    Ok(())
}
