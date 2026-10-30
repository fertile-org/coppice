use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::design_service::{CreateScope, DesignError, DesignOption, DesignService};
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;

pub async fn dispatch(name: &str, ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let scope = create_scope(ctx)?;
    let svc = DesignService::new(ctx.pool, &ctx.state.config.storage.artifacts_dir);
    match name {
        "design__create" => {
            let title = required_str(&args, "title")?;
            let created = svc.create(title, scope).await.map_err(map_err)?;
            let dir = PathBuf::from(&ctx.state.config.storage.artifacts_dir).join(&created.dir);
            Ok(json!({ "setId": created.id, "dir": dir.display().to_string() }))
        }
        "design__write" => {
            let set_id = required_uuid(&args, "setId")?;
            let path = required_str(&args, "path")?;
            let content = required_str(&args, "content")?;
            svc.write(set_id, ctx.scope.run_id, path, content.as_bytes())
                .await
                .map_err(map_err)?;
            Ok(json!({ "path": path }))
        }
        "design__publish" => {
            let set_id = required_uuid(&args, "setId")?;
            let options = args
                .get("options")
                .cloned()
                .ok_or_else(|| ToolError::InvalidArgs("options is required".into()))?;
            let options: Vec<DesignOption> = serde_json::from_value(options)
                .map_err(|err| ToolError::InvalidArgs(err.to_string()))?;
            let id = svc
                .publish(set_id, ctx.scope.run_id, options)
                .await
                .map_err(map_err)?;
            Ok(json!({ "setId": id }))
        }
        "design__list" => {
            let sets = if let Some(ticket_id) = scope.ticket_id {
                svc.list_for_ticket(ticket_id).await
            } else if let Some(chat_session_id) = scope.chat_session_id {
                svc.list_for_chat(chat_session_id).await
            } else {
                return Err(ToolError::InvalidArgs(
                    "design set needs a ticket or a chat, not both".into(),
                ));
            }
            .map_err(map_err)?;
            let sets: Vec<Value> = sets
                .into_iter()
                .map(|set| {
                    let chosen = set
                        .chosen
                        .as_ref()
                        .map(|chosen| json!({ "id": chosen.id, "title": chosen.title }));
                    json!({
                        "id": set.id,
                        "title": set.title,
                        "options": set.options,
                        "chosen": chosen,
                    })
                })
                .collect();
            Ok(json!({ "sets": sets }))
        }
        other => Err(ToolError::Internal(anyhow::anyhow!(
            "unknown design tool {other}"
        ))),
    }
}

fn create_scope(ctx: &ToolCtx<'_>) -> Result<CreateScope, ToolError> {
    let (ticket_id, chat_session_id) = match (ctx.scope.ticket_id, ctx.scope.chat_session_id) {
        (Some(ticket_id), None) => (Some(ticket_id), None),
        (None, Some(chat_session_id)) => (None, Some(chat_session_id)),
        _ => {
            return Err(ToolError::InvalidArgs(
                "design set needs a ticket or a chat, not both".into(),
            ));
        }
    };
    Ok(CreateScope {
        ticket_id,
        chat_session_id,
        run_id: ctx.scope.run_id,
        agent_id: ctx.scope.agent_id,
    })
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgs(format!("{key} is required")))
}

fn required_uuid(args: &Value, key: &str) -> Result<Uuid, ToolError> {
    let raw = required_str(args, key)?;
    Uuid::parse_str(raw).map_err(|_| ToolError::InvalidArgs(format!("{key} must be a uuid")))
}

fn map_err(err: DesignError) -> ToolError {
    match err {
        DesignError::Invalid(message) => ToolError::InvalidArgs(message),
        DesignError::NotFound => ToolError::NotFound("design set not found".into()),
        DesignError::Denied => ToolError::Denied("denied".into()),
        DesignError::Database(err) => ToolError::Internal(err.into()),
    }
}
