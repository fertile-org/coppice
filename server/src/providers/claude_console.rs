use crate::providers::AgentRunResult;
use crate::sessions::opencode_events::extract_result_from_text;
use crate::sessions::{LiveMessage, run_registry::RunStreamHandle};
use serde_json::{json, Value};
use std::sync::Arc;

/// Structured live-console events for Claude Code (rendered like OpenCode session UI).
pub struct ClaudeConsolePublisher {
    contract_published: bool,
}

impl ClaudeConsolePublisher {
    pub fn new() -> Self {
        Self {
            contract_published: false,
        }
    }

    pub fn handle_stream_json(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(ty) = value.get("type").and_then(|v| v.as_str()) else {
            return;
        };
        match ty {
            "system" => self.handle_system(stream, value),
            "assistant" | "user" => self.handle_message(stream, value),
            "result" => self.handle_result(stream, value),
            _ => {}
        }
    }

    fn handle_system(&self, stream: &Arc<RunStreamHandle>, value: &Value) {
        if value.get("subtype").and_then(|v| v.as_str()) != Some("init") {
            return;
        }
        let model = value
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        emit(
            stream,
            json!({
                "type": "claude.console.session",
                "model": model,
            }),
        );
    }

    fn handle_message(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(content) = value.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array())
        else {
            return;
        };
        for block in content {
            let Some(block_ty) = block.get("type").and_then(|t| t.as_str()) else {
                continue;
            };
            match block_ty {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        self.publish_text(stream, text);
                    }
                }
                "tool_use" => self.publish_tool_start(stream, block),
                "tool_result" => self.publish_tool_result(stream, block),
                "thinking" => {
                    if let Some(text) = block.get("thinking").and_then(|v| v.as_str()) {
                        if !text.trim().is_empty() {
                            emit(
                                stream,
                                json!({
                                    "type": "claude.console.thinking",
                                    "text": text,
                                }),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_result(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(text) = value.get("result").and_then(|v| v.as_str()) else {
            return;
        };
        self.publish_text(stream, text);
    }

    fn publish_text(&mut self, stream: &Arc<RunStreamHandle>, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Some(result) = extract_result_from_text(trimmed) {
            self.publish_result(stream, &result);
            return;
        }
        emit(
            stream,
            json!({
                "type": "claude.console.text",
                "markdown": trimmed,
            }),
        );
    }

    fn publish_result(&mut self, stream: &Arc<RunStreamHandle>, result: &AgentRunResult) {
        if self.contract_published {
            return;
        }
        self.contract_published = true;
        let contract = serde_json::to_value(result).unwrap_or(json!({}));
        emit(
            stream,
            json!({
                "type": "claude.console.result",
                "contract": contract,
            }),
        );
    }

    fn publish_tool_start(&self, stream: &Arc<RunStreamHandle>, block: &Value) {
        let Some(id) = block.get("id").and_then(|v| v.as_str()) else {
            return;
        };
        let Some(name) = block.get("name").and_then(|v| v.as_str()) else {
            return;
        };
        let input = block.get("input").unwrap_or(&Value::Null);
        let (variant, title) = tool_title(name, input);
        emit(
            stream,
            json!({
                "type": "claude.console.tool",
                "id": id,
                "variant": variant,
                "status": "running",
                "title": title,
            }),
        );
    }

    fn publish_tool_result(&self, stream: &Arc<RunStreamHandle>, block: &Value) {
        let Some(id) = block
            .get("tool_use_id")
            .and_then(|v| v.as_str())
            .or_else(|| block.get("id").and_then(|v| v.as_str()))
        else {
            return;
        };
        let is_error = block.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
        let output = tool_result_text(block.get("content"));
        emit(
            stream,
            json!({
                "type": "claude.console.tool",
                "id": id,
                "status": if is_error { "error" } else { "completed" },
                "output": output,
            }),
        );
    }
}

fn emit(stream: &Arc<RunStreamHandle>, event: Value) {
    stream.publish(LiveMessage::Event { event });
}

fn tool_title(name: &str, input: &Value) -> (&'static str, String) {
    if name == "Bash" {
        if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
            return ("shell", command.to_string());
        }
    }
    let detail = match name {
        "Read" | "Write" | "Edit" | "MultiEdit" | "Glob" | "Grep" | "NotebookEdit" => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(|v| v.as_str())
            .map(|p| format!("{name} {p}")),
        "WebSearch" | "WebFetch" => input
            .get("url")
            .or_else(|| input.get("query"))
            .and_then(|v| v.as_str())
            .map(|target| format!("{name} {target}")),
        _ => None,
    };
    ("action", detail.unwrap_or_else(|| name.to_string()))
}

fn tool_result_text(content: Option<&Value>) -> Option<String> {
    let content = content?;
    if let Some(text) = content.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }
    if let Some(parts) = content.as_array() {
        let mut text = String::new();
        for part in parts {
            if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                    text.push_str(t);
                }
            }
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::run_registry::RunStreamRegistry;

    #[test]
    fn publishes_session_and_tool_events() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = ClaudeConsolePublisher::new();

        publisher.handle_stream_json(
            &handle,
            &json!({
                "type": "system",
                "subtype": "init",
                "model": "claude-opus-4",
            }),
        );
        publisher.handle_stream_json(
            &handle,
            &json!({
                "type": "assistant",
                "message": {
                    "content": [{
                        "type": "tool_use",
                        "id": "tu_1",
                        "name": "Read",
                        "input": {"file_path": "src/main.rs"}
                    }]
                }
            }),
        );

        let mut events = Vec::new();
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            events.push(event);
        }
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["type"], "claude.console.session");
        assert_eq!(events[1]["type"], "claude.console.tool");
        assert_eq!(events[1]["title"], "Read src/main.rs");
    }

    #[test]
    fn duplicate_result_contract_is_skipped() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = ClaudeConsolePublisher::new();
        let contract = r#"{"status":"done","summary":"Done.","changedFiles":[],"testsRun":[],"blockers":[]}"#;

        publisher.handle_stream_json(
            &handle,
            &json!({
                "type": "assistant",
                "message": {"content": [{"type": "text", "text": contract}]}
            }),
        );
        publisher.handle_stream_json(
            &handle,
            &json!({"type": "result", "result": contract}),
        );

        let mut result_count = 0;
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            if event.get("type").and_then(|v| v.as_str()) == Some("claude.console.result") {
                result_count += 1;
            }
        }
        assert_eq!(result_count, 1);
    }
}
