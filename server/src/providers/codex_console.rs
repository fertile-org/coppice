use crate::providers::AgentRunResult;
use crate::sessions::opencode_events::extract_result_from_text;
use crate::sessions::{LiveMessage, run_registry::RunStreamHandle};
use serde_json::{json, Value};
use std::sync::Arc;

/// Structured live-console events for Codex (rendered like OpenCode session UI).
pub struct CodexConsolePublisher {
    contract_published: bool,
}

impl Default for CodexConsolePublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexConsolePublisher {
    pub fn new() -> Self {
        Self {
            contract_published: false,
        }
    }

    pub fn handle_json(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(ty) = value.get("type").and_then(|v| v.as_str()) else {
            return;
        };
        match ty {
            "thread.started" => self.handle_thread_started(stream, value),
            "item.completed" => self.handle_item_completed(stream, value),
            _ => {}
        }
    }

    fn handle_thread_started(&self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(thread_id) = value.get("thread_id").and_then(|v| v.as_str()) else {
            return;
        };
        let model = value
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        emit(
            stream,
            json!({
                "type": "codex.console.session",
                "threadId": thread_id,
                "model": model,
            }),
        );
    }

    fn handle_item_completed(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(item) = value.get("item") else {
            return;
        };
        let Some(item_ty) = item.get("type").and_then(|v| v.as_str()) else {
            return;
        };
        match item_ty {
            "agent_message" => {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    self.publish_text(stream, text);
                }
            }
            "command_execution" => {
                self.handle_command_result(stream, item);
            }
            _ => {}
        }
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
                "type": "codex.console.text",
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
                "type": "codex.console.result",
                "contract": contract,
            }),
        );
    }

    fn handle_command_result(&self, stream: &Arc<RunStreamHandle>, item: &Value) {
        let Some(command) = item.get("command").and_then(|v| v.as_str()) else {
            return;
        };
        let exit_code = item.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
        let status = if exit_code == 0 { "completed" } else { "error" };
        let output = item
            .get("aggregated_output")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        emit(
            stream,
            json!({
                "type": "codex.console.tool",
                "variant": "shell",
                "title": command,
                "status": status,
                "output": output,
            }),
        );
    }
}

fn emit(stream: &Arc<RunStreamHandle>, event: Value) {
    stream.publish(LiveMessage::Event { event });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::run_registry::RunStreamRegistry;

    #[test]
    fn publishes_session_and_text_events() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = CodexConsolePublisher::new();

        publisher.handle_json(
            &handle,
            &json!({
                "type": "thread.started",
                "thread_id": "thread_123",
                "model": "gpt-4o"
            }),
        );
        publisher.handle_json(
            &handle,
            &json!({
                "type": "item.completed",
                "item": {
                    "id": "item_0",
                    "type": "agent_message",
                    "text": "Working on the task..."
                }
            }),
        );

        let mut events = Vec::new();
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            events.push(event);
        }
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["type"], "codex.console.session");
        assert_eq!(events[0]["threadId"], "thread_123");
        assert_eq!(events[1]["type"], "codex.console.text");
    }

    #[test]
    fn publishes_command_execution_result() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = CodexConsolePublisher::new();

        publisher.handle_json(
            &handle,
            &json!({
                "type": "item.completed",
                "item": {
                    "id": "item_1",
                    "type": "command_execution",
                    "command": "cargo test",
                    "exit_code": 0,
                    "aggregated_output": "test result: passed"
                }
            }),
        );

        let mut events = Vec::new();
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            events.push(event);
        }
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "codex.console.tool");
        assert_eq!(events[0]["variant"], "shell");
        assert_eq!(events[0]["status"], "completed");
    }

    #[test]
    fn extracts_result_contract_from_text() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = CodexConsolePublisher::new();

        let contract = r#"{"status":"done","summary":"Done.","changedFiles":[],"testsRun":[],"blockers":[]}"#;
        publisher.handle_json(
            &handle,
            &json!({
                "type": "item.completed",
                "item": {
                    "id": "item_0",
                    "type": "agent_message",
                    "text": contract
                }
            }),
        );

        let mut events = Vec::new();
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            events.push(event);
        }
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "codex.console.result");
        assert_eq!(events[0]["contract"]["status"], "done");
    }

    #[test]
    fn duplicate_result_contract_is_skipped() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = CodexConsolePublisher::new();
        let contract = r#"{"status":"done","summary":"Done.","changedFiles":[],"testsRun":[],"blockers":[]}"#;

        publisher.handle_json(
            &handle,
            &json!({
                "type": "item.completed",
                "item": {
                    "id": "item_0",
                    "type": "agent_message",
                    "text": contract
                }
            }),
        );
        publisher.handle_json(
            &handle,
            &json!({
                "type": "item.completed",
                "item": {
                    "id": "item_1",
                    "type": "agent_message",
                    "text": contract
                }
            }),
        );

        let mut result_count = 0;
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            if event.get("type").and_then(|v| v.as_str()) == Some("codex.console.result") {
                result_count += 1;
            }
        }
        assert_eq!(result_count, 1);
    }
}
