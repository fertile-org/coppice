//! OpenCode 2.x over ACP (`opencode acp`).
//!
//! OpenCode 2 moved its HTTP API under `/api/` behind a password and changed
//! its shape, so the 1.x `serve` client cannot talk to it. Its ACP server is a
//! standard protocol instead: Coppice passes the gateway in `session/new`,
//! keeps read-only as deny rules in `OPENCODE_CONFIG` (still honoured by
//! `opencode acp` 2.0.20), and gets updates it turns into the structured live
//! console the other connectors use.

use super::acp::{self, AcpLaunch, AcpLimits, AcpObserver, AcpTurn, ToolPolicy};
use super::{AgentRunInput, AgentRunResult, ProviderError};
use crate::sessions::opencode_events::extract_result_from_text;
use crate::sessions::run_registry::RunStreamHandle;
use crate::sessions::LiveMessage;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const DISPLAY_NAME: &str = "OpenCode";
const CONSOLE_PREFIX: &str = "opencode";
/// Longest tool output kept on one console entry.
const TOOL_OUTPUT_MAX: usize = 4000;

/// OpenCode 2 and later speak ACP here; 1.x keeps `opencode serve`.
pub fn uses_acp(version: &str) -> bool {
    coppice_connectors::version_at_least(version, "2.0.0")
}

/// The `configOptions` value for a model: `provider/model`.
pub fn model_value(model_provider: Option<&str>, model: Option<&str>) -> Option<String> {
    let model = super::model_flag::selected_model(model)?;
    match super::model_flag::selected_model(model_provider) {
        Some(provider) if !model.starts_with(&format!("{provider}/")) => {
            Some(format!("{provider}/{model}"))
        }
        _ => Some(model.to_string()),
    }
}

pub struct RunSpec<'a> {
    pub program: &'a str,
    pub config_path: &'a Path,
    pub worktree: &'a Path,
    pub prompt: &'a str,
    pub run_timeout: Duration,
}

pub async fn run(input: AgentRunInput, spec: RunSpec<'_>) -> Result<AgentRunResult, ProviderError> {
    let mut env = vec![(
        "OPENCODE_CONFIG".to_string(),
        spec.config_path.to_string_lossy().into_owned(),
    )];
    if input.read_only_tools {
        // A checkout's opencode.json could allow edit and bash back.
        env.push(("OPENCODE_DISABLE_PROJECT_CONFIG".into(), "1".into()));
    }
    let launch = AcpLaunch {
        display_name: DISPLAY_NAME.into(),
        program: spec.program.to_string(),
        args: vec!["acp".into()],
        env,
        cwd: spec.worktree.to_path_buf(),
        run_id: input.run_id.clone(),
        ticket_id: input.ticket_id.clone(),
        task_log_dir: super::cli_runner::run_log_dir(
            input.artifacts_dir.as_deref(),
            input.run_id.as_deref(),
        ),
    };
    let model = model_value(input.model_provider.as_deref(), input.model.as_deref());
    let turn = AcpTurn {
        cwd: spec.worktree,
        mcp: input.mcp.as_ref(),
        model: model.as_deref(),
        resume_session_id: input.resume_session_id.as_deref(),
        prompt: spec.prompt,
        policy: if input.read_only_tools {
            ToolPolicy::ReadOnly
        } else {
            ToolPolicy::AllowAll
        },
    };
    let mut console = AcpConsole::new(CONSOLE_PREFIX, input.stream.clone());
    let outcome = acp::run_turn(
        launch,
        turn,
        AcpLimits::for_run(spec.run_timeout),
        &mut console,
        input.cancel_rx,
        input.session_created_tx,
    )
    .await?;

    if let Some(problem) = outcome.stop_problem(DISPLAY_NAME) {
        // MissingResult, so a result already sent through result_submit still wins.
        return Err(ProviderError::MissingResult(problem));
    }
    extract_result_from_text(&outcome.agent_text).ok_or_else(|| {
        ProviderError::MissingResult("no result contract found in OpenCode output".into())
    })
}

/// ACP updates as `<prefix>.console.*` events. Text and thinking arrive in
/// small chunks and are published as whole blocks when something else starts.
pub struct AcpConsole {
    prefix: &'static str,
    stream: Option<Arc<RunStreamHandle>>,
    text: String,
    thinking: String,
    result_published: bool,
}

impl AcpConsole {
    pub fn new(prefix: &'static str, stream: Option<Arc<RunStreamHandle>>) -> Self {
        Self {
            prefix,
            stream,
            text: String::new(),
            thinking: String::new(),
            result_published: false,
        }
    }

    fn emit(&self, kind: &str, mut event: Value) {
        if let Some(stream) = &self.stream {
            event["type"] = json!(format!("{}.console.{kind}", self.prefix));
            stream.publish(LiveMessage::Event { event });
        }
    }

    fn flush_thinking(&mut self) {
        let text = std::mem::take(&mut self.thinking);
        let text = text.trim();
        if !text.is_empty() {
            self.emit("thinking", json!({ "text": text }));
        }
    }

    fn flush_text(&mut self) {
        let text = std::mem::take(&mut self.text);
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if let Some(result) = extract_result_from_text(text) {
            if !self.result_published {
                self.result_published = true;
                let contract = serde_json::to_value(&result).unwrap_or(json!({}));
                self.emit("result", json!({ "contract": contract }));
            }
            return;
        }
        self.emit("text", json!({ "markdown": text }));
    }

    fn flush(&mut self) {
        self.flush_thinking();
        self.flush_text();
    }
}

impl AcpObserver for AcpConsole {
    fn on_session(&mut self, _session_id: &str, model: Option<&str>) {
        if let (Some(stream), Some(model)) = (&self.stream, model) {
            stream.note_reported_model(model);
        }
        self.emit("session", json!({ "model": model.unwrap_or("") }));
    }

    fn on_update(&mut self, update: &Value) {
        match update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("")
        {
            "agent_message_chunk" => {
                self.flush_thinking();
                if let Some(text) = update.pointer("/content/text").and_then(Value::as_str) {
                    self.text.push_str(text);
                }
            }
            "agent_thought_chunk" => {
                self.flush_text();
                if let Some(text) = update.pointer("/content/text").and_then(Value::as_str) {
                    self.thinking.push_str(text);
                }
            }
            "tool_call" => {
                self.flush();
                if let Some(event) = tool_event(update, true) {
                    self.emit("tool", event);
                }
            }
            "tool_call_update" => {
                self.flush();
                if let Some(event) = tool_event(update, false) {
                    self.emit("tool", event);
                }
            }
            "plan" => {
                self.flush();
                if let Some(markdown) = plan_markdown(update) {
                    self.emit("text", json!({ "markdown": markdown }));
                }
            }
            // Usage, mode, command list, and config changes are not shown.
            _ => {}
        }
    }

    fn on_end(&mut self) {
        self.flush();
    }
}

fn tool_event(update: &Value, started: bool) -> Option<Value> {
    let id = update.get("toolCallId").and_then(Value::as_str)?;
    let status = match update.get("status").and_then(Value::as_str) {
        Some("completed") => "completed",
        Some("failed") => "error",
        _ => "running",
    };
    let mut event = json!({ "id": id, "status": status });
    if let Some(title) = update
        .get("title")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
    {
        event["title"] = json!(title);
    } else if started {
        event["title"] = json!(update.get("kind").and_then(Value::as_str).unwrap_or("tool"));
    }
    if let Some(kind) = update.get("kind").and_then(Value::as_str) {
        event["variant"] = json!(if kind == "execute" { "shell" } else { "action" });
    }
    let output = tool_output(update);
    if !output.is_empty() {
        event["output"] = json!(crate::util::truncate::truncate_with_ellipsis(
            &output,
            TOOL_OUTPUT_MAX
        ));
    }
    Some(event)
}

/// Text parts of a tool's `content`. Diffs show the file they touch.
fn tool_output(update: &Value) -> String {
    let mut parts = Vec::new();
    for item in update
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match item.get("type").and_then(Value::as_str) {
            Some("content") => {
                if let Some(text) = item.pointer("/content/text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                }
            }
            Some("diff") => {
                if let Some(path) = item.get("path").and_then(Value::as_str) {
                    parts.push(format!("Changed {path}"));
                }
            }
            _ => {}
        }
    }
    parts.join("\n")
}

fn plan_markdown(update: &Value) -> Option<String> {
    let entries = update.get("entries").and_then(Value::as_array)?;
    let lines: Vec<String> = entries
        .iter()
        .filter_map(|entry| {
            let content = entry.get("content").and_then(Value::as_str)?.trim();
            if content.is_empty() {
                return None;
            }
            let mark = match entry.get("status").and_then(Value::as_str) {
                Some("completed") => "[x]",
                _ => "[ ]",
            };
            Some(format!("- {mark} {content}"))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::run_registry::RunStreamRegistry;

    fn events(updates: &[Value]) -> Vec<Value> {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut rx = handle.subscribe();
        let mut console = AcpConsole::new("opencode", Some(handle.clone()));
        console.on_session("s1", Some("opencode/big-pickle"));
        for update in updates {
            console.on_update(update);
        }
        console.on_end();
        let mut out = Vec::new();
        while let Ok(message) = rx.try_recv() {
            if let LiveMessage::Event { event } = message {
                out.push(event);
            }
        }
        out
    }

    #[test]
    fn console_kind_names_round_trip() {
        use coppice_connectors::ConsoleKind;
        assert_eq!(
            crate::providers::console_kind_name(ConsoleKind::Structured),
            "structured"
        );
        assert_eq!(
            crate::providers::console_kind_name(ConsoleKind::OpenCodeSession),
            "openCodeSession"
        );
        for kind in [
            ConsoleKind::Structured,
            ConsoleKind::OpenCodeSession,
            ConsoleKind::Plain,
        ] {
            let name = crate::providers::console_kind_name(kind);
            assert_eq!(crate::providers::parse_console_kind(&name), Some(kind));
        }
        assert_eq!(crate::providers::parse_console_kind("hologram"), None);
    }

    #[test]
    fn recorded_2_0_20_turn_renders_thinking_tool_and_result() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/opencode-acp/opencode-2.0.20-read-only-turn.jsonl");
        let updates: Vec<Value> = std::fs::read_to_string(path)
            .expect("fixture")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("json"))
            .filter(|m| m.get("method").and_then(Value::as_str) == Some("session/update"))
            .filter_map(|m| m.pointer("/params/update").cloned())
            .collect();
        let got = events(&updates);
        let kinds: Vec<&str> = got.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"opencode.console.thinking"), "{kinds:?}");
        let tools: Vec<&Value> = got
            .iter()
            .filter(|e| e["type"] == "opencode.console.tool")
            .collect();
        assert_eq!(tools.first().unwrap()["title"], "read");
        let last = tools.last().unwrap();
        assert_eq!(last["status"], "completed");
        assert!(last["output"]
            .as_str()
            .unwrap()
            .contains("first line of readme"));
        let result = got
            .iter()
            .find(|e| e["type"] == "opencode.console.result")
            .expect("result");
        assert_eq!(result["contract"]["summary"], "first line of readme");
    }

    #[test]
    fn version_two_and_later_use_acp() {
        assert!(uses_acp("2.0.20"));
        assert!(uses_acp("3.1.0"));
        assert!(!uses_acp("1.19.0"));
        assert!(!uses_acp("0.9.1"));
    }

    #[test]
    fn model_value_joins_provider_and_model_once() {
        assert_eq!(
            model_value(Some("opencode"), Some("big-pickle")).as_deref(),
            Some("opencode/big-pickle")
        );
        assert_eq!(
            model_value(Some("opencode"), Some("opencode/big-pickle")).as_deref(),
            Some("opencode/big-pickle")
        );
        assert_eq!(
            model_value(None, Some("anthropic/claude")).as_deref(),
            Some("anthropic/claude")
        );
        assert_eq!(model_value(Some("opencode"), None), None);
        assert_eq!(model_value(Some("opencode"), Some("  ")), None);
    }

    #[test]
    fn chunks_become_whole_blocks_around_tools() {
        let got = events(&[
            json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "Let me " } }),
            json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "look." } }),
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Reading " } }),
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "the file." } }),
            json!({ "sessionUpdate": "tool_call", "toolCallId": "t1", "kind": "read", "status": "pending", "title": "read" }),
            json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed",
                    "content": [{ "type": "content", "content": { "type": "text", "text": "first line" } }] }),
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Done." } }),
            json!({ "sessionUpdate": "usage_update", "used": 10 }),
        ]);
        let kinds: Vec<&str> = got.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            [
                "opencode.console.session",
                "opencode.console.thinking",
                "opencode.console.text",
                "opencode.console.tool",
                "opencode.console.tool",
                "opencode.console.text"
            ]
        );
        assert_eq!(got[0]["model"], "opencode/big-pickle");
        assert_eq!(got[1]["text"], "Let me look.");
        assert_eq!(got[2]["markdown"], "Reading the file.");
        assert_eq!(got[3]["status"], "running");
        assert_eq!(got[3]["variant"], "action");
        assert_eq!(got[4]["status"], "completed");
        assert_eq!(got[4]["output"], "first line");
        assert_eq!(got[5]["markdown"], "Done.");
    }

    #[test]
    fn shell_tools_failures_and_plans() {
        let got = events(&[
            json!({ "sessionUpdate": "tool_call", "toolCallId": "t2", "kind": "execute", "status": "pending", "title": "touch x" }),
            json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t2", "status": "failed" }),
            json!({ "sessionUpdate": "plan", "entries": [
                { "content": "Read code", "status": "completed", "priority": "high" },
                { "content": "Write plan", "status": "pending", "priority": "high" }
            ] }),
        ]);
        assert_eq!(got[1]["variant"], "shell");
        assert_eq!(got[2]["status"], "error");
        assert_eq!(got[3]["markdown"], "- [x] Read code\n- [ ] Write plan");
    }

    #[test]
    fn a_result_contract_in_text_is_published_once_as_a_result() {
        let contract = r#"{"status":"done","summary":"All set"}"#;
        let got = events(&[
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": contract } }),
            json!({ "sessionUpdate": "tool_call", "toolCallId": "t3", "kind": "read", "title": "read" }),
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": contract } }),
        ]);
        let results = got
            .iter()
            .filter(|e| e["type"] == "opencode.console.result")
            .count();
        assert_eq!(results, 1);
        assert!(got.iter().all(|e| e["type"] != "opencode.console.text"));
    }
}
