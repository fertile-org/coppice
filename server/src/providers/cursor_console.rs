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

const KNOWN_TOOL_KEYS: &[&str] = &[
    "shellToolCall",
    "editToolCall",
    "readToolCall",
    "writeToolCall",
    "deleteToolCall",
];

fn find_tool_payload(tool_call: &Value) -> Option<(&str, &Value)> {
    let obj = tool_call.as_object()?;
    for &key in KNOWN_TOOL_KEYS {
        if let Some(value) = obj.get(key) {
            return Some((key, value));
        }
    }
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

fn non_empty_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Walk string / object / simple string-array shapes for error-ish text.
fn value_text(value: &Value) -> Option<String> {
    if let Some(text) = non_empty_str(value.as_str()) {
        return Some(text);
    }
    if let Some(arr) = value.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|item| non_empty_str(item.as_str()))
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("\n"));
        }
        return None;
    }
    let obj = value.as_object()?;
    for key in ["message", "reason", "error", "text", "content", "details"] {
        if let Some(text) = obj.get(key).and_then(value_text) {
            return Some(text);
        }
    }
    None
}

fn extract_error_output(result: &Value) -> Option<String> {
    value_text(result.get("error")?)
}

fn extract_failure_output(result: &Value) -> Option<String> {
    value_text(result.get("failure")?)
}

fn stream_text(success: &Value, key: &str) -> Option<String> {
    let value = success.get(key)?;
    if let Some(text) = non_empty_str(value.as_str()) {
        return Some(text);
    }
    // Tolerate `{ "text": "..." }` or arrays of strings / text objects.
    if let Some(text) = value.get("text").and_then(|v| non_empty_str(v.as_str())) {
        return Some(text);
    }
    if let Some(arr) = value.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                non_empty_str(item.as_str()).or_else(|| {
                    item.get("text")
                        .and_then(|v| non_empty_str(v.as_str()))
                })
            })
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("\n"));
        }
    }
    None
}

fn is_generic_failure_phrase(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "tool failed" | "tool call failed" | "failed" | "error"
    )
}

fn exit_code_line(success: &Value) -> Option<String> {
    success
        .get("exitCode")
        .and_then(|v| v.as_i64())
        .map(|code| format!("exit code {code}"))
}

fn args_summary(payload: &Value) -> Option<String> {
    let args = payload.get("args")?;
    if let Some(command) = non_empty_str(args.get("command").and_then(|v| v.as_str())) {
        return Some(format!("command: {command}"));
    }
    if let Some(path) = non_empty_str(args.get("path").and_then(|v| v.as_str())) {
        return Some(format!("path: {path}"));
    }
    None
}

fn join_secondary(parts: &[String]) -> Option<String> {
    let nonempty: Vec<&str> = parts
        .iter()
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .collect();
    if nonempty.is_empty() {
        None
    } else {
        Some(nonempty.join("\n\n"))
    }
}

fn secondary_adds_info(primary: &str, secondary: &str) -> bool {
    let primary_lower = primary.to_ascii_lowercase();
    secondary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .any(|line| !primary_lower.contains(&line.to_ascii_lowercase()))
}

fn compose_error_output(primary: Option<String>, secondary: Option<String>) -> Option<String> {
    match (primary, secondary) {
        (Some(primary), Some(secondary)) if is_generic_failure_phrase(&primary) => {
            // Prefer actionable secondary; keep primary only when it still adds a label.
            if secondary_adds_info(&primary, &secondary) {
                Some(format!("{primary}\n\n{secondary}"))
            } else {
                Some(secondary)
            }
        }
        (Some(primary), Some(secondary)) => {
            if secondary_adds_info(&primary, &secondary) {
                Some(format!("{primary}\n\n{secondary}"))
            } else {
                Some(primary)
            }
        }
        (Some(primary), None) => Some(primary),
        (None, Some(secondary)) => Some(secondary),
        (None, None) => None,
    }
}

fn stream_and_exit_secondary(result: &Value) -> Option<String> {
    let success = result.get("success")?;
    let stdout = stream_text(success, "stdout");
    let stderr = stream_text(success, "stderr");
    let mut parts: Vec<String> = Vec::new();
    match (stderr, stdout) {
        (Some(err), Some(out)) => {
            parts.push(err);
            parts.push(out);
        }
        (Some(err), None) => parts.push(err),
        (None, Some(out)) => parts.push(out),
        (None, None) => {}
    }
    // Exit code only when streams are absent (keeps PR #2 stream-only output).
    if parts.is_empty() {
        if let Some(code) = exit_code_line(success) {
            parts.push(code);
        }
    }
    join_secondary(&parts)
}

/// Extraction for completed tools:
/// - Success: stdout-first, then stderr.
/// - Error: compose primary (`result.error` / `result.failure`) with secondary
///   (stderr → stdout, exit code, short args summary). Generic primary phrases
///   like `tool call failed` never suppress secondary context.
fn tool_output(payload: &Value) -> Option<String> {
    let status = completed_tool_status(payload);
    let result = payload.get("result")?;

    if status != "error" {
        if let Some(success) = result.get("success") {
            let stdout = stream_text(success, "stdout");
            let stderr = stream_text(success, "stderr");
            if let Some(out) = stdout {
                return Some(out);
            }
            if let Some(err) = stderr {
                return Some(err);
            }
        }
        return None;
    }

    let primary = extract_error_output(result).or_else(|| extract_failure_output(result));
    let primary_was_generic = primary
        .as_ref()
        .is_some_and(|p| is_generic_failure_phrase(p));
    let secondary = stream_and_exit_secondary(result);

    if let Some(output) = compose_error_output(primary, secondary) {
        // Args summary only when the Cursor primary was opaque and nothing richer landed.
        if primary_was_generic && is_generic_failure_phrase(&output) {
            if let Some(summary) = args_summary(payload) {
                return Some(format!("{output}\n\n{summary}"));
            }
        }
        return Some(output);
    }

    if let Some(summary) = args_summary(payload) {
        return Some(summary);
    }
    Some("tool failed".to_string())
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

    #[test]
    fn duplicate_result_contract_is_skipped() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();
        let contract =
            r#"{"status":"done","summary":"Done.","changedFiles":[],"testsRun":[],"blockers":[]}"#;

        console.handle_stream_json(
            &handle,
            &json!({
                "type": "assistant",
                "message": {"content": [{"type": "text", "text": contract}]}
            }),
        );
        console.handle_stream_json(
            &handle,
            &json!({"type": "result", "result": contract}),
        );

        let result_count = collect_events(&handle)
            .iter()
            .filter(|e| e["type"] == "cursor.console.result")
            .count();
        assert_eq!(result_count, 1);
    }

    fn publish_completed_shell(
        console: &mut CursorConsolePublisher,
        handle: &std::sync::Arc<crate::sessions::run_registry::RunStreamHandle>,
        call_id: &str,
        result: Value,
    ) {
        console.handle_stream_json(
            handle,
            &json!({
                "type": "tool_call",
                "subtype": "completed",
                "call_id": call_id,
                "tool_call": {
                    "shellToolCall": {
                        "args": {"command": "cargo test"},
                        "result": result
                    },
                    "toolCallId": call_id
                }
            }),
        );
    }

    fn last_tool(handle: &std::sync::Arc<crate::sessions::run_registry::RunStreamHandle>) -> Value {
        collect_events(handle)
            .into_iter()
            .filter(|e| e["type"] == "cursor.console.tool")
            .next_back()
            .expect("tool event")
    }

    #[test]
    fn nonzero_exit_with_stdout_publishes_output() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        publish_completed_shell(
            &mut console,
            &handle,
            "tool_fail",
            json!({"success": {"exitCode": 1, "stdout": "FAILED"}}),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "error");
        assert_eq!(tool["output"], "FAILED");
    }

    #[test]
    fn nonzero_exit_with_stderr_and_stdout_publishes_stderr_first() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        publish_completed_shell(
            &mut console,
            &handle,
            "tool_both",
            json!({
                "success": {
                    "exitCode": 2,
                    "stdout": "out line",
                    "stderr": "err line"
                }
            }),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "error");
        assert_eq!(tool["output"], "err line\n\nout line");
    }

    #[test]
    fn nonzero_exit_with_empty_streams_publishes_exit_code_fallback() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        publish_completed_shell(
            &mut console,
            &handle,
            "tool_empty",
            json!({"success": {"exitCode": 7, "stdout": "", "stderr": ""}}),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "error");
        assert_eq!(tool["output"], "exit code 7");
    }

    #[test]
    fn result_error_message_and_string_error_are_published() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        console.handle_stream_json(
            &handle,
            &json!({
                "type": "tool_call",
                "subtype": "completed",
                "call_id": "tool_err_obj",
                "tool_call": {
                    "readToolCall": {
                        "args": {"path": "/missing.rs"},
                        "result": {"error": {"message": "not found"}}
                    },
                    "toolCallId": "tool_err_obj"
                }
            }),
        );
        console.handle_stream_json(
            &handle,
            &json!({
                "type": "tool_call",
                "subtype": "completed",
                "call_id": "tool_err_str",
                "tool_call": {
                    "readToolCall": {
                        "args": {"path": "/missing.rs"},
                        "result": {"error": "boom"}
                    },
                    "toolCallId": "tool_err_str"
                }
            }),
        );

        let tools: Vec<_> = collect_events(&handle)
            .into_iter()
            .filter(|e| e["type"] == "cursor.console.tool")
            .collect();
        assert_eq!(tools[0]["status"], "error");
        assert_eq!(tools[0]["output"], "not found");
        assert_eq!(tools[1]["status"], "error");
        assert_eq!(tools[1]["output"], "boom");
    }

    #[test]
    fn result_failure_string_and_object_message_are_published() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        publish_completed_shell(
            &mut console,
            &handle,
            "tool_fail_str",
            json!({"failure": "permission denied"}),
        );
        publish_completed_shell(
            &mut console,
            &handle,
            "tool_fail_obj",
            json!({"failure": {"message": "timeout waiting"}}),
        );

        let tools: Vec<_> = collect_events(&handle)
            .into_iter()
            .filter(|e| e["type"] == "cursor.console.tool")
            .collect();
        assert_eq!(tools[0]["status"], "error");
        assert_eq!(tools[0]["output"], "permission denied");
        assert_eq!(tools[1]["status"], "error");
        assert_eq!(tools[1]["output"], "timeout waiting");
    }

    #[test]
    fn successful_tool_still_publishes_stdout_as_completed() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        publish_completed_shell(
            &mut console,
            &handle,
            "tool_ok",
            json!({"success": {"exitCode": 0, "stdout": "ok\n"}}),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "completed");
        assert_eq!(tool["output"], "ok");
    }

    #[test]
    fn generic_tool_call_failed_merges_stderr_and_exit_code() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        publish_completed_shell(
            &mut console,
            &handle,
            "tool_opaque",
            json!({
                "error": "tool call failed",
                "success": {
                    "exitCode": 1,
                    "stdout": "",
                    "stderr": "error: could not compile `coppice-server`"
                }
            }),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "error");
        let output = tool["output"].as_str().unwrap();
        assert!(
            output.contains("could not compile"),
            "expected actionable stderr, got {output:?}"
        );
        assert!(
            !matches!(
                output.trim().to_ascii_lowercase().as_str(),
                "tool call failed" | "tool failed" | "failed" | "error"
            ),
            "must not leave only the generic phrase, got {output:?}"
        );
    }

    #[test]
    fn generic_failure_phrase_with_exit_code_only_publishes_exit_code() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        publish_completed_shell(
            &mut console,
            &handle,
            "tool_opaque_exit",
            json!({
                "failure": {"message": "tool call failed"},
                "success": {"exitCode": 127, "stdout": "", "stderr": ""}
            }),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "error");
        let output = tool["output"].as_str().unwrap();
        assert!(
            output.contains("exit code 127"),
            "expected exit code in output, got {output:?}"
        );
        assert!(
            output.contains("tool call failed") || output.contains("exit code 127"),
            "expected composed or secondary detail, got {output:?}"
        );
    }

    #[test]
    fn generic_error_without_streams_falls_back_to_args_summary() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        console.handle_stream_json(
            &handle,
            &json!({
                "type": "tool_call",
                "subtype": "completed",
                "call_id": "tool_opaque_args",
                "tool_call": {
                    "readToolCall": {
                        "args": {"path": "/tmp/missing.rs"},
                        "result": {"error": {"details": "tool call failed"}}
                    },
                    "toolCallId": "tool_opaque_args"
                }
            }),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "error");
        let output = tool["output"].as_str().unwrap();
        assert!(
            output.contains("/tmp/missing.rs"),
            "expected path args summary, got {output:?}"
        );
    }

    #[test]
    fn nested_error_object_and_object_stream_text_are_extracted() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        publish_completed_shell(
            &mut console,
            &handle,
            "tool_nested",
            json!({
                "error": {"text": "tool call failed"},
                "success": {
                    "exitCode": 1,
                    "stderr": {"text": "permission denied: /etc/shadow"},
                    "stdout": ""
                }
            }),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "error");
        let output = tool["output"].as_str().unwrap();
        assert!(
            output.contains("permission denied"),
            "expected nested stderr text, got {output:?}"
        );
    }

    #[test]
    fn specific_error_message_is_not_diluted_by_args_summary() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();

        console.handle_stream_json(
            &handle,
            &json!({
                "type": "tool_call",
                "subtype": "completed",
                "call_id": "tool_specific",
                "tool_call": {
                    "readToolCall": {
                        "args": {"path": "/missing.rs"},
                        "result": {"error": {"message": "not found"}}
                    },
                    "toolCallId": "tool_specific"
                }
            }),
        );

        let tool = last_tool(&handle);
        assert_eq!(tool["status"], "error");
        assert_eq!(tool["output"], "not found");
    }
}
