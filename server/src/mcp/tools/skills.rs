use crate::mcp::tools::{ToolCtx, ToolError};
use serde_json::{json, Value};

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
