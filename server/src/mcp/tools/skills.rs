use crate::mcp::catalog::object;
use crate::mcp::protocol::ToolDefinition;
use crate::mcp::tools::{ToolCtx, ToolError};
use serde_json::{json, Value};

pub const SKILL_LIST: &str = "skill_list";
pub const SKILL_LOAD: &str = "skill_load";

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: SKILL_LIST.to_string(),
            description: "List skills available to this run (name, description).".to_string(),
            input_schema: object(json!({}), &[]),
            read_only: true,
        },
        ToolDefinition {
            name: SKILL_LOAD.to_string(),
            description: "Load a skill's instructions and its absolute folder path.".to_string(),
            input_schema: object(json!({ "name": { "type": "string" } }), &["name"]),
            read_only: true,
        },
    ]
}

pub async fn dispatch(name: &str, ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    match name {
        SKILL_LIST => call_skill_list(ctx, args).await,
        SKILL_LOAD => call_skill_load(ctx, args).await,
        other => Err(ToolError::Internal(anyhow::anyhow!(
            "unknown skill tool {other}"
        ))),
    }
}

pub async fn call_skill_list(ctx: &ToolCtx<'_>, _args: Value) -> Result<Value, ToolError> {
    let skills: Vec<Value> = ctx
        .state
        .skills
        .skills_for(&ctx.scope.plugin_ids)
        .into_iter()
        .map(|s| json!({ "id": s.id, "description": s.description }))
        .collect();
    Ok(json!({ "skills": skills }))
}

pub async fn call_skill_load(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ToolError::InvalidArgs("name is required".into()))?;
    let (info, body) = ctx
        .state
        .skills
        .get(&ctx.scope.plugin_ids, name)
        .ok_or_else(|| ToolError::NotFound(format!("skill \"{name}\" not found")))?;
    Ok(json!({
        "id": info.id,
        "path": info.path.display().to_string(),
        "body": body,
    }))
}
