//! Plugin MCP servers as the gateway's third tool source.

use super::naming::{exposed_name, is_exposed_by};
use super::pool::{McpServerPool, PoolError, PoolServerSpec, ServerKey};
use super::transport::RemoteTool;
use crate::crypto::SecretStore;
use crate::domain::context_profile::ContextProfile;
use crate::mcp::protocol::{ToolDefinition, ToolResult};
use crate::mcp::source::{SourceKind, SourcedTool, ToolSource, UnlistedClaim};
use crate::mcp::token::RunToolScope;
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::plugins::capability::McpServerTransport;
use crate::services::plugin_service::PluginService;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginServerNames {
    pub plugin_id: Uuid,
    pub plugin_name: String,
    pub servers: Vec<String>,
}

#[async_trait]
pub trait PluginServerCatalog: Send + Sync {
    /// Servers of the usable plugins among `plugin_ids`; empty on lookup failure.
    async fn servers_for(&self, plugin_ids: &[Uuid]) -> Vec<PoolServerSpec>;
    /// Same plugins as `servers_for`, names only (no settings decrypted).
    async fn server_names_for(&self, plugin_ids: &[Uuid]) -> Vec<PluginServerNames>;
}

/// Enabled `ok` plugins from the database, settings decrypted.
pub struct DbPluginServerCatalog {
    pool: PgPool,
    store: SecretStore,
}

impl DbPluginServerCatalog {
    pub fn new(pool: PgPool, store: SecretStore) -> Self {
        Self { pool, store }
    }
}

#[async_trait]
impl PluginServerCatalog for DbPluginServerCatalog {
    async fn servers_for(&self, plugin_ids: &[Uuid]) -> Vec<PoolServerSpec> {
        PluginService::new(&self.pool)
            .server_specs_for(plugin_ids, &self.store, true)
            .await
            .unwrap_or_else(|err| {
                tracing::warn!(error = %err, "failed to load plugin MCP servers");
                Vec::new()
            })
    }

    async fn server_names_for(&self, plugin_ids: &[Uuid]) -> Vec<PluginServerNames> {
        PluginService::new(&self.pool)
            .mcp_server_names_for(plugin_ids)
            .await
            .unwrap_or_else(|err| {
                tracing::warn!(error = %err, "failed to load plugin MCP server names");
                Vec::new()
            })
    }
}

pub struct PluginMcpSource {
    pool: Arc<McpServerPool>,
    catalog: Arc<dyn PluginServerCatalog>,
    server_timeout: Duration,
}

impl PluginMcpSource {
    /// `server_timeout` bounds each server's tool listing; keep it below the
    /// registry list timeout so one slow server cannot drop the whole source.
    pub fn new(
        pool: Arc<McpServerPool>,
        catalog: Arc<dyn PluginServerCatalog>,
        server_timeout: Duration,
    ) -> Self {
        Self {
            pool,
            catalog,
            server_timeout,
        }
    }

    /// Concurrent listers share an in-flight start, each waiting at most the budget.
    async fn server_tools(&self, spec: &PoolServerSpec) -> Result<Vec<RemoteTool>, PoolError> {
        tokio::time::timeout(self.server_timeout, self.pool.tools(spec))
            .await
            .unwrap_or(Err(PoolError::Timeout))
    }

    async fn invoke(&self, tool: &SourcedTool, args: Value) -> Result<ToolResult, ToolError> {
        let not_available = || {
            ToolError::Denied(format!(
                "denied: tool \"{}\" is not available for this run",
                tool.def.name
            ))
        };
        let plugin_id = tool.plugin_id.ok_or_else(not_available)?;
        let (server, remote) = tool.key.split_once('/').ok_or_else(not_available)?;
        let spec = self
            .catalog
            .servers_for(&[plugin_id])
            .await
            .into_iter()
            .find(|spec| spec.key.server == server)
            .ok_or_else(not_available)?;
        match self.pool.call(&spec, remote, args).await {
            Ok(result) => Ok(result),
            Err(err) => Ok(unavailable(&spec.plugin_name, &err)),
        }
    }
}

/// Only placeholder and transport errors carry a message worth showing the agent.
fn actionable(err: &PoolError) -> Option<&str> {
    match err {
        PoolError::Config(message) => Some(message),
        PoolError::Unhealthy(message)
            if message.starts_with("missing setting")
                || message.starts_with("unsupported transport") =>
        {
            Some(message)
        }
        _ => None,
    }
}

fn unavailable_message(plugin: &str, detail: Option<&str>) -> String {
    match detail {
        Some(message) => format!("plugin \"{plugin}\" unavailable: {message}"),
        None => format!("plugin \"{plugin}\" unavailable"),
    }
}

fn unavailable(plugin: &str, err: &PoolError) -> ToolResult {
    ToolResult::text(unavailable_message(plugin, actionable(err)), true)
}

fn listable(spec: &PoolServerSpec) -> bool {
    !matches!(spec.entry.transport, McpServerTransport::Unsupported { .. })
}

fn exposes_plugins(scope: &RunToolScope) -> bool {
    !matches!(
        scope.profile,
        ContextProfile::KnowledgeCompaction | ContextProfile::ConnectorCheck
    ) && !scope.plugin_ids.is_empty()
}

#[async_trait]
impl ToolSource for PluginMcpSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Plugin
    }

    async fn list(&self, scope: &RunToolScope) -> Vec<SourcedTool> {
        if !exposes_plugins(scope) {
            return Vec::new();
        }
        let specs: Vec<PoolServerSpec> = self
            .catalog
            .servers_for(&scope.plugin_ids)
            .await
            .into_iter()
            .filter(listable)
            .collect();
        let listed = futures_util::future::join_all(
            specs
                .iter()
                .map(|spec| async move { (spec, self.server_tools(spec).await) }),
        )
        .await;
        let mut tools = Vec::new();
        for (spec, result) in listed {
            match result {
                Ok(remote) => tools.extend(remote.into_iter().map(|remote| SourcedTool {
                    def: ToolDefinition {
                        name: exposed_name(&spec.plugin_name, &remote.name),
                        description: remote.description,
                        input_schema: remote.input_schema,
                        read_only: remote.read_only,
                    },
                    source: SourceKind::Plugin,
                    plugin_id: Some(spec.key.plugin_id),
                    key: format!("{}/{}", spec.key.server, remote.name),
                })),
                Err(err) => tracing::debug!(
                    plugin = %spec.plugin_name,
                    server = %spec.key.server,
                    error = %err,
                    "plugin MCP server skipped"
                ),
            }
        }
        tools
    }

    async fn call(
        &self,
        _ctx: &ToolCtx<'_>,
        tool: &SourcedTool,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        self.invoke(tool, args).await
    }

    /// Claims `<plugin>__*` names of the token's usable plugins; their servers
    /// failed to list, so the call reports the plugin as unavailable.
    async fn claim_unlisted(&self, scope: &RunToolScope, name: &str) -> Option<UnlistedClaim> {
        if !exposes_plugins(scope) {
            return None;
        }
        let owner = self
            .catalog
            .server_names_for(&scope.plugin_ids)
            .await
            .into_iter()
            .filter(|plugin| is_exposed_by(&plugin.plugin_name, name))
            .max_by_key(|plugin| plugin.plugin_name.len())?;
        let errors: Vec<PoolError> = owner
            .servers
            .iter()
            .filter_map(|server| {
                self.pool.last_error(&ServerKey {
                    plugin_id: owner.plugin_id,
                    server: server.clone(),
                })
            })
            .map(PoolError::Unhealthy)
            .collect();
        let detail = errors.iter().find_map(actionable);
        Some(UnlistedClaim {
            plugin_id: Some(owner.plugin_id),
            message: unavailable_message(&owner.plugin_name, detail),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::ToolContent;
    use crate::mcp::proxy::{
        McpConnection, McpTransport, PoolConfig, ProxyError, ServerKey, Transports,
    };
    use crate::plugins::capability::McpServerEntry;
    use crate::plugins::placeholders::ResolvedTransport;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct FakeCatalog {
        specs: Vec<PoolServerSpec>,
        queries: AtomicUsize,
    }

    #[async_trait]
    impl PluginServerCatalog for FakeCatalog {
        async fn servers_for(&self, plugin_ids: &[Uuid]) -> Vec<PoolServerSpec> {
            self.queries.fetch_add(1, Ordering::SeqCst);
            self.specs
                .iter()
                .filter(|spec| plugin_ids.contains(&spec.key.plugin_id))
                .cloned()
                .collect()
        }

        async fn server_names_for(&self, plugin_ids: &[Uuid]) -> Vec<PluginServerNames> {
            let mut names: Vec<PluginServerNames> = Vec::new();
            for spec in self
                .specs
                .iter()
                .filter(|spec| plugin_ids.contains(&spec.key.plugin_id))
            {
                match names.iter_mut().find(|n| n.plugin_id == spec.key.plugin_id) {
                    Some(entry) => entry.servers.push(spec.key.server.clone()),
                    None => names.push(PluginServerNames {
                        plugin_id: spec.key.plugin_id,
                        plugin_name: spec.plugin_name.clone(),
                        servers: vec![spec.key.server.clone()],
                    }),
                }
            }
            names
        }
    }

    /// Command `slow` hangs in `connect`, `starting` serves one `echo` tool after
    /// 100 ms, `fast` serves it at once, anything else fails.
    struct ScriptedTransport;

    struct EchoConn;

    #[async_trait]
    impl McpConnection for EchoConn {
        async fn list_tools(&self) -> Result<Vec<RemoteTool>, ProxyError> {
            Ok(vec![RemoteTool {
                name: "echo".into(),
                description: "Echo".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                read_only: true,
            }])
        }

        async fn call_tool(&self, name: &str, _args: Value) -> Result<ToolResult, ProxyError> {
            Ok(ToolResult::text(name, false))
        }

        fn take_tools_changed(&self) -> bool {
            false
        }

        fn is_closed(&self) -> bool {
            false
        }

        async fn close(&self) {}
    }

    #[async_trait]
    impl McpTransport for ScriptedTransport {
        fn kind(&self) -> &'static str {
            "stdio"
        }

        async fn connect(
            &self,
            spec: &ResolvedTransport,
            _cwd: &Path,
        ) -> Result<Box<dyn McpConnection>, ProxyError> {
            match spec {
                ResolvedTransport::Stdio { command, .. } if command == "fast" => {
                    Ok(Box::new(EchoConn))
                }
                ResolvedTransport::Stdio { command, .. } if command == "starting" => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok(Box::new(EchoConn))
                }
                ResolvedTransport::Stdio { command, .. } if command == "slow" => {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    Err(ProxyError::Start("slow".into()))
                }
                _ => Err(ProxyError::Start("boom".into())),
            }
        }
    }

    const PLUGIN: Uuid = Uuid::from_u128(7);

    fn spec(env: BTreeMap<String, String>) -> PoolServerSpec {
        PoolServerSpec {
            key: ServerKey {
                plugin_id: PLUGIN,
                server: "fake".into(),
            },
            plugin_name: "mcp-fake".into(),
            plugin_root: PathBuf::from("/plugins/mcp-fake"),
            entry: McpServerEntry {
                name: "fake".into(),
                transport: McpServerTransport::Stdio {
                    command: "fake-mcp".into(),
                    args: Vec::new(),
                    env,
                },
                error: None,
            },
            settings: BTreeMap::new(),
        }
    }

    fn make_source(specs: Vec<PoolServerSpec>) -> (PluginMcpSource, Arc<FakeCatalog>) {
        let mut transports = Transports::default();
        transports.register(Arc::new(ScriptedTransport));
        let cfg = PoolConfig {
            start_timeout: Duration::from_secs(5),
            idle_shutdown: Duration::from_secs(600),
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            unhealthy_after: 3,
        };
        let catalog = Arc::new(FakeCatalog {
            specs,
            queries: AtomicUsize::new(0),
        });
        let source = PluginMcpSource::new(
            Arc::new(McpServerPool::new(transports, cfg)),
            catalog.clone(),
            Duration::from_millis(200),
        );
        (source, catalog)
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
            job_type: "knowledge_compaction".into(),
            compaction_ticket_ids: vec![],
            plugin_ids: vec![PLUGIN],
        }
    }

    fn echo_tool() -> SourcedTool {
        SourcedTool {
            def: ToolDefinition {
                name: "mcp-fake__echo".into(),
                description: String::new(),
                input_schema: serde_json::json!({ "type": "object" }),
                read_only: true,
            },
            source: SourceKind::Plugin,
            plugin_id: Some(PLUGIN),
            key: "fake/echo".into(),
        }
    }

    #[tokio::test]
    async fn compaction_lists_no_plugin_tools() {
        let (source, catalog) = make_source(vec![spec(BTreeMap::new())]);
        assert!(source
            .list(&scope(ContextProfile::KnowledgeCompaction))
            .await
            .is_empty());
        assert_eq!(catalog.queries.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn unavailable_call_returns_tool_error() {
        let (source, _) = make_source(vec![spec(BTreeMap::new())]);
        let result = source.invoke(&echo_tool(), Value::Null).await.unwrap();
        assert!(result.is_error);
        assert_eq!(
            result.content,
            vec![ToolContent::Text("plugin \"mcp-fake\" unavailable".into())]
        );

        let env = [("TOKEN".to_string(), "${X}".to_string())].into();
        let (source, _) = make_source(vec![spec(env)]);
        let result = source.invoke(&echo_tool(), Value::Null).await.unwrap();
        assert!(result.is_error);
        assert_eq!(
            result.content,
            vec![ToolContent::Text(
                "plugin \"mcp-fake\" unavailable: missing setting \"X\"".into()
            )]
        );
    }

    #[tokio::test]
    async fn failing_server_is_skipped_when_listing() {
        let (source, catalog) = make_source(vec![spec(BTreeMap::new())]);
        assert!(source.list(&scope(ContextProfile::Full)).await.is_empty());
        assert_eq!(catalog.queries.load(Ordering::SeqCst), 1);
    }

    fn server(plugin_id: Uuid, name: &str, command: &str) -> PoolServerSpec {
        let mut spec = spec(BTreeMap::new());
        spec.key.plugin_id = plugin_id;
        spec.plugin_name = name.into();
        spec.entry.transport = McpServerTransport::Stdio {
            command: command.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
        };
        spec
    }

    #[tokio::test]
    async fn slow_server_skipped_after_bounded_wait() {
        let fast = Uuid::from_u128(1);
        let slow = Uuid::from_u128(2);
        let (source, _) = make_source(vec![
            server(fast, "fast", "fast"),
            server(slow, "slow", "slow"),
        ]);
        let mut scope = scope(ContextProfile::Full);
        scope.plugin_ids = vec![fast, slow];

        let started = std::time::Instant::now();
        let names: Vec<String> = source
            .list(&scope)
            .await
            .into_iter()
            .map(|t| t.def.name)
            .collect();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(names, vec!["fast__echo".to_string()]);

        let started = std::time::Instant::now();
        let names: Vec<String> = source
            .list(&scope)
            .await
            .into_iter()
            .map(|t| t.def.name)
            .collect();
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(150) && waited < Duration::from_secs(1),
            "still-starting server should be awaited up to the budget: {waited:?}"
        );
        assert_eq!(names, vec!["fast__echo".to_string()]);
    }

    #[tokio::test]
    async fn concurrent_listers_share_a_start_that_finishes_within_budget() {
        let plugin = Uuid::from_u128(3);
        let (source, _) = make_source(vec![server(plugin, "late", "starting")]);
        let mut scope = scope(ContextProfile::Full);
        scope.plugin_ids = vec![plugin];
        let names = |tools: Vec<SourcedTool>| -> Vec<String> {
            tools.into_iter().map(|t| t.def.name).collect()
        };
        let (first, second) = tokio::join!(source.list(&scope), source.list(&scope));
        assert_eq!(names(first), vec!["late__echo".to_string()]);
        assert_eq!(names(second), vec!["late__echo".to_string()]);
    }

    #[test]
    fn unhealthy_placeholder_errors_stay_actionable() {
        let text = |err: PoolError| match unavailable("p", &err).content.as_slice() {
            [ToolContent::Text(text)] => text.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            text(PoolError::Unhealthy("missing setting \"X\"".into())),
            "plugin \"p\" unavailable: missing setting \"X\""
        );
        assert_eq!(
            text(PoolError::Unhealthy("unsupported transport \"sse\"".into())),
            "plugin \"p\" unavailable: unsupported transport \"sse\""
        );
        assert_eq!(
            text(PoolError::Unhealthy(
                "MCP server failed to start: boom".into()
            )),
            "plugin \"p\" unavailable"
        );
        assert_eq!(text(PoolError::Timeout), "plugin \"p\" unavailable");
    }

    #[tokio::test]
    async fn claims_unlisted_tools_of_snapshot_plugins_only() {
        let (source, _) = make_source(vec![spec(BTreeMap::new())]);
        let full = scope(ContextProfile::Full);
        assert!(source.list(&full).await.is_empty());
        assert_eq!(
            source.claim_unlisted(&full, "mcp-fake__echo").await,
            Some(UnlistedClaim {
                plugin_id: Some(PLUGIN),
                message: "plugin \"mcp-fake\" unavailable".into(),
            })
        );
        assert_eq!(source.claim_unlisted(&full, "ticket_get").await, None);
        assert_eq!(source.claim_unlisted(&full, "other__echo").await, None);

        let mut other = scope(ContextProfile::Full);
        other.plugin_ids = vec![Uuid::from_u128(99)];
        assert_eq!(source.claim_unlisted(&other, "mcp-fake__echo").await, None);
        let compaction = scope(ContextProfile::KnowledgeCompaction);
        assert_eq!(
            source.claim_unlisted(&compaction, "mcp-fake__echo").await,
            None
        );
    }

    #[tokio::test]
    async fn claim_keeps_missing_setting_detail() {
        let env = [("TOKEN".to_string(), "${X}".to_string())].into();
        let (source, _) = make_source(vec![spec(env)]);
        let full = scope(ContextProfile::Full);
        assert!(source.list(&full).await.is_empty());
        let claim = source
            .claim_unlisted(&full, "mcp-fake__echo")
            .await
            .unwrap();
        assert_eq!(
            claim.message,
            "plugin \"mcp-fake\" unavailable: missing setting \"X\""
        );
    }

    #[tokio::test]
    async fn unhealthy_after_config_error_keeps_setting_message() {
        let env = [("TOKEN".to_string(), "${X}".to_string())].into();
        let (source, _) = make_source(vec![spec(env)]);
        for _ in 0..2 {
            let result = source.invoke(&echo_tool(), Value::Null).await.unwrap();
            assert_eq!(
                result.content,
                vec![ToolContent::Text(
                    "plugin \"mcp-fake\" unavailable: missing setting \"X\"".into()
                )]
            );
        }
    }
}
