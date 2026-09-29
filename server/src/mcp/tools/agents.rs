use crate::domain::slug::slugify;
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::agent_service::AgentService;
use serde_json::{json, Value};

pub async fn call_board_agents(ctx: &ToolCtx<'_>, _args: Value) -> Result<Value, ToolError> {
    let agents = AgentService::new(ctx.pool)
        .list_agents()
        .await
        .map_err(|e| ToolError::Internal(e.into()))?;
    let items: Vec<Value> = agents
        .into_iter()
        .filter(|a| a.enabled)
        .map(|a| {
            let key = a.preset_source.clone().unwrap_or_else(|| slugify(&a.name));
            json!({ "key": key, "name": a.name, "role": a.role })
        })
        .collect();
    Ok(json!({ "agents": items }))
}
