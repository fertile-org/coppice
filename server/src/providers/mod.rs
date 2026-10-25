pub mod acp;
pub mod acp_cli;
pub mod acp_connector;
pub mod claude_code;
pub mod claude_console;
pub mod claude_hook;
pub mod cli_catalog;
pub mod cli_runner;
pub mod cli_version;
pub mod codex;
pub mod codex_console;
pub mod codex_models;
pub mod env_hygiene;
pub mod cursor;
pub mod cursor_console;
pub mod cursor_models;
pub mod kilo_code;
pub mod kilo_console;
pub mod kilo_models;
pub mod line_reader;
#[cfg(feature = "mock-provider")]
pub mod mock;
pub mod model_flag;
pub mod models;
pub mod opencode;
pub mod opencode_acp;
pub mod opencode_cli;
pub mod opencode_models;
pub mod registry;
pub mod reported_model;
pub mod resume;
pub mod terminal;

pub use coppice_connectors::{ConnectorDescriptor, ToolNameStyle};
pub use models::{ModelCatalog, ModelInfo};
pub use registry::ConnectorRegistry;

pub fn descriptor(id: &str) -> Option<&'static ConnectorDescriptor> {
    coppice_connectors::get(id)
}

/// How the connector names gateway tools in its output; drives console titles.
pub fn tool_name_style(id: &str) -> ToolNameStyle {
    descriptor(id).map_or(ToolNameStyle::None, |d| d.mcp_tool_names)
}

/// Temporary alias for gradual migration.
pub type ProviderRegistry = ConnectorRegistry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::watch;

use crate::domain::context_profile::ContextProfile;
use crate::domain::substatus::TicketStatus;
use crate::domain::workflow::SplitTicketSpec;
pub use crate::knowledge::candidates::KnowledgeCandidateSpec;
use crate::sessions::run_registry::RunStreamHandle;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
    #[serde(default)]
    pub agent_key: String,
    #[serde(default)]
    pub intent: String,
    #[serde(default)]
    pub request: String,
}

#[derive(Clone)]
pub struct AgentRunInput {
    pub agent_id: String,
    pub agent_key: String,
    pub agent_role: String,
    pub job_type: String,
    pub ticket_id: Option<String>,
    pub ticket_status: Option<TicketStatus>,
    pub context_profile: ContextProfile,
    pub context_path: String,
    pub run_id: Option<String>,
    /// Set on chat turns. Connectors that keep CLI state on disk key it by the
    /// chat session so `--resume` finds the previous turn.
    pub chat_session_id: Option<String>,
    pub artifacts_dir: Option<String>,
    pub stream: Option<Arc<RunStreamHandle>>,
    pub cancel_rx: Option<watch::Receiver<bool>>,
    pub model_provider: Option<String>,
    pub model: Option<String>,
    pub session_created_tx: Option<watch::Sender<String>>,
    pub resume_context: Option<String>,
    pub resume_session_id: Option<String>,
    /// When true, connectors must enforce a read-only tool allowlist or fail closed.
    pub read_only_tools: bool,
    /// Gateway URL + per-run token; `None` when no run/token exists (probes, drafts, tests).
    pub mcp: Option<crate::mcp::grant::McpAccess>,
}

/// Read-only allowlist for connectors that can enforce it (claude-code).
pub const CHAT_READ_ONLY_TOOLS: &str = "Read,Glob,Grep,WebFetch,WebSearch";

/// Build an **absolute** path under the artifacts dir.
///
/// `storage.artifacts_dir` is usually relative (`./data/artifacts`), and every
/// subprocess connector spawns the CLI with `current_dir(worktree)`. A relative
/// path handed to a child — `HOME`, `--mcp-config`, `KILO_CONFIG` — would
/// resolve against the worktree, writing MCP config into the repo checkout and
/// losing the gateway. Absolutize once, here, against the server's cwd.
pub fn artifacts_path(artifacts_dir: &str, segments: &[&str]) -> std::io::Result<PathBuf> {
    let base = if artifacts_dir.is_empty() {
        Path::new(".")
    } else {
        Path::new(artifacts_dir)
    };
    let mut path = std::path::absolute(base)?;
    path.extend(segments);
    Ok(path)
}

/// Absolute `<artifacts_dir>/runs/<run_id>/`. The only place a connector may
/// write per-run MCP configuration: never the worktree, a registered repo
/// checkout, or the user's global CLI config.
pub fn run_dir(input: &AgentRunInput, connector: &str) -> Result<PathBuf, ProviderError> {
    let (Some(artifacts_dir), Some(run_id)) = (&input.artifacts_dir, &input.run_id) else {
        return Err(mcp_unavailable(&format!(
            "{connector} has no run artifacts dir for its MCP configuration"
        )));
    };
    Ok(artifacts_path(artifacts_dir, &["runs", run_id])?)
}

/// A run that has a gateway token but no way to hand it to the CLI must fail
/// loudly — there is no fallback to the fat context.
/// The console a run on `connector` produces when the installed CLI version
/// decides it, or `None` to use the connector's default.
pub async fn run_console_kind(
    connector: &str,
    config: &coppice_config::AppConfig,
) -> Option<coppice_connectors::ConsoleKind> {
    if connector != coppice_connectors::OPENCODE {
        return None;
    }
    let version = cli_version::detect(&config.agent.connectors.opencode.command)
        .await
        .ok()?;
    Some(if opencode_acp::uses_acp(&version) {
        coppice_connectors::ConsoleKind::Structured
    } else {
        coppice_connectors::ConsoleKind::OpenCodeSession
    })
}

/// The sentence to fail a run with when the installed CLI is older than the
/// connector's floor. `None` when it is new enough or its version is unknown.
pub async fn below_min_version(connector: &str, config: &coppice_config::AppConfig) -> Option<String> {
    let descriptor = descriptor(connector)?;
    descriptor.min_version?;
    let command = config
        .agent
        .connectors
        .command(connector)
        .unwrap_or(descriptor.binary);
    let version = cli_version::detect(command).await.ok()?;
    coppice_connectors::check_min_version(descriptor, Some(&version)).err()
}

/// `ConsoleKind` as stored on a run and sent to the web app.
pub fn console_kind_name(kind: coppice_connectors::ConsoleKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub fn parse_console_kind(name: &str) -> Option<coppice_connectors::ConsoleKind> {
    use coppice_connectors::ConsoleKind;
    [ConsoleKind::OpenCodeSession, ConsoleKind::Structured, ConsoleKind::Plain]
        .into_iter()
        .find(|kind| console_kind_name(*kind) == name)
}

pub fn mcp_unavailable(reason: &str) -> ProviderError {
    ProviderError::InvalidInput(format!("mcp_unavailable: {reason}"))
}

/// Connectors whose descriptor has `caps.read_only_tools`, in the order shown
/// to users (settings API, compaction error). Compaction and plan use this
/// list. Chat does not: Claude Code, Cursor, and Kilo Code stay on with the
/// untested banner. Cursor is omitted here because `--mode ask` was not shown
/// to block shell writes. Kilo Code's ask agent is the same. OpenCode is
/// included: its per-run deny was. Codex is included: `--sandbox read-only`
/// blocked a shell write and a file write on 0.163.0.
pub const READ_ONLY_CAPABLE_CONNECTORS: &[&str] = &[
    #[cfg(feature = "mock-provider")]
    coppice_connectors::MOCK,
    coppice_connectors::CLAUDE_CODE,
    coppice_connectors::CODEX,
    coppice_connectors::OPENCODE,
    coppice_connectors::COPILOT,
    coppice_connectors::QWEN_CODE,
    coppice_connectors::KIMI,
    coppice_connectors::GROK,
];

pub fn connector_enforces_read_only(connector: &str) -> bool {
    descriptor(connector).is_some_and(|d| d.caps.read_only_tools)
}

pub fn chat_read_only_status(connector: &str) -> coppice_connectors::ChatReadOnlyStatus {
    coppice_connectors::chat_read_only_status(connector)
}

/// OpenCode passed a live write test. Mock does not write. This is not a working-directory selector.
pub fn chat_read_only_verified(connector: &str) -> bool {
    chat_read_only_status(connector) == coppice_connectors::ChatReadOnlyStatus::Passed
}

/// Whether this connector's read-only mode has not been shown to hold.
/// Chat working directories are the per-session folder for every connector,
/// including ones whose read-only mode was verified. This flag is not a
/// working-directory selector.
pub fn chat_needs_isolation(connector: &str) -> bool {
    !chat_read_only_verified(connector)
}

fn connector_display_name(connector: &str) -> &str {
    descriptor(connector)
        .map(|d| d.display_name)
        .unwrap_or(connector)
}

fn chat_banner_for(name: &str, status: coppice_connectors::ChatReadOnlyStatus) -> Option<String> {
    use coppice_connectors::ChatReadOnlyStatus;
    match status {
        ChatReadOnlyStatus::Passed | ChatReadOnlyStatus::Unavailable => None,
        ChatReadOnlyStatus::Untested => Some(format!(
            "Coppice hasn't confirmed that {name} stays read-only. The agent can still run commands on your computer."
        )),
    }
}

/// Warning on chats whose CLI has not been shown to stay read-only. `None`
/// when chat is off or the write test passed.
pub fn chat_isolation_banner(connector: &str) -> Option<String> {
    chat_banner_for(
        connector_display_name(connector),
        chat_read_only_status(connector),
    )
}

/// User-facing reason Chat cannot start. `None` when this connector may chat.
/// Unknown connectors stay unavailable. Cursor and Kilo Code may chat.
pub fn chat_unavailable_message(connector: &str) -> Option<String> {
    if chat_read_only_status(connector) != coppice_connectors::ChatReadOnlyStatus::Unavailable {
        return None;
    }
    let name = connector_display_name(connector);
    Some(format!(
        "Chat isn't available for {name} yet, because Coppice can't keep it read-only."
    ))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentRunResult {
    Done {
        summary: String,
        #[serde(default, rename = "changedFiles")]
        changed_files: Vec<String>,
        #[serde(default, rename = "testsRun")]
        tests_run: Vec<String>,
        #[serde(default, rename = "nextStatus")]
        next_status: Option<String>,
        #[serde(default, rename = "assignTo")]
        assign_to: Option<String>,
        #[serde(default, rename = "updatedDescription")]
        updated_description: Option<String>,
        #[serde(default, rename = "acceptanceCriteria")]
        acceptance_criteria: Option<String>,
        #[serde(default, rename = "mentionAgents")]
        mention_agents: Vec<String>,
        #[serde(default, rename = "agentRequests")]
        agent_requests: Vec<AgentRequest>,
        #[serde(default)]
        blockers: Vec<String>,
        #[serde(default, rename = "splitTickets")]
        split_tickets: Vec<SplitTicketSpec>,
        #[serde(default, rename = "knowledgeCandidates")]
        knowledge_candidates: Vec<KnowledgeCandidateSpec>,
        /// Short Human Review note. Absent when the connector did not report a brief.
        #[serde(default, rename = "howItWorks", skip_serializing_if = "Option::is_none")]
        how_it_works: Option<String>,
        /// What changed from the approved plan. Empty means nothing changed.
        #[serde(default, rename = "changesFromPlan", skip_serializing_if = "Option::is_none")]
        changes_from_plan: Option<String>,
        /// Where a reviewer should look first.
        #[serde(default, rename = "lookHereFirst", skip_serializing_if = "Option::is_none")]
        look_here_first: Option<String>,
        /// One entry per plan acceptance criterion. Malformed entries and
        /// unknown statuses are read leniently; a missing list is empty.
        #[serde(
            default,
            rename = "criteriaResults",
            deserialize_with = "lenient_criteria_results",
            skip_serializing_if = "Vec::is_empty"
        )]
        criteria_results: Vec<CriterionResult>,
        /// Planning runs only: 1–3 questions instead of a plan.
        #[serde(
            default,
            rename = "planQuestions",
            deserialize_with = "lenient_plan_questions",
            skip_serializing_if = "Vec::is_empty"
        )]
        plan_questions: Vec<PlanQuestion>,
        /// Agent-chat turns only: proposed ticket/knowledge items for the human to confirm.
        /// Kept raw so validation can report the specific `parse_chat_actions` error.
        #[serde(default, rename = "actions", skip_serializing_if = "Option::is_none")]
        actions: Option<Box<serde_json::Value>>,
    },
    Blocked {
        #[serde(rename = "blockerType")]
        blocker_type: String,
        summary: String,
        #[serde(default, rename = "nextStatus")]
        next_status: Option<String>,
        #[serde(default, rename = "assignTo")]
        assign_to: Option<String>,
        #[serde(default, rename = "updatedDescription")]
        updated_description: Option<String>,
        #[serde(default, rename = "acceptanceCriteria")]
        acceptance_criteria: Option<String>,
        #[serde(rename = "mentionAgents")]
        mention_agents: Vec<String>,
        #[serde(default, rename = "requiredCapabilities")]
        required_capabilities: Vec<String>,
        #[serde(default, rename = "requiredSecrets")]
        required_secrets: Vec<String>,
    },
    Continued {
        summary: String,
        #[serde(default, rename = "progressNote")]
        progress_note: Option<String>,
        #[serde(default, rename = "changedFiles")]
        changed_files: Vec<String>,
        #[serde(default, rename = "testsRun")]
        tests_run: Vec<String>,
        #[serde(default)]
        blockers: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionStatus {
    Met,
    NotMet,
    NotChecked,
}

impl CriterionStatus {
    /// Unknown words read as not checked.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "met" | "pass" | "passed" | "done" => Self::Met,
            "not_met" | "fail" | "failed" | "unmet" => Self::NotMet,
            _ => Self::NotChecked,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionResult {
    pub criterion: String,
    pub status: CriterionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanQuestion {
    pub question: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
}

fn lenient_criteria_results<'de, D>(deserializer: D) -> Result<Vec<CriterionResult>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            let criterion = item.get("criterion")?.as_str()?.trim();
            if criterion.is_empty() {
                return None;
            }
            let status = item
                .get("status")
                .and_then(serde_json::Value::as_str)
                .map(CriterionStatus::parse)
                .unwrap_or(CriterionStatus::NotChecked);
            let evidence = item
                .get("evidence")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string);
            Some(CriterionResult {
                criterion: criterion.to_string(),
                status,
                evidence,
            })
        })
        .collect())
}

fn lenient_plan_questions<'de, D>(deserializer: D) -> Result<Vec<PlanQuestion>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            let (question, choices) = match item {
                serde_json::Value::String(text) => (text.as_str(), Vec::new()),
                serde_json::Value::Object(_) => (
                    item.get("question")?.as_str()?,
                    item.get("choices")
                        .and_then(serde_json::Value::as_array)
                        .map(|choices| {
                            choices
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .map(str::trim)
                                .filter(|choice| !choice.is_empty())
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                ),
                _ => return None,
            };
            let question = question.trim();
            (!question.is_empty()).then(|| PlanQuestion {
                question: question.to_string(),
                choices,
            })
        })
        .collect())
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("fixture not found: {0}")]
    FixtureNotFound(String),
    #[error("invalid fixture: {0}")]
    InvalidFixture(String),
    /// The connector finished without a parseable final result. A result
    /// submitted through `result_submit` may still exist.
    #[error("no result submitted: {0}")]
    MissingResult(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("run cancelled")]
    Cancelled,
    #[error("resume session invalid: {0}")]
    ResumeSessionInvalid(String),
}

pub fn connector_supports_chat_resume(connector: &str) -> bool {
    descriptor(connector).is_some_and(|d| d.caps.chat_resume)
}

pub fn is_resume_session_invalid(err: &ProviderError) -> bool {
    match err {
        ProviderError::ResumeSessionInvalid(_) => true,
        ProviderError::InvalidInput(msg)
        | ProviderError::InvalidFixture(msg)
        | ProviderError::MissingResult(msg) => {
            // Claude Code prints "No conversation found" on stderr, which
            // the failure message now carries.
            resume::session_is_unusable(msg) || msg.to_ascii_lowercase().contains("invalid resume")
        }
        _ => false,
    }
}

#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError>;
}

#[cfg(any(test, feature = "mock-provider"))]
pub fn fixtures_root() -> PathBuf {
    fixtures_root_from(std::env::var("COPPICE_MOCK_FIXTURES_DIR").ok().as_deref())
}

/// Packaged builds have no source tree, so the env override must win when set.
#[cfg(any(test, feature = "mock-provider"))]
pub fn fixtures_root_from(env: Option<&str>) -> PathBuf {
    match env {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/agent-responses"),
    }
}

/// Resolve the ticket worktree directory from `.agent/context.md` and return an
/// absolute path.
///
/// Config often uses relative `worktrees_path` (e.g. `./data/worktrees`). Providers
/// that both `current_dir(worktree)` and pass that same relative path as a CLI
/// flag (`--workspace`, `-C`, …) make the child resolve the flag against the new
/// cwd and look for a duplicated path. Canonicalizing once here keeps every
/// connector safe.
pub fn worktree_dir_from_context(context_path: &str) -> Result<PathBuf, ProviderError> {
    let context_path = PathBuf::from(context_path);
    let worktree = context_path
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| ProviderError::InvalidInput("bad context path".into()))?;
    absolute_existing_dir(worktree)
}

pub fn absolute_existing_dir(path: &std::path::Path) -> Result<PathBuf, ProviderError> {
    std::fs::canonicalize(path).map_err(|err| {
        ProviderError::Io(std::io::Error::new(
            err.kind(),
            format!(
                "worktree path `{}` is not an accessible directory: {err}",
                path.display()
            ),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "mock-provider")]
    use crate::providers::mock::{mock_env_lock, MockProvider};

    /// A run input with no run, no artifacts dir and no gateway — the shape a
    /// probe or draft uses. Tests override just the fields they care about.
    fn probe_input() -> AgentRunInput {
        AgentRunInput {
            agent_id: "agent-1".into(),
            agent_key: "agent-1".into(),
            agent_role: "Backend Engineer".into(),
            job_type: "work_on_ticket".into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::Full,
            context_path: "/tmp".into(),
            run_id: None,
            chat_session_id: None,
            artifacts_dir: None,
            stream: None,
            cancel_rx: None,
            model_provider: None,
            model: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: false,
            mcp: None,
        }
    }

    #[test]
    fn fixtures_root_from_uses_env_override() {
        assert_eq!(fixtures_root_from(Some("/x/f")), PathBuf::from("/x/f"));
    }

    #[test]
    fn fixtures_root_from_falls_back_to_source_tree() {
        assert!(fixtures_root_from(Some("")).ends_with("fixtures/agent-responses"));
        assert!(fixtures_root_from(None).ends_with("fixtures/agent-responses"));
    }

    #[test]
    fn resume_invalid_detection() {
        assert!(is_resume_session_invalid(
            &ProviderError::ResumeSessionInvalid("x".into())
        ));
        assert!(is_resume_session_invalid(&ProviderError::InvalidInput(
            "session not found".into()
        )));
        assert!(is_resume_session_invalid(&ProviderError::MissingResult(
            "Error: unknown session abc".into()
        )));
        assert!(!is_resume_session_invalid(&ProviderError::MissingResult(
            "no final result".into()
        )));
        assert!(!is_resume_session_invalid(&ProviderError::Cancelled));
        // Claude Code's missing transcript, carried in the failure's stderr.
        assert!(is_resume_session_invalid(&ProviderError::InvalidFixture(
            "Claude Code exited with exit status: 1 before finishing (stderr: No conversation found with session ID: 9f0)".into()
        )));
        // Not cured by a new session: keep the session for the next turn.
        for msg in [
            "Claude Code exited with exit status: 1 before finishing (stderr: connect ECONNREFUSED)",
            "Codex ended with an error: 429 Too Many Requests",
            "Your session has expired. Please run claude login.",
        ] {
            assert!(!is_resume_session_invalid(&ProviderError::InvalidFixture(msg.into())), "{msg}");
        }
    }

    #[test]
    fn chat_resume_connectors_include_all_vendor_chat_clis() {
        #[cfg(feature = "mock-provider")]
        assert!(connector_supports_chat_resume("mock"));
        #[cfg(not(feature = "mock-provider"))]
        assert!(!connector_supports_chat_resume("mock"));
        for id in ["opencode", "claude-code", "cursor", "codex", "kilo-code"] {
            assert!(connector_supports_chat_resume(id));
        }
        assert!(!connector_supports_chat_resume("unknown"));
    }

    #[test]
    fn read_only_capable_connectors_match_descriptor_caps() {
        let mut listed: Vec<_> = READ_ONLY_CAPABLE_CONNECTORS.to_vec();
        listed.sort();
        let mut from_caps: Vec<_> = coppice_connectors::all()
            .iter()
            .filter(|d| d.caps.read_only_tools)
            .map(|d| d.id)
            .collect();
        from_caps.sort();
        assert_eq!(listed, from_caps);
        for id in READ_ONLY_CAPABLE_CONNECTORS {
            assert!(connector_enforces_read_only(id));
        }
        assert!(connector_enforces_read_only("codex"));
        assert!(!connector_enforces_read_only("cursor"));
        assert!(connector_enforces_read_only("opencode"));
        assert!(!connector_enforces_read_only("kilo-code"));
    }

    #[test]
    fn chat_status_sets_banner_isolation_and_refusal() {
        use coppice_connectors::ChatReadOnlyStatus;

        let untested = [
            (coppice_connectors::CLAUDE_CODE, "Claude Code"),
            (coppice_connectors::CURSOR, "Cursor"),
            (coppice_connectors::KILO_CODE, "Kilo Code"),
        ];
        for (id, name) in untested {
            assert_eq!(chat_read_only_status(id), ChatReadOnlyStatus::Untested);
            assert!(chat_needs_isolation(id), "{id}");
            assert!(chat_unavailable_message(id).is_none(), "{id}");
            let expected = format!(
                "Coppice hasn't confirmed that {name} stays read-only. The agent can still run commands on your computer."
            );
            assert_eq!(chat_isolation_banner(id).as_deref(), Some(expected.as_str()));
        }

        assert_eq!(
            chat_read_only_status("not-a-connector"),
            ChatReadOnlyStatus::Unavailable
        );
        assert!(chat_isolation_banner("not-a-connector").is_none());
        assert_eq!(
            chat_unavailable_message("not-a-connector").as_deref(),
            Some(
                "Chat isn't available for not-a-connector yet, because Coppice can't keep it read-only."
            )
        );

        assert_eq!(
            chat_read_only_status(coppice_connectors::OPENCODE),
            ChatReadOnlyStatus::Passed
        );
        assert!(!chat_needs_isolation(coppice_connectors::OPENCODE));
        assert!(chat_isolation_banner(coppice_connectors::OPENCODE).is_none());
        assert!(chat_unavailable_message(coppice_connectors::OPENCODE).is_none());
        assert_eq!(
            chat_read_only_status(coppice_connectors::CODEX),
            ChatReadOnlyStatus::Passed
        );
        assert!(!chat_needs_isolation(coppice_connectors::CODEX));
        assert!(chat_isolation_banner(coppice_connectors::CODEX).is_none());
        assert!(chat_unavailable_message(coppice_connectors::CODEX).is_none());
        assert_eq!(
            chat_read_only_status(coppice_connectors::MOCK),
            ChatReadOnlyStatus::Passed
        );
        assert!(chat_isolation_banner(coppice_connectors::MOCK).is_none());
    }

    #[test]
    fn agent_run_result_deserializes_done_fixture() {
        let path = fixtures_root().join("done.json");
        let raw = std::fs::read_to_string(path).expect("read done fixture");
        let result: AgentRunResult = serde_json::from_str(&raw).expect("deserialize done");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Mock implementation complete.");
            }
            _ => panic!("expected done variant"),
        }
    }

    #[test]
    fn agent_run_result_deserializes_structured_consultation_requests() {
        let raw = r#"{
            "status": "done",
            "summary": "Need a focused review.",
            "agentRequests": [
                {
                    "agentKey": "tech_lead",
                    "intent": "consult",
                    "request": "Check whether this transaction boundary is safe."
                }
            ]
        }"#;

        let result: AgentRunResult = serde_json::from_str(raw).expect("deserialize requests");
        match result {
            AgentRunResult::Done { agent_requests, .. } => {
                assert_eq!(agent_requests.len(), 1);
                assert_eq!(agent_requests[0].agent_key, "tech_lead");
                assert_eq!(agent_requests[0].intent, "consult");
                assert_eq!(
                    agent_requests[0].request,
                    "Check whether this transaction boundary is safe."
                );
            }
            _ => panic!("expected done variant"),
        }
    }

    #[cfg(feature = "mock-provider")]
    #[tokio::test]
    async fn mock_provider_returns_done_fixture_via_env_override() {
        let _lock = mock_env_lock();
        let prev = std::env::var("MOCK_AGENT_RESPONSE").ok();
        std::env::set_var("MOCK_AGENT_RESPONSE", "done");
        let provider = MockProvider::default();
        let result = provider.run(probe_input()).await.expect("mock run");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Mock implementation complete.");
            }
            _ => panic!("expected done variant"),
        }
        match prev {
            Some(v) => std::env::set_var("MOCK_AGENT_RESPONSE", v),
            None => std::env::remove_var("MOCK_AGENT_RESPONSE"),
        }
    }

    #[test]
    fn worktree_dir_from_context_absolutizes_existing_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let worktree = tmp.path().join("wt");
        let context = worktree.join(".agent").join("context.md");
        std::fs::create_dir_all(context.parent().unwrap()).expect("mkdir");
        std::fs::write(&context, "x").expect("write");

        let resolved = worktree_dir_from_context(context.to_str().unwrap()).expect("resolve");
        assert!(resolved.is_absolute());
        assert_eq!(resolved, worktree.canonicalize().unwrap());
    }

    #[test]
    fn worktree_dir_from_context_rejects_bad_path() {
        let err = worktree_dir_from_context("context.md").expect_err("need parents");
        assert!(matches!(err, ProviderError::InvalidInput(_)));
    }

    #[test]
    fn artifacts_path_absolutizes_relative_artifacts_dir() {
        let path = artifacts_path("./data/artifacts", &["runs", "run-1"]).expect("path");
        assert!(path.is_absolute(), "{path:?}");
        assert!(path.ends_with("data/artifacts/runs/run-1"), "{path:?}");

        let already = artifacts_path("/srv/artifacts", &["runs", "run-1"]).expect("path");
        assert_eq!(already, std::path::Path::new("/srv/artifacts/runs/run-1"));
    }

    #[test]
    fn run_dir_is_absolute_so_children_do_not_resolve_it_in_the_worktree() {
        let mut input = probe_input();
        input.artifacts_dir = Some("./data/artifacts".into());
        input.run_id = Some("run-1".into());

        let dir = run_dir(&input, "cursor").expect("run dir");
        assert!(dir.is_absolute(), "{dir:?}");
        assert!(dir.ends_with("data/artifacts/runs/run-1"), "{dir:?}");
    }

    #[test]
    fn run_dir_without_artifacts_dir_is_mcp_unavailable() {
        let input = probe_input();
        let err = run_dir(&input, "cursor").expect_err("no artifacts dir");
        assert!(
            err.to_string()
                .contains("mcp_unavailable: cursor has no run artifacts dir"),
            "{err}"
        );
    }

    #[test]
    fn absolute_existing_dir_rejects_missing() {
        let err = absolute_existing_dir(std::path::Path::new(
            "./data/worktrees/definitely-missing-coppice-wt",
        ))
        .expect_err("missing");
        assert!(err.to_string().contains("not an accessible directory"));
    }
}
