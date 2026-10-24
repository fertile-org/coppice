//! OpenCode 2.x through `opencode run --format json`, for models its ACP server
//! does not offer.
//!
//! OpenCode 2.0.20's `opencode acp` lists only the `opencode/*` catalog, so a
//! signed-in provider such as `opencode-go` cannot be chosen there, while
//! `opencode run -m opencode-go/<model>` works. Such runs take this path:
//!
//! - `--standalone` starts a private server for this process. Without it the
//!   run attaches to OpenCode's shared background service, which never sees
//!   this run's `OPENCODE_CONFIG` (gateway and read-only deny rules).
//! - `--auto` approves permissions that are not denied, so the run never waits
//!   on a prompt. Read-only runs keep `edit`, `bash`, and `write` denied in the
//!   config; verified on 2.0.20 that `--auto` does not override a deny.
//! - The gateway is the `mcp` entry in `OPENCODE_CONFIG`, as on 1.x.
//! - The project folder comes from `PWD`, not the process's working
//!   directory, and `opencode run` has no `--dir` flag; `cli_runner` sets
//!   `PWD` to the worktree (`env_hygiene::work_in`).
//!
//! Events are JSON lines (`step_start`, `text`, `reasoning`, `tool_use`,
//! `step_finish`, `error`), each with `sessionID` and a `part`. They are turned
//! into ACP updates so the live console matches ACP runs.

use super::acp::AcpObserver;
use super::acp_connector::AcpConsole;
use super::cli_runner::{
    log_launch, run_cli, run_log_dir, CliError, CliInvocation, LineHandler, LineStep, RunIo,
};
use super::{AgentRunInput, AgentRunResult, ProviderError};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

const DISPLAY_NAME: &str = "OpenCode";
const CONSOLE_PREFIX: &str = "opencode";

pub struct RunSpec<'a> {
    pub program: &'a str,
    pub version: &'a str,
    pub config_path: &'a Path,
    pub worktree: &'a Path,
    pub prompt: &'a str,
    pub run_timeout: Duration,
    /// `provider/model`, as `-m` takes it.
    pub model: &'a str,
}

pub fn launch_args(model: &str, resume_session_id: Option<&str>, prompt: &str) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "--standalone".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--auto".to_string(),
        "--model".to_string(),
        model.to_string(),
    ];
    if let Some(sid) = resume_session_id.filter(|sid| !sid.is_empty()) {
        args.push("--session".to_string());
        args.push(sid.to_string());
    }
    // `--` so a prompt starting with `-` is not read as a flag.
    args.push("--".to_string());
    args.push(prompt.to_string());
    args
}

pub async fn run(input: AgentRunInput, spec: RunSpec<'_>) -> Result<AgentRunResult, ProviderError> {
    let args = launch_args(spec.model, input.resume_session_id.as_deref(), spec.prompt);
    let mut env = vec![(
        "OPENCODE_CONFIG".to_string(),
        spec.config_path.to_string_lossy().into_owned(),
    )];
    if let Some(access) = &input.mcp {
        env.extend(access.env().map(|(k, v)| (k.to_string(), v)));
    }
    if input.read_only_tools {
        // A checkout's opencode.json could allow edit and bash back.
        env.push(("OPENCODE_DISABLE_PROJECT_CONFIG".into(), "1".into()));
    }
    log_launch(
        spec.program,
        spec.version,
        &args,
        run_log_dir(input.artifacts_dir.as_deref(), input.run_id.as_deref()).as_deref(),
    );
    let invocation = CliInvocation {
        program: spec.program.to_string(),
        args,
        env,
        cwd: spec.worktree.to_path_buf(),
        timeout: spec.run_timeout,
        run_id: input.run_id.clone(),
        artifacts_dir: input.artifacts_dir.clone(),
        ticket_id: input.ticket_id.clone(),
        result_before_exit: false,
        result_exit_grace: None,
        stall_limits: None,
    };
    let mut lines = RunLines::new(
        AcpConsole::new(CONSOLE_PREFIX, input.stream.clone()),
        spec.model,
    );
    let io = RunIo {
        cancel_rx: input.cancel_rx,
        session_created_tx: input.session_created_tx,
    };
    let exit = run_cli(invocation, &mut lines, io).await;
    lines.console.on_end();
    let exit = match exit {
        Ok(exit) => exit,
        Err(CliError::Spawn(e) | CliError::Io(e)) => return Err(ProviderError::Io(e)),
        Err(CliError::Cancelled) => return Err(ProviderError::Cancelled),
        Err(CliError::Stalled {
            reason,
            stderr_tail,
        }) => {
            return Err(super::terminal::stalled(
                DISPLAY_NAME,
                &reason,
                &stderr_tail,
            ));
        }
        Err(CliError::TimedOut { stderr_tail }) => {
            return Err(ProviderError::InvalidFixture(format!(
                "opencode run timed out after {}s{}",
                spec.run_timeout.as_secs(),
                super::terminal::stderr_suffix(&stderr_tail)
            )));
        }
    };

    if !exit.status.success() {
        let why = lines
            .error
            .clone()
            .unwrap_or_else(|| format!("exited with status {}", exit.status));
        return Err(ProviderError::InvalidFixture(format!(
            "opencode run failed: {why}{}",
            super::terminal::stderr_suffix(&exit.stderr_tail)
        )));
    }
    let stderr = super::terminal::stderr_suffix(&exit.stderr_tail);
    super::terminal::result_from_text(&lines.text, input.context_profile, DISPLAY_NAME, || {
        match &lines.error {
            Some(error) => format!("opencode run reported an error: {error}{stderr}"),
            None => format!("no result contract found in {DISPLAY_NAME} output{stderr}"),
        }
    })
}

struct RunLines {
    console: AcpConsole,
    model: String,
    session_announced: bool,
    /// Text since the console last flushed; the next whole part needs a break.
    pending_text: bool,
    /// The agent's text parts, in order, for the result contract.
    text: String,
    error: Option<String>,
}

impl RunLines {
    fn new(console: AcpConsole, model: &str) -> Self {
        Self {
            console,
            model: model.to_string(),
            session_announced: false,
            pending_text: false,
            text: String::new(),
            error: None,
        }
    }

    fn on_text(&mut self, text: &str, update: &str) {
        if text.trim().is_empty() {
            return;
        }
        // Parts arrive whole, not as chunks, so separate them.
        let chunk = if self.pending_text {
            format!("\n\n{text}")
        } else {
            text.to_string()
        };
        self.pending_text = true;
        self.console.on_update(&json!({
            "sessionUpdate": update,
            "content": { "type": "text", "text": chunk },
        }));
    }
}

impl LineHandler for RunLines {
    fn on_json(&mut self, value: &Value) -> LineStep {
        let session_id = value
            .get("sessionID")
            .and_then(Value::as_str)
            .filter(|sid| !sid.is_empty())
            .map(str::to_string);
        if let Some(sid) = &session_id {
            if !self.session_announced {
                self.session_announced = true;
                self.console.on_session(sid, Some(&self.model));
            }
        }
        let part = value.get("part").unwrap_or(&Value::Null);
        match value.get("type").and_then(Value::as_str).unwrap_or("") {
            "text" => {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    if !self.text.is_empty() {
                        self.text.push('\n');
                    }
                    self.text.push_str(text);
                    self.on_text(text, "agent_message_chunk");
                }
            }
            "reasoning" => {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    self.on_text(text, "agent_thought_chunk");
                }
            }
            "tool_use" => {
                if let Some(update) = tool_update(part) {
                    self.pending_text = false;
                    self.console.on_update(&update);
                }
            }
            "step_finish" => {
                self.pending_text = false;
                self.console.on_end();
            }
            "error" => self.error = Some(error_message(value)),
            _ => {}
        }
        LineStep {
            session_id,
            stop: false,
        }
    }
}

/// A `tool` part as an ACP `tool_call`.
fn tool_update(part: &Value) -> Option<Value> {
    let id = part
        .get("callID")
        .or_else(|| part.get("id"))
        .and_then(Value::as_str)?;
    let tool = part.get("tool").and_then(Value::as_str).unwrap_or("tool");
    let state = part.get("state").unwrap_or(&Value::Null);
    let status = match state.get("status").and_then(Value::as_str) {
        Some("completed") => "completed",
        Some("error") => "failed",
        _ => "in_progress",
    };
    let shell = matches!(tool, "shell" | "bash");
    let output = state
        .get("output")
        .or_else(|| state.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("");
    Some(json!({
        "sessionUpdate": "tool_call",
        "toolCallId": id,
        "status": status,
        "title": tool_title(tool, shell, state.get("input")),
        "kind": if shell { "execute" } else { "other" },
        "content": [{ "type": "content", "content": { "type": "text", "text": output } }],
    }))
}

/// The command for a shell call, otherwise the tool and its main argument.
/// OpenCode 2 reaches MCP tools through `execute`, whose argument is the
/// script (`tools.coppice.result_submit(…)`). Titles keep the first line.
fn tool_title(tool: &str, shell: bool, input: Option<&Value>) -> String {
    let arg = [
        "command", "filePath", "path", "pattern", "url", "query", "code",
    ]
    .iter()
    .find_map(|key| input?.get(*key)?.as_str())
    .and_then(|arg| arg.lines().map(str::trim).find(|line| !line.is_empty()));
    match arg {
        Some(arg) if shell => arg.to_string(),
        Some(arg) => format!("{tool} {arg}"),
        None => tool.to_string(),
    }
}

fn error_message(value: &Value) -> String {
    let error = value.get("error").unwrap_or(value);
    for pointer in ["/data/message", "/message", ""] {
        if let Some(text) = error.pointer(pointer).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return text.trim().to_string();
            }
        }
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::run_registry::RunStreamRegistry;
    use crate::sessions::LiveMessage;

    #[test]
    fn launch_args_run_standalone_with_the_model_and_resume() {
        assert_eq!(
            launch_args("opencode-go/kimi-k2.7-code", Some("ses_1"), "-do it"),
            vec![
                "run",
                "--standalone",
                "--format",
                "json",
                "--auto",
                "--model",
                "opencode-go/kimi-k2.7-code",
                "--session",
                "ses_1",
                "--",
                "-do it",
            ]
        );
        assert!(!launch_args("m/x", Some(""), "p").contains(&"--session".to_string()));
    }

    /// Lines captured from `opencode run --format json` 2.0.20, trimmed.
    fn captured_run() -> Vec<Value> {
        let session = "ses_e9628f0caffeRakB0qgjIqW843";
        vec![
            json!({ "type": "step_start", "sessionID": session,
                "part": { "type": "step-start", "sessionID": session } }),
            json!({ "type": "tool_use", "sessionID": session, "part": {
                "type": "tool", "id": "read_0", "tool": "read",
                "state": { "status": "completed", "input": { "path": "note.txt" },
                    "output": "1: hello from file", "title": "read" } } }),
            json!({ "type": "tool_use", "sessionID": session, "part": {
                "type": "tool", "id": "shell_1", "tool": "shell",
                "state": { "status": "error", "input": { "command": "echo hi" },
                    "error": "denied" } } }),
            json!({ "type": "step_finish", "sessionID": session,
                "part": { "type": "step-finish", "reason": "tool-calls" } }),
            json!({ "type": "text", "sessionID": session,
                "part": { "type": "text", "text": "Read it." } }),
            json!({ "type": "step_finish", "sessionID": session,
                "part": { "type": "step-finish", "reason": "stop" } }),
            json!({ "type": "text", "sessionID": session,
                "part": { "type": "text", "text": "{\"status\":\"done\",\"summary\":\"All set\"}" } }),
        ]
    }

    fn feed(lines: &[Value]) -> (RunLines, Vec<Value>, Vec<Option<String>>) {
        let registry = RunStreamRegistry::new();
        let stream = registry.register(uuid::Uuid::new_v4());
        let mut rx = stream.subscribe();
        let mut handler = RunLines::new(
            AcpConsole::new(CONSOLE_PREFIX, Some(stream)),
            "opencode-go/kimi-k2.7-code",
        );
        let sessions = lines
            .iter()
            .map(|line| handler.on_json(line).session_id)
            .collect();
        handler.console.on_end();
        let mut events = Vec::new();
        while let Ok(message) = rx.try_recv() {
            if let LiveMessage::Event { event } = message {
                events.push(event);
            }
        }
        (handler, events, sessions)
    }

    #[test]
    fn events_become_the_opencode_console() {
        let (handler, events, sessions) = feed(&captured_run());

        assert!(sessions
            .iter()
            .all(|sid| sid.as_deref() == Some("ses_e9628f0caffeRakB0qgjIqW843")));
        let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            vec![
                "opencode.console.session",
                "opencode.console.tool",
                "opencode.console.tool",
                "opencode.console.text",
                "opencode.console.result",
            ]
        );
        assert_eq!(events[0]["model"], "opencode-go/kimi-k2.7-code");
        assert_eq!(events[1]["title"], "read note.txt");
        assert_eq!(events[1]["status"], "completed");
        assert_eq!(events[1]["output"], "1: hello from file");
        assert_eq!(events[2]["title"], "echo hi");
        assert_eq!(events[2]["variant"], "shell");
        assert_eq!(events[2]["status"], "error");
        assert_eq!(events[3]["markdown"], "Read it.");
        assert_eq!(events[4]["contract"]["summary"], "All set");
        assert!(handler
            .text
            .ends_with(r#"{"status":"done","summary":"All set"}"#));
        assert!(handler.error.is_none());
    }

    #[test]
    fn a_contract_on_its_own_is_published_as_the_result() {
        let session = "ses_1";
        let (_, events, _) = feed(&[json!({ "type": "text", "sessionID": session,
            "part": { "type": "text", "text": "{\"status\":\"done\",\"summary\":\"All set\"}" } })]);
        assert_eq!(events.last().unwrap()["type"], "opencode.console.result");
        assert_eq!(events.last().unwrap()["contract"]["summary"], "All set");
    }

    #[test]
    fn execute_calls_are_titled_by_their_first_line() {
        let input =
            json!({ "code": "\n  tools.coppice.result_submit({ summary: 'ok' });\nmore();" });
        assert_eq!(
            tool_title("execute", false, Some(&input)),
            "execute tools.coppice.result_submit({ summary: 'ok' });"
        );
        assert_eq!(tool_title("todo", false, None), "todo");
    }

    #[test]
    fn error_events_keep_their_message() {
        let (handler, _, _) = feed(&[json!({ "type": "error", "sessionID": "ses_1",
            "error": { "name": "ProviderError", "data": { "message": "model not found" } } })]);
        assert_eq!(handler.error.as_deref(), Some("model not found"));
    }
}
