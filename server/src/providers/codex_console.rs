use crate::providers::AgentRunResult;
use crate::sessions::opencode_events::extract_result_from_text;
use crate::sessions::{run_registry::RunStreamHandle, LiveMessage};
use serde_json::{json, Value};
use std::sync::Arc;

/// Structured live-console events for Codex (rendered like OpenCode session UI).
pub struct CodexConsolePublisher {
    contract_published: bool,
    synthetic_error_id: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemLifecycle {
    Started,
    Updated,
    Completed,
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
            synthetic_error_id: 0,
        }
    }

    pub fn handle_json(&mut self, stream: &Arc<RunStreamHandle>, value: &Value) {
        let Some(ty) = value.get("type").and_then(|v| v.as_str()) else {
            return;
        };
        match ty {
            "thread.started" => self.handle_thread_started(stream, value),
            "item.started" => self.handle_item_event(stream, value, ItemLifecycle::Started, ty),
            "item.updated" => self.handle_item_event(stream, value, ItemLifecycle::Updated, ty),
            "item.completed" => self.handle_item_event(stream, value, ItemLifecycle::Completed, ty),
            "error" => self.handle_stream_error(stream, value, "Codex error", ty),
            "turn.failed" => {
                let error = value.get("error").unwrap_or(&Value::Null);
                self.handle_stream_error(stream, error, "Codex turn failed", ty);
            }
            "turn.started" | "turn.completed" => {}
            event_type => {
                tracing::debug!(
                    target: "codex.console",
                    event_type,
                    "ignoring unsupported Codex event type"
                );
            }
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

    fn handle_item_event(
        &mut self,
        stream: &Arc<RunStreamHandle>,
        value: &Value,
        lifecycle: ItemLifecycle,
        event_type: &str,
    ) {
        let Some(item) = value.get("item").filter(|item| item.is_object()) else {
            tracing::debug!(
                target: "codex.console",
                event_type,
                "ignoring malformed Codex item event"
            );
            return;
        };
        let Some(item_ty) = item.get("type").and_then(|v| v.as_str()) else {
            tracing::debug!(
                target: "codex.console",
                event_type,
                "ignoring Codex item event without an item type"
            );
            return;
        };
        if item_id(item).is_none() {
            debug_malformed_item(item_ty);
            return;
        }
        match item_ty {
            "agent_message" if lifecycle == ItemLifecycle::Completed => {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    self.publish_text(stream, text);
                } else {
                    debug_malformed_item(item_ty);
                }
            }
            "reasoning" if lifecycle == ItemLifecycle::Completed => {
                self.handle_reasoning(stream, item);
            }
            "command_execution" => self.handle_command(stream, item, lifecycle),
            "file_change" if lifecycle == ItemLifecycle::Completed => {
                self.handle_file_change(stream, item, lifecycle);
            }
            "mcp_tool_call" => self.handle_mcp_tool_call(stream, item, lifecycle),
            "collab_tool_call" => self.handle_collab_tool_call(stream, item, lifecycle),
            "web_search" => self.handle_web_search(stream, item, lifecycle),
            "todo_list" => self.handle_todo_list(stream, item, lifecycle),
            "error" if lifecycle == ItemLifecycle::Completed => {
                self.handle_item_error(stream, item);
            }
            "agent_message" | "reasoning" | "file_change" | "error" => {}
            item_type => {
                tracing::debug!(
                    target: "codex.console",
                    item_type,
                    "ignoring unsupported Codex item type"
                );
            }
        }
    }

    fn handle_reasoning(&self, stream: &Arc<RunStreamHandle>, item: &Value) {
        let Some(text) = non_empty_string(item, "text") else {
            debug_malformed_item("reasoning");
            return;
        };
        emit(
            stream,
            json!({
                "type": "codex.console.thinking",
                "text": text,
            }),
        );
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

    fn handle_command(
        &self,
        stream: &Arc<RunStreamHandle>,
        item: &Value,
        lifecycle: ItemLifecycle,
    ) {
        let Some(id) = item_id(item) else {
            debug_malformed_item("command_execution");
            return;
        };
        let Some(command) = non_empty_string(item, "command") else {
            debug_malformed_item("command_execution");
            return;
        };
        let output = item
            .get("aggregated_output")
            .and_then(|v| v.as_str())
            .filter(|text| !text.is_empty());

        emit_tool(
            stream,
            id,
            "shell",
            command,
            normalized_status(item, lifecycle),
            output,
        );
    }

    fn handle_file_change(
        &self,
        stream: &Arc<RunStreamHandle>,
        item: &Value,
        lifecycle: ItemLifecycle,
    ) {
        let Some(id) = item_id(item) else {
            debug_malformed_item("file_change");
            return;
        };
        let Some(changes) = item.get("changes").and_then(Value::as_array) else {
            debug_malformed_item("file_change");
            return;
        };
        let mut lines = Vec::with_capacity(changes.len());
        for change in changes {
            let Some(path) = non_empty_string(change, "path") else {
                debug_malformed_item("file_change");
                return;
            };
            let Some(kind @ ("add" | "update" | "delete")) =
                change.get("kind").and_then(Value::as_str)
            else {
                debug_malformed_item("file_change");
                return;
            };
            lines.push(format!("{kind} {path}"));
        }
        let output = (!lines.is_empty()).then(|| lines.join("\n"));
        emit_tool(
            stream,
            id,
            "action",
            "File changes",
            normalized_status(item, lifecycle),
            output.as_deref(),
        );
    }

    fn handle_mcp_tool_call(
        &self,
        stream: &Arc<RunStreamHandle>,
        item: &Value,
        lifecycle: ItemLifecycle,
    ) {
        let Some(id) = item_id(item) else {
            debug_malformed_item("mcp_tool_call");
            return;
        };
        let (Some(server), Some(tool)) = (
            non_empty_string(item, "server"),
            non_empty_string(item, "tool"),
        ) else {
            debug_malformed_item("mcp_tool_call");
            return;
        };
        let status = normalized_status(item, lifecycle);
        let output = (status == "error")
            .then(|| item.get("error")?.get("message")?.as_str())
            .flatten()
            .filter(|text| !text.trim().is_empty());
        emit_tool(
            stream,
            id,
            "action",
            &format!("MCP {server}.{tool}"),
            status,
            output,
        );
    }

    fn handle_collab_tool_call(
        &self,
        stream: &Arc<RunStreamHandle>,
        item: &Value,
        lifecycle: ItemLifecycle,
    ) {
        let Some(id) = item_id(item) else {
            debug_malformed_item("collab_tool_call");
            return;
        };
        let Some(tool) = non_empty_string(item, "tool") else {
            debug_malformed_item("collab_tool_call");
            return;
        };
        let receiver_count = item
            .get("receiver_thread_ids")
            .and_then(Value::as_array)
            .map(Vec::len);
        let output = receiver_count.map(|count| {
            let noun = if count == 1 { "agent" } else { "agents" };
            format!("{count} {noun}")
        });
        emit_tool(
            stream,
            id,
            "action",
            &format!("Collaboration: {}", tool.replace('_', " ")),
            normalized_status(item, lifecycle),
            output.as_deref(),
        );
    }

    fn handle_web_search(
        &self,
        stream: &Arc<RunStreamHandle>,
        item: &Value,
        lifecycle: ItemLifecycle,
    ) {
        let (Some(id), Some(query)) = (item_id(item), non_empty_string(item, "query")) else {
            debug_malformed_item("web_search");
            return;
        };
        emit_tool(
            stream,
            id,
            "action",
            &format!("Web search: {query}"),
            normalized_status(item, lifecycle),
            None,
        );
    }

    fn handle_todo_list(
        &self,
        stream: &Arc<RunStreamHandle>,
        item: &Value,
        lifecycle: ItemLifecycle,
    ) {
        let Some(id) = item_id(item) else {
            debug_malformed_item("todo_list");
            return;
        };
        let Some(items) = item.get("items").and_then(Value::as_array) else {
            debug_malformed_item("todo_list");
            return;
        };
        let mut lines = Vec::with_capacity(items.len());
        for todo in items {
            let Some(text) = non_empty_string(todo, "text") else {
                debug_malformed_item("todo_list");
                return;
            };
            let Some(completed) = todo.get("completed").and_then(Value::as_bool) else {
                debug_malformed_item("todo_list");
                return;
            };
            lines.push(format!("[{}] {text}", if completed { 'x' } else { ' ' }));
        }
        let output = (!lines.is_empty()).then(|| lines.join("\n"));
        emit_tool(
            stream,
            id,
            "action",
            "To-do list",
            normalized_status(item, lifecycle),
            output.as_deref(),
        );
    }

    fn handle_item_error(&self, stream: &Arc<RunStreamHandle>, item: &Value) {
        let (Some(id), Some(message)) = (item_id(item), non_empty_string(item, "message")) else {
            debug_malformed_item("error");
            return;
        };
        emit_tool(stream, id, "action", "Codex error", "error", Some(message));
    }

    fn handle_stream_error(
        &mut self,
        stream: &Arc<RunStreamHandle>,
        value: &Value,
        title: &str,
        event_type: &str,
    ) {
        let Some(message) = non_empty_string(value, "message") else {
            tracing::debug!(
                target: "codex.console",
                event_type,
                "ignoring malformed Codex error event"
            );
            return;
        };
        self.synthetic_error_id += 1;
        let id = format!("codex-error-{}", self.synthetic_error_id);
        emit_tool(stream, &id, "action", title, "error", Some(message));
    }
}

fn item_id(item: &Value) -> Option<&str> {
    non_empty_string(item, "id")
}

fn non_empty_string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
}

fn normalized_status(item: &Value, lifecycle: ItemLifecycle) -> &'static str {
    match item.get("status").and_then(Value::as_str) {
        Some("completed") => "completed",
        Some("failed" | "declined") => "error",
        Some("in_progress") if lifecycle != ItemLifecycle::Completed => "running",
        Some("in_progress") | None if lifecycle == ItemLifecycle::Completed => {
            match item.get("exit_code").and_then(Value::as_i64) {
                Some(code) if code != 0 => "error",
                _ => "completed",
            }
        }
        _ if lifecycle == ItemLifecycle::Completed => "completed",
        _ => "running",
    }
}

fn emit_tool(
    stream: &Arc<RunStreamHandle>,
    id: &str,
    variant: &str,
    title: &str,
    status: &str,
    output: Option<&str>,
) {
    emit(
        stream,
        json!({
            "type": "codex.console.tool",
            "id": id,
            "variant": variant,
            "title": title,
            "status": status,
            "output": output,
        }),
    );
}

fn debug_malformed_item(item_type: &str) {
    tracing::debug!(
        target: "codex.console",
        item_type,
        "ignoring malformed Codex item"
    );
}

fn emit(stream: &Arc<RunStreamHandle>, event: Value) {
    stream.publish(LiveMessage::Event { event });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::artifact_service::{ArtifactService, RunArtifactPaths};
    use crate::sessions::run_registry::RunStreamRegistry;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/codex")
    }

    fn published_fixture_events() -> Vec<Value> {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut publisher = CodexConsolePublisher::new();
        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read Codex done fixture");

        for line in raw.lines() {
            let value: Value = serde_json::from_str(line).expect("valid fixture event");
            publisher.handle_json(&handle, &value);
        }

        handle
            .buffered_tail()
            .into_iter()
            .filter_map(|message| match message {
                LiveMessage::Event { event } => Some(event),
                _ => None,
            })
            .collect()
    }

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
        assert_eq!(events[0]["id"], "item_1");
        assert_eq!(events[0]["variant"], "shell");
        assert_eq!(events[0]["status"], "completed");
    }

    #[test]
    fn failed_and_declined_commands_publish_error_status() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut publisher = CodexConsolePublisher::new();

        for value in [
            json!({
                "type": "item.completed",
                "item": {
                    "id": "cmd_failed",
                    "type": "command_execution",
                    "command": "cargo test",
                    "aggregated_output": "test failed",
                    "exit_code": 1,
                    "status": "failed"
                }
            }),
            json!({
                "type": "item.completed",
                "item": {
                    "id": "cmd_declined",
                    "type": "command_execution",
                    "command": "git push",
                    "aggregated_output": "",
                    "exit_code": null,
                    "status": "declined"
                }
            }),
        ] {
            publisher.handle_json(&handle, &value);
        }

        let events: Vec<Value> = handle
            .buffered_tail()
            .into_iter()
            .filter_map(|message| match message {
                LiveMessage::Event { event } => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["id"], "cmd_failed");
        assert_eq!(events[0]["status"], "error");
        assert_eq!(events[1]["id"], "cmd_declined");
        assert_eq!(events[1]["status"], "error");
    }

    #[test]
    fn publishes_sanitized_fixture_in_order() {
        let events = published_fixture_events();

        assert_eq!(events.len(), 10);
        assert_eq!(events[0]["type"], "codex.console.session");
        assert_eq!(events[1]["type"], "codex.console.thinking");
        assert_eq!(
            events[1]["text"],
            "Inspecting the provider event boundary and existing console contract."
        );

        for (index, status) in [(2, "running"), (3, "running"), (4, "completed")] {
            assert_eq!(events[index]["type"], "codex.console.tool");
            assert_eq!(events[index]["id"], "cmd_1");
            assert_eq!(events[index]["variant"], "shell");
            assert_eq!(events[index]["status"], status);
        }
        assert!(events[4]["output"].as_str().unwrap().contains("... ok"));

        assert_eq!(events[5]["id"], "patch_1");
        assert_eq!(events[5]["variant"], "action");
        let file_output = events[5]["output"].as_str().expect("file-change output");
        assert!(file_output.contains("update server/src/providers/codex_console.rs"));
        assert!(file_output.contains("add fixtures/codex/schema-sample.jsonl"));
        assert!(file_output.contains("delete fixtures/codex/legacy-sample.jsonl"));

        assert_eq!(events[6]["id"], "mcp_1");
        assert_eq!(events[6]["status"], "running");
        assert_eq!(events[7]["id"], "mcp_1");
        assert_eq!(events[7]["status"], "completed");
        assert_eq!(events[8]["type"], "codex.console.text");
        assert_eq!(events[9]["type"], "codex.console.result");
        assert_eq!(events[9]["contract"]["status"], "done");
    }

    #[test]
    fn publishes_collaboration_web_search_todo_and_error_items() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut publisher = CodexConsolePublisher::new();
        let values = [
            json!({
                "type": "item.started",
                "item": {
                    "id": "collab_1",
                    "type": "collab_tool_call",
                    "tool": "spawn_agent",
                    "sender_thread_id": "thread_parent",
                    "receiver_thread_ids": ["thread_child"],
                    "prompt": "Review the adapter",
                    "agents_states": {
                        "thread_child": {"status": "pending_init", "message": null}
                    },
                    "status": "in_progress"
                }
            }),
            json!({
                "type": "item.completed",
                "item": {
                    "id": "collab_1",
                    "type": "collab_tool_call",
                    "tool": "spawn_agent",
                    "sender_thread_id": "thread_parent",
                    "receiver_thread_ids": ["thread_child"],
                    "prompt": "Review the adapter",
                    "agents_states": {
                        "thread_child": {"status": "completed", "message": "Done"}
                    },
                    "status": "completed"
                }
            }),
            json!({
                "type": "item.started",
                "item": {
                    "id": "web_1",
                    "type": "web_search",
                    "query": "Codex exec event schema",
                    "action": {"type": "search", "query": "Codex exec event schema"}
                }
            }),
            json!({
                "type": "item.completed",
                "item": {
                    "id": "web_1",
                    "type": "web_search",
                    "query": "Codex exec event schema",
                    "action": {"type": "search", "query": "Codex exec event schema"}
                }
            }),
            json!({
                "type": "item.started",
                "item": {
                    "id": "todo_1",
                    "type": "todo_list",
                    "items": [{"text": "Inspect schema", "completed": false}]
                }
            }),
            json!({
                "type": "item.updated",
                "item": {
                    "id": "todo_1",
                    "type": "todo_list",
                    "items": [
                        {"text": "Inspect schema", "completed": true},
                        {"text": "Update adapter", "completed": false}
                    ]
                }
            }),
            json!({
                "type": "item.completed",
                "item": {
                    "id": "todo_1",
                    "type": "todo_list",
                    "items": [
                        {"text": "Inspect schema", "completed": true},
                        {"text": "Update adapter", "completed": true}
                    ]
                }
            }),
            json!({
                "type": "item.completed",
                "item": {
                    "id": "error_1",
                    "type": "error",
                    "message": "A recoverable tool error occurred"
                }
            }),
        ];

        for value in values {
            publisher.handle_json(&handle, &value);
        }

        let events: Vec<Value> = handle
            .buffered_tail()
            .into_iter()
            .filter_map(|message| match message {
                LiveMessage::Event { event } => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 8);
        assert_eq!(events[0]["id"], "collab_1");
        assert_eq!(events[0]["status"], "running");
        assert_eq!(events[1]["id"], "collab_1");
        assert_eq!(events[1]["status"], "completed");
        assert_eq!(events[2]["id"], "web_1");
        assert_eq!(events[3]["status"], "completed");
        assert_eq!(events[4]["id"], "todo_1");
        assert!(events[5]["output"]
            .as_str()
            .unwrap()
            .contains("[x] Inspect schema"));
        assert_eq!(events[6]["status"], "completed");
        assert_eq!(events[7]["id"], "error_1");
        assert_eq!(events[7]["status"], "error");
        assert_eq!(events[7]["output"], "A recoverable tool error occurred");
    }

    #[test]
    fn normalized_fixture_events_round_trip_through_artifact_storage() {
        let events = published_fixture_events();
        let temp = tempfile::tempdir().expect("temp artifact directory");
        let paths =
            RunArtifactPaths::new(temp.path().to_str().expect("UTF-8 temp path"), "codex-run");

        ArtifactService::write_console_events(&paths, &events).expect("write console events");

        assert_eq!(ArtifactService::read_console_events(&paths), events);
    }

    #[test]
    fn reasoning_that_looks_like_a_contract_is_not_published_as_result() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut publisher = CodexConsolePublisher::new();
        let contract =
            r#"{"status":"done","summary":"Final.","changedFiles":[],"testsRun":[],"blockers":[]}"#;

        publisher.handle_json(
            &handle,
            &json!({
                "type": "item.completed",
                "item": {"id": "reason_1", "type": "reasoning", "text": contract}
            }),
        );
        publisher.handle_json(
            &handle,
            &json!({
                "type": "item.completed",
                "item": {"id": "message_1", "type": "agent_message", "text": contract}
            }),
        );

        let events: Vec<Value> = handle
            .buffered_tail()
            .into_iter()
            .filter_map(|message| match message {
                LiveMessage::Event { event } => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["type"], "codex.console.thinking");
        assert_eq!(events[1]["type"], "codex.console.result");
    }

    #[test]
    fn unknown_and_malformed_events_are_ignored() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut publisher = CodexConsolePublisher::new();

        for value in [
            json!({"type": "future.event", "secret": "must not be logged"}),
            json!({
                "type": "item.completed",
                "item": {"id": "future_1", "type": "future_item", "secret": "hidden"}
            }),
            json!({
                "type": "item.started",
                "item": {"type": "command_execution", "command": "missing id"}
            }),
            json!({
                "type": "item.completed",
                "item": {
                    "type": "agent_message",
                    "text": "{\"status\":\"done\",\"summary\":\"Malformed.\"}"
                }
            }),
            json!({"type": "item.updated", "item": {"id": "missing_type"}}),
            json!({"item": {"id": "missing_event_type", "type": "reasoning"}}),
        ] {
            publisher.handle_json(&handle, &value);
        }

        assert!(handle.buffered_tail().is_empty());
    }

    #[test]
    fn extracts_result_contract_from_text() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut publisher = CodexConsolePublisher::new();

        let contract =
            r#"{"status":"done","summary":"Done.","changedFiles":[],"testsRun":[],"blockers":[]}"#;
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
        let contract =
            r#"{"status":"done","summary":"Done.","changedFiles":[],"testsRun":[],"blockers":[]}"#;

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
