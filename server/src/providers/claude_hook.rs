//! Read-only runs decide Claude Code's tool use themselves.
//!
//! With `--input-format stream-json`, Coppice opens the session with an
//! `initialize` control request that registers a `PreToolUse` hook. Claude
//! Code then sends a `hook_callback` control request before every tool call,
//! and Coppice answers allow or deny. Read-only then holds whatever a
//! permission mode means in the next release; `dontAsk` and the allow and
//! deny lists stay as a second layer.
//!
//! Checked against Claude Code 2.1.302: the callbacks arrive even with
//! `--settings {"disableAllHooks":true}`, which only turns off hooks from
//! settings files, and a deny holds even for a tool on `--allowedTools`.

use serde_json::{json, Value};

use super::CHAT_READ_ONLY_TOOLS;

const INITIALIZE_ID: &str = "coppice-initialize";
const PRE_TOOL_USE_ID: &str = "coppice-pre-tool-use";
const DENY_REASON: &str = "Coppice: this run only allows tools that read. Do not retry this tool.";

/// Tools a read-only run may use besides [`CHAT_READ_ONLY_TOOLS`] and the
/// Coppice gateway. `ToolSearch` loads deferred MCP tools, including
/// `result_submit`, and changes nothing. `LS` is the older file list tool.
const ALSO_ALLOWED: &[&str] = &["ToolSearch", "LS"];

/// The handshake and the prompt, written to stdin before anything is read.
pub fn opening_lines(prompt: &str) -> Vec<String> {
    vec![
        json!({
            "type": "control_request",
            "request_id": INITIALIZE_ID,
            "request": {
                "subtype": "initialize",
                "hooks": { "PreToolUse": [{ "matcher": null, "hookCallbackIds": [PRE_TOOL_USE_ID] }] }
            }
        })
        .to_string(),
        json!({
            "type": "user",
            "message": { "role": "user", "content": [{ "type": "text", "text": prompt }] }
        })
        .to_string(),
    ]
}

pub fn allows(tool: &str) -> bool {
    tool.starts_with(&format!("mcp__{}__", crate::mcp::protocol::SERVER_NAME))
        || CHAT_READ_ONLY_TOOLS
            .split(',')
            .any(|allowed| allowed == tool)
        || ALSO_ALLOWED.contains(&tool)
}

/// Answers control requests and remembers what it refused.
#[derive(Debug, Default)]
pub struct HookGate {
    replies: Vec<String>,
    pub refused: Vec<String>,
    /// Claude Code rejected the handshake; only the second layer applies.
    pub initialize_error: Option<String>,
}

impl HookGate {
    pub fn on_json(&mut self, value: &Value) {
        match value.get("type").and_then(Value::as_str) {
            Some("control_request") => self.on_request(value),
            Some("control_response") => self.on_response(value),
            _ => {}
        }
    }

    fn on_request(&mut self, value: &Value) {
        let Some(request_id) = value.get("request_id").and_then(Value::as_str) else {
            return;
        };
        let request = value.get("request").cloned().unwrap_or(Value::Null);
        let subtype = request.get("subtype").and_then(Value::as_str).unwrap_or("");
        let reply = if subtype == "hook_callback" {
            let tool = request
                .pointer("/input/tool_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let output = if request.get("callback_id").and_then(Value::as_str)
                != Some(PRE_TOOL_USE_ID)
            {
                // Not ours: carry on without a decision.
                json!({})
            } else if allows(tool) {
                json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "allow" } })
            } else {
                self.refused.push(tool.to_string());
                tracing::info!(tool, "refused a tool in a read-only Claude Code run");
                json!({ "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": DENY_REASON
                } })
            };
            json!({ "type": "control_response",
                    "response": { "subtype": "success", "request_id": request_id, "response": output } })
        } else {
            // Nothing else was registered; say so instead of leaving it waiting.
            json!({ "type": "control_response",
                    "response": { "subtype": "error", "request_id": request_id,
                                  "error": format!("Coppice doesn't handle {subtype}") } })
        };
        self.replies.push(reply.to_string());
    }

    fn on_response(&mut self, value: &Value) {
        let Some(response) = value.get("response") else {
            return;
        };
        if response.get("request_id").and_then(Value::as_str) != Some(INITIALIZE_ID) {
            return;
        }
        if response.get("subtype").and_then(Value::as_str) != Some("success") {
            let error = response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("no reason given")
                .to_string();
            tracing::warn!(%error, "Claude Code rejected Coppice's tool hook; dontAsk and the tool lists still apply");
            self.initialize_error = Some(error);
        }
    }

    pub fn take_replies(&mut self) -> Vec<String> {
        std::mem::take(&mut self.replies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callback(id: &str, tool: &str) -> Value {
        json!({ "type": "control_request", "request_id": id,
                "request": { "subtype": "hook_callback", "callback_id": PRE_TOOL_USE_ID,
                             "input": { "hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": {} } } })
    }

    fn decision(reply: &str) -> (String, Option<String>) {
        let v: Value = serde_json::from_str(reply).unwrap();
        let id = v
            .pointer("/response/request_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let d = v
            .pointer("/response/response/hookSpecificOutput/permissionDecision")
            .and_then(Value::as_str)
            .map(str::to_string);
        (id, d)
    }

    #[test]
    fn opens_with_the_hook_then_the_prompt() {
        let lines = opening_lines("do it");
        let init: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(init["request"]["subtype"], "initialize");
        assert_eq!(
            init["request"]["hooks"]["PreToolUse"][0]["hookCallbackIds"][0],
            PRE_TOOL_USE_ID
        );
        let user: Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(user["message"]["content"][0]["text"], "do it");
    }

    #[test]
    fn allows_reading_and_the_gateway_and_denies_the_rest() {
        for tool in [
            "Read",
            "Glob",
            "Grep",
            "WebFetch",
            "WebSearch",
            "ToolSearch",
            "LS",
            "mcp__coppice__result_submit",
        ] {
            assert!(allows(tool), "{tool}");
        }
        for tool in [
            "Bash",
            "Write",
            "Edit",
            "MultiEdit",
            "NotebookEdit",
            "Task",
            "TodoWrite",
            "mcp__other__delete",
            "mcp__coppice_evil__x",
            "SomeNewTool",
            "",
        ] {
            assert!(!allows(tool), "{tool}");
        }
    }

    #[test]
    fn answers_each_callback_with_its_request_id() {
        let mut gate = HookGate::default();
        gate.on_json(&callback("r1", "Read"));
        gate.on_json(&callback("r2", "Bash"));
        let replies = gate.take_replies();
        assert_eq!(decision(&replies[0]), ("r1".into(), Some("allow".into())));
        assert_eq!(decision(&replies[1]), ("r2".into(), Some("deny".into())));
        assert_eq!(gate.refused, ["Bash"]);
        assert!(gate.take_replies().is_empty());
    }

    #[test]
    fn other_requests_get_an_error_and_a_rejected_handshake_is_noted() {
        let mut gate = HookGate::default();
        gate.on_json(&json!({ "type": "control_request", "request_id": "x",
                              "request": { "subtype": "can_use_tool", "tool_name": "Bash" } }));
        let reply: Value = serde_json::from_str(&gate.take_replies()[0]).unwrap();
        assert_eq!(reply["response"]["subtype"], "error");
        assert_eq!(reply["response"]["request_id"], "x");

        gate.on_json(&json!({ "type": "control_response",
                              "response": { "subtype": "error", "request_id": INITIALIZE_ID, "error": "unknown subtype" } }));
        assert_eq!(gate.initialize_error.as_deref(), Some("unknown subtype"));
    }
}
