use crate::domain::context_profile::ContextProfile;
use crate::mcp::protocol::{ToolContent, ToolDefinition, ToolResult};
use crate::mcp::source::{
    CoreToolSource, SkillToolSource, SourceKind, SourcedTool, ToolSource, UnlistedClaim,
};
use crate::mcp::token::RunToolScope;
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::AppState;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use uuid::Uuid;

const TRUNCATION_SUFFIX: &str = "\n…[truncated: use limit/before to page]";
const ARGS_SUMMARY_CHARS: usize = 200;
/// The chat reply path; allowed on read-only profiles despite writing.
const REPLY_TOOL: &str = "result_submit";
pub const DEFAULT_LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// Why `find` returned no tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Miss {
    /// Listed, but not allowed for the scope's profile.
    Filtered,
    /// No source listed the name.
    Unlisted,
}

#[derive(Clone, Copy)]
enum CallStatus {
    Ok,
    Error,
    Denied,
    Timeout,
}

impl CallStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Denied => "denied",
            Self::Timeout => "timeout",
        }
    }
}

/// Routes MCP tool calls over an ordered list of sources: merges their tools,
/// applies the profile filter, enforces limits, and logs every call.
pub struct ToolRegistry {
    sources: Vec<Arc<dyn ToolSource>>,
    list_timeout: Duration,
    warned_duplicates: Mutex<HashSet<String>>,
}

impl ToolRegistry {
    pub fn new(sources: Vec<Arc<dyn ToolSource>>) -> Self {
        Self {
            sources,
            list_timeout: DEFAULT_LIST_TIMEOUT,
            warned_duplicates: Mutex::new(HashSet::new()),
        }
    }

    /// Bounds each source's `list`; a source that overruns contributes no tools.
    pub fn with_list_timeout(mut self, list_timeout: Duration) -> Self {
        self.list_timeout = list_timeout;
        self
    }

    /// Core, then skill tools.
    pub fn builtin() -> Self {
        Self::new(vec![Arc::new(CoreToolSource), Arc::new(SkillToolSource)])
    }

    pub async fn tools_for(&self, scope: &RunToolScope) -> Vec<SourcedTool> {
        self.resolve(scope)
            .await
            .into_iter()
            .map(|(_, tool)| tool)
            .collect()
    }

    async fn list_source(&self, source: &dyn ToolSource, scope: &RunToolScope) -> Vec<SourcedTool> {
        match tokio::time::timeout(self.list_timeout, source.list(scope)).await {
            Ok(tools) => tools,
            Err(_) => {
                tracing::warn!(
                    source = source.kind().as_str(),
                    "tool source list timed out"
                );
                Vec::new()
            }
        }
    }

    /// Merged tool set for `scope`, each paired with the index of its source.
    async fn resolve(&self, scope: &RunToolScope) -> Vec<(usize, SourcedTool)> {
        let listed = futures_util::future::join_all(
            self.sources
                .iter()
                .map(|source| self.list_source(source.as_ref(), scope)),
        )
        .await;
        let mut seen = HashSet::new();
        let mut merged = Vec::new();
        for (index, tools) in listed.into_iter().enumerate() {
            for tool in tools {
                if seen.insert(tool.def.name.clone()) {
                    merged.push((index, tool));
                } else {
                    self.warn_duplicate(&tool);
                }
            }
        }
        merged.retain(|(_, tool)| allowed_for(scope.profile, &tool.def));
        merged
    }

    /// Same answer as `resolve` for one name, but stops at the first source that
    /// has it, so core calls never wait on later (e.g. plugin) sources.
    async fn find(&self, scope: &RunToolScope, name: &str) -> Result<(usize, SourcedTool), Miss> {
        for (index, source) in self.sources.iter().enumerate() {
            let found = self
                .list_source(source.as_ref(), scope)
                .await
                .into_iter()
                .find(|tool| tool.def.name == name);
            if let Some(tool) = found {
                return if allowed_for(scope.profile, &tool.def) {
                    Ok((index, tool))
                } else {
                    Err(Miss::Filtered)
                };
            }
        }
        Err(Miss::Unlisted)
    }

    /// First source that claims an unlisted `name`, each bounded like `list`.
    async fn claim(&self, scope: &RunToolScope, name: &str) -> Option<(SourceKind, UnlistedClaim)> {
        for source in &self.sources {
            let claim = tokio::time::timeout(self.list_timeout, source.claim_unlisted(scope, name))
                .await
                .ok()
                .flatten();
            if let Some(claim) = claim {
                return Some((source.kind(), claim));
            }
        }
        None
    }

    fn warn_duplicate(&self, tool: &SourcedTool) {
        let mut warned = self
            .warned_duplicates
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if warned.insert(tool.def.name.clone()) {
            tracing::warn!(
                tool = %tool.def.name,
                source = tool.source.as_str(),
                "duplicate mcp tool name; keeping the first source's tool"
            );
        }
    }

    #[cfg(test)]
    fn warned_duplicates(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .warned_duplicates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect();
        names.sort();
        names
    }

    pub async fn call(
        &self,
        state: &AppState,
        scope: &RunToolScope,
        name: &str,
        args: Value,
    ) -> ToolResult {
        let started = Instant::now();
        let (index, tool) = match self.find(scope, name).await {
            Ok(found) => found,
            Err(miss) => {
                let claim = match miss {
                    Miss::Unlisted => self.claim(scope, name).await,
                    Miss::Filtered => None,
                };
                let (source, plugin_id, status, message) = match claim {
                    Some((source, claim)) => {
                        (source, claim.plugin_id, CallStatus::Error, claim.message)
                    }
                    None => (
                        SourceKind::Core,
                        None,
                        CallStatus::Denied,
                        format!("denied: tool \"{name}\" is not available for this run"),
                    ),
                };
                let entry = CallLog {
                    tool: name,
                    source,
                    plugin_id,
                    args: &args,
                    status,
                    error: Some(&message),
                    started,
                };
                log_call(state, scope, entry).await;
                return ToolResult::text(message, true);
            }
        };

        let (status, result, error) = self
            .run(
                state,
                scope,
                self.sources[index].as_ref(),
                &tool,
                args.clone(),
            )
            .await;
        let entry = CallLog {
            tool: name,
            source: tool.source,
            plugin_id: tool.plugin_id,
            args: &args,
            status,
            error: error.as_deref(),
            started,
        };
        log_call(state, scope, entry).await;
        result
    }

    async fn run(
        &self,
        state: &AppState,
        scope: &RunToolScope,
        source: &dyn ToolSource,
        tool: &SourcedTool,
        args: Value,
    ) -> (CallStatus, ToolResult, Option<String>) {
        let name = tool.def.name.as_str();
        let Some(pool) = state.db.as_ref() else {
            tracing::error!(tool = name, "mcp tool call without database");
            return error_output(CallStatus::Error, "internal error");
        };
        let ctx = ToolCtx { state, pool, scope };
        let timeout = Duration::from_secs(state.config.mcp.call_timeout_secs);
        match tokio::time::timeout(timeout, source.call(&ctx, tool, args)).await {
            Err(_) => error_output(CallStatus::Timeout, "timeout"),
            Ok(result) => {
                if let Err(ToolError::Internal(inner)) = &result {
                    tracing::error!(tool = name, run_id = %scope.run_id, error = ?inner, "mcp tool failed");
                }
                classify(result, state.config.mcp.max_output_bytes)
            }
        }
    }
}

fn allowed_for(profile: ContextProfile, def: &ToolDefinition) -> bool {
    match profile {
        ContextProfile::HumanChat | ContextProfile::Conversation => {
            def.read_only || def.name == REPLY_TOOL
        }
        ContextProfile::Full | ContextProfile::HumanAgent | ContextProfile::KnowledgeCompaction => {
            true
        }
    }
}

struct CallLog<'a> {
    tool: &'a str,
    source: SourceKind,
    plugin_id: Option<Uuid>,
    args: &'a Value,
    status: CallStatus,
    error: Option<&'a str>,
    started: Instant,
}

async fn log_call(state: &AppState, scope: &RunToolScope, entry: CallLog<'_>) {
    let Some(pool) = state.db.as_ref() else {
        return;
    };
    let duration_ms = i32::try_from(entry.started.elapsed().as_millis()).unwrap_or(i32::MAX);
    let result = sqlx::query(
        r#"
        INSERT INTO run_tool_calls (id, run_id, tool, source, plugin_id, args_summary, status, error, duration_ms)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(scope.run_id)
    .bind(entry.tool)
    .bind(entry.source.as_str())
    .bind(entry.plugin_id)
    .bind(summarize_args(entry.args))
    .bind(entry.status.as_str())
    .bind(entry.error)
    .bind(duration_ms)
    .execute(pool)
    .await;
    if let Err(e) = result {
        tracing::warn!(run_id = %scope.run_id, tool = entry.tool, error = %e, "failed to log mcp tool call");
    }
}

fn error_output(status: CallStatus, message: &str) -> (CallStatus, ToolResult, Option<String>) {
    (
        status,
        ToolResult::text(message, true),
        Some(message.to_string()),
    )
}

fn summarize_args(args: &Value) -> String {
    args.to_string().chars().take(ARGS_SUMMARY_CHARS).collect()
}

fn classify(
    result: Result<ToolResult, ToolError>,
    max_output_bytes: usize,
) -> (CallStatus, ToolResult, Option<String>) {
    match result {
        Ok(mut output) => {
            for block in &mut output.content {
                if let ToolContent::Text(text) = block {
                    *text = truncate_output(std::mem::take(text), max_output_bytes);
                }
            }
            if !output.is_error {
                return (CallStatus::Ok, output, None);
            }
            let error = output
                .content
                .iter()
                .find_map(|block| match block {
                    ToolContent::Text(text) => Some(text.clone()),
                    ToolContent::Image { .. } => None,
                })
                .unwrap_or_else(|| "tool error".to_string());
            (CallStatus::Error, output, Some(error))
        }
        Err(e) => {
            let status = match e {
                ToolError::Denied(_) => CallStatus::Denied,
                _ => CallStatus::Error,
            };
            error_output(status, &e.message())
        }
    }
}

/// The returned text, suffix included, never exceeds `max_bytes`.
fn truncate_output(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes.saturating_sub(TRUNCATION_SUFFIX.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = format!("{}{}", &text[..end], TRUNCATION_SUFFIX);
    if out.len() > max_bytes {
        let mut cut = max_bytes;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::context_profile::ContextProfile;
    use crate::mcp::protocol::{ToolContent, ToolDefinition, ToolResult};
    use crate::mcp::source::{SourceKind, SourcedTool, ToolSource, UnlistedClaim};
    use crate::mcp::token::RunToolScope;
    use crate::mcp::tools::{ToolCtx, ToolError};
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use uuid::Uuid;

    struct FakeSource {
        kind: SourceKind,
        tools: Vec<(&'static str, bool)>,
    }

    #[async_trait]
    impl ToolSource for FakeSource {
        fn kind(&self) -> SourceKind {
            self.kind
        }
        async fn list(&self, _scope: &RunToolScope) -> Vec<SourcedTool> {
            self.tools
                .iter()
                .map(|(name, read_only)| SourcedTool {
                    def: ToolDefinition {
                        name: (*name).to_string(),
                        description: format!("{name} from {}", self.kind.as_str()),
                        input_schema: json!({"type": "object"}),
                        read_only: *read_only,
                    },
                    source: self.kind,
                    plugin_id: None,
                    key: (*name).to_string(),
                })
                .collect()
        }
        async fn call(
            &self,
            _ctx: &ToolCtx<'_>,
            _tool: &SourcedTool,
            _args: Value,
        ) -> Result<ToolResult, ToolError> {
            Ok(ToolResult {
                content: vec![ToolContent::Text("ok".into())],
                is_error: false,
            })
        }
    }

    fn scope(profile: ContextProfile) -> RunToolScope {
        RunToolScope {
            token_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            ticket_id: None,
            chat_session_id: None,
            board_id: None,
            profile,
            job_type: "work_on_ticket".into(),
            compaction_ticket_ids: vec![],
            plugin_ids: vec![],
        }
    }

    fn fake(kind: SourceKind, tools: Vec<(&'static str, bool)>) -> Arc<dyn ToolSource> {
        Arc::new(FakeSource { kind, tools })
    }

    async fn names(registry: &ToolRegistry, profile: ContextProfile) -> Vec<String> {
        registry
            .tools_for(&scope(profile))
            .await
            .into_iter()
            .map(|t| t.def.name)
            .collect()
    }

    #[tokio::test]
    async fn merge_keeps_source_order_and_first_duplicate() {
        let registry = ToolRegistry::new(vec![
            fake(SourceKind::Core, vec![("a", true), ("x", true)]),
            fake(SourceKind::Plugin, vec![("x", true), ("b", true)]),
        ]);
        for _ in 0..2 {
            let tools = registry.tools_for(&scope(ContextProfile::Full)).await;
            let listed: Vec<&str> = tools.iter().map(|t| t.def.name.as_str()).collect();
            assert_eq!(listed, vec!["a", "x", "b"]);
            let x = tools.iter().find(|t| t.def.name == "x").unwrap();
            assert_eq!(x.source, SourceKind::Core);
            assert_eq!(x.def.description, "x from core");
        }
        assert_eq!(registry.warned_duplicates(), vec!["x".to_string()]);
    }

    #[tokio::test]
    async fn chat_profiles_list_unchanged() {
        let registry = ToolRegistry::builtin();
        let full: Vec<&str> = vec![
            "board_agents",
            "ticket_get",
            "ticket_comments",
            "ticket_runs",
            "ticket_search",
            "knowledge_search",
            "comment_post",
            "result_submit",
            "skill_list",
            "skill_load",
        ];
        let chat: Vec<&str> = full
            .iter()
            .copied()
            .filter(|n| *n != "comment_post")
            .collect();
        let compaction = vec![
            "ticket_get",
            "ticket_comments",
            "ticket_runs",
            "knowledge_search",
            "skill_list",
            "skill_load",
            "result_submit",
        ];
        for (profile, expected) in [
            (ContextProfile::Full, &full),
            (ContextProfile::HumanAgent, &full),
            (ContextProfile::HumanChat, &chat),
            (ContextProfile::Conversation, &chat),
            (ContextProfile::KnowledgeCompaction, &compaction),
        ] {
            let mut got = names(&registry, profile).await;
            got.sort();
            let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
            want.sort();
            assert_eq!(got, want, "{}", profile.as_str());
        }
    }

    #[tokio::test]
    async fn chat_filter_drops_non_read_only_but_keeps_result_submit() {
        let registry = ToolRegistry::new(vec![fake(
            SourceKind::Plugin,
            vec![
                ("writer", false),
                ("reader", true),
                ("result_submit", false),
            ],
        )]);
        assert_eq!(
            names(&registry, ContextProfile::Full).await,
            vec!["writer", "reader", "result_submit"]
        );
        for profile in [ContextProfile::HumanChat, ContextProfile::Conversation] {
            assert_eq!(
                names(&registry, profile).await,
                vec!["reader", "result_submit"]
            );
        }
    }

    struct HungSource;

    #[async_trait]
    impl ToolSource for HungSource {
        fn kind(&self) -> SourceKind {
            SourceKind::Plugin
        }
        async fn list(&self, _scope: &RunToolScope) -> Vec<SourcedTool> {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            Vec::new()
        }
        async fn call(
            &self,
            _ctx: &ToolCtx<'_>,
            _tool: &SourcedTool,
            _args: Value,
        ) -> Result<ToolResult, ToolError> {
            unreachable!("hung source is never resolved")
        }
    }

    #[tokio::test]
    async fn hung_source_list_times_out_core_tools_intact() {
        let registry = ToolRegistry::new(vec![Arc::new(CoreToolSource), Arc::new(HungSource)])
            .with_list_timeout(std::time::Duration::from_millis(100));
        let started = std::time::Instant::now();
        let got = names(&registry, ContextProfile::Full).await;
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        let want: Vec<String> = crate::mcp::catalog::core_tools_for(ContextProfile::Full)
            .into_iter()
            .map(|t| t.definition().name)
            .collect();
        assert!(!want.is_empty());
        assert_eq!(got, want);
        assert_eq!(DEFAULT_LIST_TIMEOUT, std::time::Duration::from_secs(10));
    }

    struct CountingSource {
        inner: Arc<dyn ToolSource>,
        lists: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl ToolSource for CountingSource {
        fn kind(&self) -> SourceKind {
            self.inner.kind()
        }
        async fn list(&self, scope: &RunToolScope) -> Vec<SourcedTool> {
            self.lists.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.list(scope).await
        }
        async fn call(
            &self,
            ctx: &ToolCtx<'_>,
            tool: &SourcedTool,
            args: Value,
        ) -> Result<ToolResult, ToolError> {
            self.inner.call(ctx, tool, args).await
        }
    }

    #[tokio::test]
    async fn call_lookup_stops_at_first_source_with_the_name() {
        let later = Arc::new(CountingSource {
            inner: fake(SourceKind::Plugin, vec![("x", true), ("writer", false)]),
            lists: std::sync::atomic::AtomicUsize::new(0),
        });
        let registry = ToolRegistry::new(vec![
            fake(SourceKind::Core, vec![("a", true), ("x", true)]),
            later.clone(),
        ]);
        let lists = || later.lists.load(std::sync::atomic::Ordering::SeqCst);
        let full = scope(ContextProfile::Full);

        let (index, tool) = registry.find(&full, "a").await.unwrap();
        assert_eq!((index, tool.source), (0, SourceKind::Core));
        assert_eq!(lists(), 0, "core lookup must not list later sources");

        let (index, tool) = registry.find(&full, "x").await.unwrap();
        assert_eq!((index, tool.source), (0, SourceKind::Core));
        assert_eq!(tool.def.description, "x from core");
        assert_eq!(lists(), 0);

        let (index, tool) = registry.find(&full, "writer").await.unwrap();
        assert_eq!((index, tool.source), (1, SourceKind::Plugin));
        assert_eq!(lists(), 1);

        assert_eq!(
            registry
                .find(&scope(ContextProfile::HumanChat), "writer")
                .await
                .err(),
            Some(Miss::Filtered)
        );
        assert_eq!(
            registry.find(&full, "missing").await.err(),
            Some(Miss::Unlisted)
        );
    }

    struct ClaimingSource;

    #[async_trait]
    impl ToolSource for ClaimingSource {
        fn kind(&self) -> SourceKind {
            SourceKind::Plugin
        }
        async fn list(&self, _scope: &RunToolScope) -> Vec<SourcedTool> {
            Vec::new()
        }
        async fn call(
            &self,
            _ctx: &ToolCtx<'_>,
            _tool: &SourcedTool,
            _args: Value,
        ) -> Result<ToolResult, ToolError> {
            unreachable!("claiming source lists nothing")
        }
        async fn claim_unlisted(&self, _scope: &RunToolScope, name: &str) -> Option<UnlistedClaim> {
            name.starts_with("p__").then(|| UnlistedClaim {
                plugin_id: Some(Uuid::from_u128(5)),
                message: "plugin \"p\" unavailable".into(),
            })
        }
    }

    #[tokio::test]
    async fn unlisted_names_are_claimed_by_their_source() {
        let registry = ToolRegistry::new(vec![
            Arc::new(CoreToolSource),
            Arc::new(SkillToolSource),
            Arc::new(ClaimingSource),
        ]);
        let full = scope(ContextProfile::Full);
        assert_eq!(
            registry.claim(&full, "p__tool").await,
            Some((
                SourceKind::Plugin,
                UnlistedClaim {
                    plugin_id: Some(Uuid::from_u128(5)),
                    message: "plugin \"p\" unavailable".into(),
                }
            ))
        );
        assert_eq!(registry.claim(&full, "nope").await, None);
        assert_eq!(ToolRegistry::builtin().claim(&full, "p__tool").await, None);
    }

    #[test]
    fn truncate_output_stays_within_limit_and_char_boundaries() {
        assert_eq!(truncate_output("short".into(), 10), "short");

        let max = TRUNCATION_SUFFIX.len() + 10;
        let out = truncate_output("é".repeat(100), max);
        assert!(out.len() <= max, "len {} > {max}", out.len());
        assert!(out.ends_with(TRUNCATION_SUFFIX));
        assert!(out.starts_with("ééééé"));

        let ascii = truncate_output("x".repeat(500), max);
        assert_eq!(ascii.len(), max);

        let tiny = truncate_output("x".repeat(500), 5);
        assert!(tiny.len() <= 5);
    }

    fn only_text(result: &ToolResult) -> &str {
        match result.content.as_slice() {
            [ToolContent::Text(text)] => text,
            other => panic!("expected one text block, got {other:?}"),
        }
    }

    #[test]
    fn handler_denied_is_logged_as_denied() {
        let (status, output, error) = classify(Err(ToolError::Denied("nope".into())), 1000);
        assert_eq!(status.as_str(), "denied");
        assert!(output.is_error);
        assert_eq!(only_text(&output), "nope");
        assert_eq!(error.as_deref(), Some("nope"));
    }

    #[test]
    fn handler_errors_map_to_error_status() {
        for e in [
            ToolError::InvalidArgs("a".into()),
            ToolError::NotFound("b".into()),
            ToolError::Limit("c".into()),
            ToolError::Internal(anyhow::anyhow!("boom")),
        ] {
            let (status, output, _) = classify(Err(e), 1000);
            assert_eq!(status.as_str(), "error");
            assert!(output.is_error);
        }
        let (status, output, _) = classify(Err(ToolError::Internal(anyhow::anyhow!("boom"))), 1000);
        assert_eq!(status.as_str(), "error");
        assert_eq!(only_text(&output), "internal error");
    }

    #[test]
    fn ok_text_blocks_are_truncated() {
        let max = TRUNCATION_SUFFIX.len() + 10;
        let result = ToolResult {
            content: vec![
                ToolContent::Text("x".repeat(500)),
                ToolContent::Image {
                    data: "AAAA".into(),
                    mime_type: "image/png".into(),
                },
            ],
            is_error: false,
        };
        let (status, output, error) = classify(Ok(result), max);
        assert_eq!(status.as_str(), "ok");
        assert!(error.is_none());
        match &output.content[0] {
            ToolContent::Text(text) => assert!(text.ends_with(TRUNCATION_SUFFIX)),
            other => panic!("{other:?}"),
        }
        assert!(matches!(&output.content[1], ToolContent::Image { data, .. } if data == "AAAA"));
    }

    #[test]
    fn summarize_args_is_capped() {
        let args = serde_json::json!({ "body": "x".repeat(500) });
        assert_eq!(summarize_args(&args).chars().count(), ARGS_SUMMARY_CHARS);
    }
}
