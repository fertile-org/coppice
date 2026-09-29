pub mod claude_code;
pub mod claude_console;
pub mod codex;
pub mod codex_console;
pub mod codex_models;
pub mod cursor;
pub mod cursor_console;
pub mod cursor_models;
pub mod kilo_code;
pub mod kilo_console;
pub mod kilo_models;
pub mod mock;
pub mod opencode;
pub mod opencode_models;
pub mod registry;

pub use registry::ConnectorRegistry;

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

/// Allowlist pattern for the gateway's tools under connectors that namespace
/// MCP tools as `mcp__<server>__<tool>` (claude-code).
pub const COPPICE_MCP_TOOLS: &str = "mcp__coppice__*";

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
pub fn mcp_unavailable(reason: &str) -> ProviderError {
    ProviderError::InvalidInput(format!("mcp_unavailable: {reason}"))
}

/// Connectors whose adapters actually restrict tools when `read_only_tools` is set.
/// Others either refuse (kilo-code) or ignore the flag (codex, opencode).
pub const READ_ONLY_CAPABLE_CONNECTORS: &[&str] = &["mock", "claude-code", "cursor"];

pub fn connector_enforces_read_only(connector: &str) -> bool {
    READ_ONLY_CAPABLE_CONNECTORS.contains(&connector)
}

/// Fail closed when a connector cannot enforce read-only tools for chat turns.
pub fn refuse_unsupported_read_only(connector_id: &str) -> ProviderError {
    ProviderError::InvalidInput(format!(
        "connector `{connector_id}` cannot enforce read-only tools for conversation chat turns"
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

pub const CHAT_RESUME_CONNECTORS: &[&str] =
    &["mock", "opencode", "claude-code", "cursor", "codex"];

pub fn connector_supports_chat_resume(connector: &str) -> bool {
    CHAT_RESUME_CONNECTORS.contains(&connector)
}

pub fn is_resume_session_invalid(err: &ProviderError) -> bool {
    match err {
        ProviderError::ResumeSessionInvalid(_) => true,
        ProviderError::InvalidInput(msg)
        | ProviderError::InvalidFixture(msg)
        | ProviderError::MissingResult(msg) => {
            let m = msg.to_ascii_lowercase();
            ["session not found", "invalid resume", "unknown session"]
                .iter()
                .any(|needle| m.contains(needle))
        }
        _ => false,
    }
}

#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError>;
}

pub fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/agent-responses")
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
    fn resume_invalid_detection() {
        assert!(is_resume_session_invalid(&ProviderError::ResumeSessionInvalid("x".into())));
        assert!(is_resume_session_invalid(
            &ProviderError::InvalidInput("session not found".into())
        ));
        assert!(is_resume_session_invalid(&ProviderError::MissingResult(
            "Error: unknown session abc".into()
        )));
        assert!(!is_resume_session_invalid(&ProviderError::MissingResult(
            "no final result".into()
        )));
        assert!(!is_resume_session_invalid(&ProviderError::Cancelled));
    }

    #[test]
    fn chat_resume_connectors_include_all_vendor_chat_clis() {
        for id in ["mock", "opencode", "claude-code", "cursor", "codex"] {
            assert!(connector_supports_chat_resume(id));
        }
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
            err.to_string().contains("mcp_unavailable: cursor has no run artifacts dir"),
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
