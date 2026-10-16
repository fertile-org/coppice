use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::chat_repos::{
    chat_dir, checkout_size_note, open_repo, OpenError, SLUG_NOT_A_PATH, SLUG_UNKNOWN,
};
use crate::services::chat_workspace::{self, limits_from_config};
use serde_json::{json, Value};
use std::path::PathBuf;

pub async fn call_open_repo(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let slug = args
        .get("slug")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let Some(session_id) = ctx.scope.chat_session_id else {
        return Err(ToolError::Denied(
            "open_repo is only available in agent chat.".into(),
        ));
    };
    let board_id = ctx.scope.board_id;
    let repos = chat_workspace::load_linked_repos(ctx.pool, board_id)
        .await
        .map_err(|err| ToolError::Internal(err.into()))?;
    let worktrees = PathBuf::from(&ctx.state.config.agent.worktrees_path);
    let limits = limits_from_config(&ctx.state.config.agent);
    let pool = ctx.pool.clone();
    let handle = tokio::runtime::Handle::current();
    let dir = chat_dir(&worktrees, session_id);

    let joined = tokio::task::spawn_blocking(move || {
        let mut notes = move |line: &str| {
            let line = line.to_string();
            let pool = pool.clone();
            handle.block_on(async move {
                if let Err(err) = crate::services::chat_service::ChatService::new(&pool)
                    .append_system_note(session_id, &line)
                    .await
                {
                    tracing::warn!(%session_id, error = %err, "failed to record open_repo note");
                }
            });
        };
        open_repo(&dir, session_id, &repos, &slug, &limits, &mut notes)
    })
    .await
    .map_err(|err| ToolError::Internal(err.into()))?;

    match joined {
        Ok(outcome) => Ok(json!({
            "slug": outcome.slug,
            "name": outcome.name,
            "path": outcome.path.display().to_string(),
            "commit": outcome.commit,
            "sizeBytes": outcome.size_bytes,
            "sizeNote": if outcome.size_note.is_empty() {
                checkout_size_note(outcome.size_bytes)
            } else {
                outcome.size_note
            },
            "elapsedMs": outcome.elapsed.as_millis() as u64,
            "reused": outcome.reused,
            "closed": outcome.closed_name,
        })),
        Err(OpenError::NotASlug) => Err(ToolError::InvalidArgs(SLUG_NOT_A_PATH.into())),
        Err(OpenError::UnknownSlug) => Err(ToolError::NotFound(SLUG_UNKNOWN.into())),
        Err(OpenError::Failed { name }) => Err(ToolError::InvalidArgs(
            crate::services::chat_repos::open_failed_line(&name),
        )),
    }
}
