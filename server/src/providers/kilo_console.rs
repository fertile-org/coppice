use crate::providers::AgentRunResult;
use crate::sessions::opencode_events::extract_result_from_text;
use crate::sessions::{LiveMessage, run_registry::RunStreamHandle};
use serde_json::{json, Value};
use std::sync::Arc;

/// Structured live-console events for Kilo Code (rendered like the OpenCode session UI).
///
/// Kilo is a documented OpenCode fork and emits OpenCode-style JSON events under
/// `--format json`. The exact event schema for the installed Kilo version is not
/// verified in CI, so the publisher is defensive: it pulls assistant text from the
/// common `session.message` shape and falls back to top-level `text`/`content`
/// fields. Tool/auxiliary events are ignored for display.
pub struct KiloConsolePublisher {
    contract_published: bool,
}

impl Default for KiloConsolePublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl KiloConsolePublisher {
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
            "session.message" => self.handle_session_message(stream, value),
            _ => self.handle_generic_text(stream, value),
        }
    }

    fn handle_session_message(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(message) = value.get("properties").and_then(|p| p.get("message")) else {
            return;
        };
        let role = message
            .get("info")
            .and_then(|i| i.get("role"))
            .or_else(|| message.get("role"))
            .and_then(|r| r.as_str());
        if role != Some("assistant") {
            return;
        }
        let Some(parts) = message.get("parts").and_then(|p| p.as_array()) else {
            return;
        };
        for part in parts {
            let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if part_type != "text" && part_type != "reasoning" && part_type != "compaction" {
                continue;
            }
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                self.publish_text(stream, text);
            }
        }
    }

    fn handle_generic_text(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        // Fallback for simpler event shapes (e.g. {"type":"text","text":"..."} or
        // {"type":"content","content":"..."}). Tool events do not carry these
        // display fields, so this does not forward tool output as prose.
        let Some(text) = value
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("content").and_then(|v| v.as_str()))
        else {
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
                "type": "kilo.console.text",
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
                "type": "kilo.console.result",
                "contract": contract,
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
    fn publishes_assistant_text_from_session_message() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = KiloConsolePublisher::new();

        publisher.handle_json(
            &handle,
            &json!({
                "type": "session.message",
                "properties": {
                    "message": {
                        "info": {"role": "assistant"},
                        "parts": [
                            {"type": "text", "text": "Working on the task..."}
                        ]
                    }
                }
            }),
        );

        let mut events = Vec::new();
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            events.push(event);
        }
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "kilo.console.text");
        assert_eq!(events[0]["markdown"], "Working on the task...");
    }

    #[test]
    fn ignores_user_and_tool_messages() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = KiloConsolePublisher::new();

        publisher.handle_json(
            &handle,
            &json!({
                "type": "session.message",
                "properties": {
                    "message": {
                        "info": {"role": "user"},
                        "parts": [{"type": "text", "text": "user prompt"}]
                    }
                }
            }),
        );
        publisher.handle_json(
            &handle,
            &json!({"type": "tool", "name": "read", "path": ".agent/context.md"}),
        );

        let mut events = Vec::new();
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            events.push(event);
        }
        assert!(events.is_empty());
    }

    #[test]
    fn publishes_result_contract_once() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = KiloConsolePublisher::new();
        let contract = r#"{"status":"done","summary":"Done.","changedFiles":[],"testsRun":[],"blockers":[]}"#;

        publisher.handle_json(
            &handle,
            &json!({
                "type": "session.message",
                "properties": {
                    "message": {
                        "info": {"role": "assistant"},
                        "parts": [{"type": "text", "text": contract}]
                    }
                }
            }),
        );
        publisher.handle_json(
            &handle,
            &json!({
                "type": "session.message",
                "properties": {
                    "message": {
                        "info": {"role": "assistant"},
                        "parts": [{"type": "text", "text": contract}]
                    }
                }
            }),
        );

        let mut result_count = 0;
        while let Ok(LiveMessage::Event { event }) = rx.try_recv() {
            if event.get("type").and_then(|v| v.as_str()) == Some("kilo.console.result") {
                result_count += 1;
            }
        }
        assert_eq!(result_count, 1);
    }
}
