//! OpenCode 2.x over ACP (`opencode acp`).
//!
//! OpenCode 2 moved its HTTP API under `/api/` behind a password and changed
//! its shape, so the 1.x `serve` client cannot talk to it. Its ACP server is a
//! standard protocol instead: Coppice passes the gateway in `session/new`,
//! keeps read-only as deny rules in `OPENCODE_CONFIG` (still honoured by
//! `opencode acp` 2.0.20), and gets updates it turns into the structured live
//! console the other connectors use.

use super::acp_connector::AcpRun;
use super::{AgentRunInput, AgentRunResult, ProviderError};
use std::path::Path;
use std::time::Duration;

const DISPLAY_NAME: &str = "OpenCode";
const CONSOLE_PREFIX: &str = "opencode";

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
    let model = model_value(input.model_provider.as_deref(), input.model.as_deref());
    super::acp_connector::run(
        input,
        AcpRun {
            display_name: DISPLAY_NAME,
            console_prefix: CONSOLE_PREFIX,
            program: spec.program,
            args: vec!["acp".into()],
            env,
            worktree: spec.worktree,
            prompt: spec.prompt,
            run_timeout: spec.run_timeout,
            model,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::acp::AcpObserver;
    use crate::providers::acp_connector::AcpConsole;
    use crate::sessions::run_registry::RunStreamRegistry;
    use crate::sessions::LiveMessage;
    use serde_json::Value;

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
}
