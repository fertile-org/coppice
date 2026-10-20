//! The ACP connectors' shared runner and live console.
//!
//! A connector that speaks the Agent Client Protocol gives its command, its
//! flags for write and read-only runs, and how it names models; the turn,
//! the console, and how the run ends are the same for all of them.

use super::acp::{self, AcpLaunch, AcpLimits, AcpObserver, AcpTurn, ToolPolicy};
use super::{AgentRunInput, AgentRunResult, ProviderError};
use crate::sessions::opencode_events::extract_result_from_text;
use crate::sessions::run_registry::RunStreamHandle;
use crate::sessions::LiveMessage;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Longest tool output kept on one console entry.
const TOOL_OUTPUT_MAX: usize = 4000;

/// One ACP turn for a connector.
pub struct AcpRun<'a> {
    /// Shown in errors, e.g. "OpenCode".
    pub display_name: &'static str,
    /// Live console events are `<prefix>.console.*`.
    pub console_prefix: &'static str,
    pub program: &'a str,
    /// Arguments after the program, for this run's policy.
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub worktree: &'a Path,
    pub prompt: &'a str,
    pub run_timeout: Duration,
    /// The model's `configOptions` value, already in the connector's form.
    pub model: Option<String>,
    /// How to sign in, shown when the agent says it isn't.
    pub sign_in_hint: Option<String>,
    /// A `mode` session option to set, when the connector needs one.
    pub session_mode: Option<&'static str>,
}

pub async fn run(input: AgentRunInput, run: AcpRun<'_>) -> Result<AgentRunResult, ProviderError> {
    let launch = AcpLaunch {
        display_name: run.display_name.into(),
        program: run.program.to_string(),
        args: run.args,
        env: run.env,
        cwd: run.worktree.to_path_buf(),
        run_id: input.run_id.clone(),
        ticket_id: input.ticket_id.clone(),
        task_log_dir: super::cli_runner::run_log_dir(
            input.artifacts_dir.as_deref(),
            input.run_id.as_deref(),
        ),
        sign_in_hint: run.sign_in_hint,
    };
    let turn = AcpTurn {
        cwd: run.worktree,
        mcp: input.mcp.as_ref(),
        model: run.model.as_deref(),
        resume_session_id: input.resume_session_id.as_deref(),
        prompt: run.prompt,
        policy: if input.read_only_tools {
            ToolPolicy::ReadOnly
        } else {
            ToolPolicy::AllowAll
        },
        session_mode: run.session_mode,
    };
    let mut console = AcpConsole::new(run.console_prefix, input.stream.clone());
    let outcome = acp::run_turn(
        launch,
        turn,
        AcpLimits::for_run(run.run_timeout),
        &mut console,
        input.cancel_rx,
        input.session_created_tx,
    )
    .await?;

    if let Some(problem) = outcome.stop_problem(run.display_name) {
        // MissingResult, so a result already sent through result_submit still wins.
        return Err(ProviderError::MissingResult(problem));
    }
    let name = run.display_name;
    super::terminal::result_from_text(&outcome.agent_text, input.context_profile, name, || {
        format!("no result contract found in {name} output")
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
