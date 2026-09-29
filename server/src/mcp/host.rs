use crate::mcp::catalog::{core_tools_for, CoreTool};
use crate::mcp::protocol::{ToolDefinition, ToolHost, ToolOutput};
use crate::mcp::token::RunToolScope;
use crate::mcp::tools::{self, ToolCtx, ToolError};
use crate::AppState;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

const TRUNCATION_SUFFIX: &str = "\n…[truncated: use limit/before to page]";
const ARGS_SUMMARY_CHARS: usize = 200;

pub struct RunToolHost {
    state: Arc<AppState>,
    scope: RunToolScope,
}

#[derive(Clone, Copy)]
enum CallStatus {
    Ok,
    Error,
    Denied,
    Timeout,
}

impl CallStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Denied => "denied",
            Self::Timeout => "timeout",
        }
    }
}

impl RunToolHost {
    pub fn new(state: Arc<AppState>, scope: RunToolScope) -> Self {
        Self { state, scope }
    }

    async fn run_handler(
        &self,
        tool: CoreTool,
        args: Value,
    ) -> (CallStatus, ToolOutput, Option<String>) {
        let Some(pool) = self.state.db.as_ref() else {
            tracing::error!(tool = tool.name(), "mcp tool call without database");
            return error_output(CallStatus::Error, "internal error");
        };
        let ctx = ToolCtx {
            state: &self.state,
            pool,
            scope: &self.scope,
        };
        let timeout = Duration::from_secs(self.state.config.mcp.call_timeout_secs);
        match tokio::time::timeout(timeout, tools::dispatch(tool, &ctx, args)).await {
            Err(_) => error_output(CallStatus::Timeout, "timeout"),
            Ok(Err(e)) => {
                if let ToolError::Internal(inner) = &e {
                    tracing::error!(tool = tool.name(), run_id = %self.scope.run_id, error = ?inner, "mcp tool failed");
                }
                let message = e.message();
                error_output(CallStatus::Error, &message)
            }
            Ok(Ok(value)) => {
                let text =
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
                (
                    CallStatus::Ok,
                    ToolOutput {
                        text: truncate_output(text, self.state.config.mcp.max_output_bytes),
                        is_error: false,
                    },
                    None,
                )
            }
        }
    }

    async fn log_call(
        &self,
        tool: &str,
        source: &str,
        args: &Value,
        status: CallStatus,
        error: Option<&str>,
        started: Instant,
    ) {
        let Some(pool) = self.state.db.as_ref() else {
            return;
        };
        let duration_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);
        let result = sqlx::query(
            r#"
            INSERT INTO run_tool_calls (id, run_id, tool, source, args_summary, status, error, duration_ms)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(self.scope.run_id)
        .bind(tool)
        .bind(source)
        .bind(summarize_args(args))
        .bind(status.as_str())
        .bind(error)
        .bind(duration_ms)
        .execute(pool)
        .await;
        if let Err(e) = result {
            tracing::warn!(run_id = %self.scope.run_id, tool, error = %e, "failed to log mcp tool call");
        }
    }
}

fn error_output(status: CallStatus, message: &str) -> (CallStatus, ToolOutput, Option<String>) {
    (
        status,
        ToolOutput {
            text: message.to_string(),
            is_error: true,
        },
        Some(message.to_string()),
    )
}

fn summarize_args(args: &Value) -> String {
    args.to_string().chars().take(ARGS_SUMMARY_CHARS).collect()
}

fn truncate_output(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &text[..end], TRUNCATION_SUFFIX)
}

#[async_trait]
impl ToolHost for RunToolHost {
    fn list(&self) -> Vec<ToolDefinition> {
        core_tools_for(self.scope.profile)
            .into_iter()
            .map(CoreTool::definition)
            .collect()
    }

    async fn call(&self, name: &str, args: Value) -> ToolOutput {
        let started = Instant::now();
        let allowed = core_tools_for(self.scope.profile)
            .into_iter()
            .find(|t| t.name() == name);
        let Some(tool) = allowed else {
            let message = format!("denied: tool \"{name}\" is not available for this run");
            self.log_call(
                name,
                "core",
                &args,
                CallStatus::Denied,
                Some(&message),
                started,
            )
            .await;
            return ToolOutput {
                text: message,
                is_error: true,
            };
        };

        let (status, output, error) = self.run_handler(tool, args.clone()).await;
        let source = if tool.is_skill() { "skill" } else { "core" };
        self.log_call(name, source, &args, status, error.as_deref(), started)
            .await;
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_output_respects_limit_and_char_boundaries() {
        assert_eq!(truncate_output("short".into(), 10), "short");
        let out = truncate_output("ééé".into(), 3);
        assert_eq!(out, format!("é{TRUNCATION_SUFFIX}"));
    }

    #[test]
    fn summarize_args_is_capped() {
        let args = serde_json::json!({ "body": "x".repeat(500) });
        assert_eq!(summarize_args(&args).chars().count(), ARGS_SUMMARY_CHARS);
    }
}
