use crate::knowledge::retrieval::retrieve;
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::context_budget::{record_usage, render_knowledge, ByteTokenCounter};
use serde_json::{json, Value};
use uuid::Uuid;

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 20;
const MAX_QUERY_CHARS: usize = 2_000;

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

/// Searches approved knowledge visible to the run's agent on its board.
/// Retrieval already restricts to approved, live, in-scope entries.
pub async fn call_knowledge_search(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| ToolError::InvalidArgs("query is required".into()))?;
    let limit = parse_limit(&args)?;

    let Some(board_id) = ctx.scope.board_id else {
        return Ok(json!({ "items": [] }));
    };
    if !ctx.state.config.knowledge.enabled {
        return Ok(json!({ "items": [] }));
    }

    let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
    let mut found = retrieve(
        ctx.pool,
        board_id,
        ctx.scope.agent_id,
        &query,
        "",
        &ctx.state.config.knowledge.retrieval,
    )
    .await?;
    found.truncate(limit);

    log_usage(ctx, &found).await?;

    let items: Vec<Value> = found
        .iter()
        .map(|k| {
            json!({
                "id": k.item_id,
                "revisionId": k.revision_id,
                "title": k.title,
                "type": k.knowledge_type,
                "content": k.content,
            })
        })
        .collect();
    Ok(json!({ "items": items }))
}

/// Records each returned revision once per run (the context file and earlier
/// searches may already have logged some).
async fn log_usage(
    ctx: &ToolCtx<'_>,
    found: &[crate::knowledge::retrieval::RetrievedKnowledge],
) -> Result<(), ToolError> {
    if found.is_empty() {
        return Ok(());
    }
    let logged: Vec<Uuid> =
        sqlx::query_scalar("SELECT revision_id FROM knowledge_usage_logs WHERE run_id = $1")
            .bind(ctx.scope.run_id)
            .fetch_all(ctx.pool)
            .await?;
    let existing = i32::try_from(logged.len()).unwrap_or(i32::MAX);
    let fresh: Vec<_> = found
        .iter()
        .filter(|k| !logged.contains(&k.revision_id))
        .cloned()
        .collect();
    if fresh.is_empty() {
        return Ok(());
    }
    // The tool returns whole entries, so render without a token cap.
    let mut section = render_knowledge(&fresh, usize::MAX, &ByteTokenCounter);
    for entry in &mut section.entries {
        entry.rank = entry.rank.saturating_add(existing);
    }
    record_usage(ctx.pool, ctx.scope.run_id, &section.entries).await?;
    Ok(())
}
