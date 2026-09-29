use crate::domain::comment::{author_type_to_str, AuthorType, CommentIntent};
use crate::events::bus::AppEvent;
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::comment_service::{CommentError, CommentService};
use serde_json::{json, Value};

/// Posts a progress comment on the run's own ticket. The target is always the
/// token's ticket; any `ticketId` argument is ignored.
pub async fn call_comment_post(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let ticket_id = ctx
        .scope
        .ticket_id
        .ok_or_else(|| ToolError::Denied("comment_post is only available on ticket runs".into()))?;
    let body = args
        .get("body")
        .and_then(Value::as_str)
        .filter(|b| !b.trim().is_empty())
        .ok_or_else(|| ToolError::InvalidArgs("body is required".into()))?;

    let posted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run_tool_calls \
         WHERE run_id = $1 AND tool = 'comment_post' AND status = 'ok'",
    )
    .bind(ctx.scope.run_id)
    .fetch_one(ctx.pool)
    .await?;
    if posted >= i64::from(ctx.state.config.mcp.comment_post_limit) {
        return Err(ToolError::Limit(
            "comment_post limit reached for this run".into(),
        ));
    }

    let comment = CommentService::new(ctx.pool)
        .create(
            ticket_id,
            AuthorType::Agent,
            Some(ctx.scope.agent_id),
            body,
            CommentIntent::ProgressUpdate,
            &[],
            &[],
        )
        .await
        .map_err(|e| match e {
            CommentError::Validation(m) => ToolError::InvalidArgs(m),
            CommentError::TicketNotFound => ToolError::NotFound("ticket not found".into()),
            other => ToolError::Internal(other.into()),
        })?;

    ctx.state.event_bus.publish(AppEvent::CommentCreated {
        comment_id: comment.id,
        ticket_id,
        author_type: author_type_to_str(comment.author_type).into(),
    });

    Ok(json!({ "commentId": comment.id }))
}
