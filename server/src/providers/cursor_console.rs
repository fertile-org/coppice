use crate::providers::AgentRunResult;
use crate::sessions::opencode_events::extract_result_from_text;
use crate::sessions::{LiveMessage, run_registry::RunStreamHandle};
use serde_json::{json, Value};
use std::sync::Arc;

/// Structured live-console events for Cursor CLI (rendered like OpenCode session UI).
pub struct CursorConsolePublisher {
    contract_published: bool,
}

impl Default for CursorConsolePublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorConsolePublisher {
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
            "assistant" => self.handle_assistant(stream, value),
            "tool_call" => self.handle_tool_call(stream, value),
            "result" => self.handle_result(stream, value),
            // Ignore thinking, user, and unknown types.
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
                "type": "cursor.console.session",
                "model": model,
            }),
        );
    }

    fn handle_assistant(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(content) = value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            return;
        };
        for block in content {
            if block.get("type").and_then(|t| t.as_str()) != Some("text") {
                continue;
            }
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                self.publish_text(stream, text);
            }
        }
    }

    fn handle_tool_call(&self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(subtype) = value.get("subtype").and_then(|v| v.as_str()) else {
            return;
        };
        let Some(id) = value.get("call_id").and_then(|v| v.as_str()) else {
            return;
        };
        let Some(tool_call) = value.get("tool_call") else {
            return;
        };
        let Some((tool_key, payload)) = find_tool_payload(tool_call) else {
            return;
        };
        let (variant, title) = tool_title(tool_key, payload);

        match subtype {
            "started" => {
                emit(
                    stream,
                    json!({
                        "type": "cursor.console.tool",
                        "id": id,
                        "variant": variant,
                        "status": "running",
                        "title": title,
                    }),
                );
            }
            "completed" => {
                let status = completed_tool_status(payload);
                let mut event = json!({
                    "type": "cursor.console.tool",
                    "id": id,
                    "variant": variant,
                    "status": status,
                    "title": title,
                });
                if let Some(output) = tool_output(payload) {
                    event["output"] = json!(output);
                }
                emit(stream, event);
            }
            _ => {}
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
                "type": "cursor.console.text",
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
                "type": "cursor.console.result",
                "contract": contract,
            }),
        );
    }
}

fn emit(stream: &Arc<RunStreamHandle>, event: Value) {
    stream.publish(LiveMessage::Event { event });
}

fn find_tool_payload(tool_call: &Value) -> Option<(&str, &Value)> {
    let obj = tool_call.as_object()?;
    for (key, value) in obj {
        if key.ends_with("ToolCall") {
            return Some((key.as_str(), value));
        }
    }
    None
}

fn tool_title(tool_key: &str, payload: &Value) -> (&'static str, String) {
    let args = payload.get("args").unwrap_or(&Value::Null);
    if tool_key == "shellToolCall" {
        if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
            return ("shell", command.to_string());
        }
        return ("shell", tool_key.to_string());
    }
    if matches!(
        tool_key,
        "editToolCall" | "readToolCall" | "writeToolCall" | "deleteToolCall"
    ) {
        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            return ("action", format!("{tool_key} {path}"));
        }
    }
    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
        return ("action", format!("{tool_key} {path}"));
    }
    ("action", tool_key.to_string())
}

fn completed_tool_status(payload: &Value) -> &'static str {
    let Some(result) = payload.get("result") else {
        return "completed";
    };
    if result.get("error").is_some() || result.get("failure").is_some() {
        return "error";
    }
    if let Some(success) = result.get("success") {
        if let Some(code) = success.get("exitCode").and_then(|v| v.as_i64()) {
            if code != 0 {
                return "error";
            }
        }
        return "completed";
    }
    if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
        return "error";
    }
    "completed"
}

fn tool_output(payload: &Value) -> Option<String> {
    let result = payload.get("result")?;
    if let Some(success) = result.get("success") {
        if let Some(stdout) = success.get("stdout").and_then(|v| v.as_str()) {
            let trimmed = stdout.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Some(stderr) = success.get("stderr").and_then(|v| v.as_str()) {
            let trimmed = stderr.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(error) = result.get("error") {
        if let Some(message) = error
            .get("message")
            .and_then(|v| v.as_str())
            .or_else(|| error.as_str())
        {
            let trimmed = message.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::run_registry::RunStreamRegistry;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/cursor")
    }

    fn collect_events(
        handle: &std::sync::Arc<crate::sessions::run_registry::RunStreamHandle>,
    ) -> Vec<serde_json::Value> {
        handle
            .buffered_tail()
            .iter()
            .filter_map(|msg| match msg {
                crate::sessions::LiveMessage::Event { event } => Some(event.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn publishes_done_fixture() {
        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl")).unwrap();
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();
        for line in raw.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            console.handle_stream_json(&handle, &value);
        }
        let events = collect_events(&handle);
        assert_eq!(events[0]["type"], "cursor.console.session");
        assert_eq!(events[0]["model"], "composer-2.5");
        assert!(events.iter().any(|e| e["type"] == "cursor.console.text"));
        let result = events
            .iter()
            .find(|e| e["type"] == "cursor.console.result")
            .unwrap();
        assert_eq!(result["contract"]["summary"], "Implemented the feature.");
    }

    #[test]
    fn publishes_tool_lifecycle_from_agentic_fixture() {
        let raw = std::fs::read_to_string(fixtures_root().join("agentic.jsonl")).unwrap();
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();
        for line in raw.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            console.handle_stream_json(&handle, &value);
        }
        let events = collect_events(&handle);
        let tools: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "cursor.console.tool")
            .collect();
        assert!(tools.iter().any(|t| {
            t["status"] == "running" && t["title"].as_str().unwrap().contains("cargo test")
        }));
        assert!(tools
            .iter()
            .any(|t| t["status"] == "completed" && t["id"] == "tool_1"));
        assert!(tools
            .iter()
            .any(|t| t["title"].as_str().unwrap().contains("cursor.rs")));
        assert!(events.iter().any(|e| e["type"] == "cursor.console.result"));
    }

    #[test]
    fn ignores_thinking_and_user_events() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();
        console.handle_stream_json(
            &handle,
            &serde_json::json!({
                "type": "thinking", "subtype": "delta", "text": "hmm", "session_id": "s"
            }),
        );
        console.handle_stream_json(
            &handle,
            &serde_json::json!({
                "type": "user", "message": {"role": "user", "content": [{"type":"text","text":"hi"}]}, "session_id": "s"
            }),
        );
        assert!(collect_events(&handle).is_empty());
    }
}
