//! Plugin MCP servers as the gateway's third tool source.

use super::naming::exposed_name;
use super::pool::{McpServerPool, PoolError, PoolServerSpec};
use crate::crypto::SecretStore;
use crate::domain::context_profile::ContextProfile;
use crate::mcp::protocol::{ToolDefinition, ToolResult};
use crate::mcp::source::{SourceKind, SourcedTool, ToolSource};
use crate::mcp::token::RunToolScope;
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::plugins::capability::McpServerTransport;
use crate::services::plugin_service::PluginService;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait PluginServerCatalog: Send + Sync {
    /// Servers of the usable plugins among `plugin_ids`; empty on lookup failure.
    async fn servers_for(&self, plugin_ids: &[Uuid]) -> Vec<PoolServerSpec>;
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
}

pub struct PluginMcpSource {
    pool: Arc<McpServerPool>,
    catalog: Arc<dyn PluginServerCatalog>,
}

impl PluginMcpSource {
    pub fn new(pool: Arc<McpServerPool>, catalog: Arc<dyn PluginServerCatalog>) -> Self {
        Self { pool, catalog }
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

fn unavailable(plugin: &str, err: &PoolError) -> ToolResult {
    let text = match err {
        PoolError::Config(message) => format!("plugin \"{plugin}\" unavailable: {message}"),
        _ => format!("plugin \"{plugin}\" unavailable"),
    };
    ToolResult::text(text, true)
}

fn listable(spec: &PoolServerSpec) -> bool {
    !matches!(spec.entry.transport, McpServerTransport::Unsupported { .. })
}

#[async_trait]
impl ToolSource for PluginMcpSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Plugin
    }

    async fn list(&self, scope: &RunToolScope) -> Vec<SourcedTool> {
        if scope.profile == ContextProfile::KnowledgeCompaction || scope.plugin_ids.is_empty() {
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
                .map(|spec| async move { (spec, self.pool.tools(spec).await) }),
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
    }

    struct FailingTransport;

    #[async_trait]
    impl McpTransport for FailingTransport {
        fn kind(&self) -> &'static str {
            "stdio"
        }

        async fn connect(
            &self,
            _spec: &ResolvedTransport,
            _cwd: &Path,
        ) -> Result<Box<dyn McpConnection>, ProxyError> {
            Err(ProxyError::Start("boom".into()))
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
        transports.register(Arc::new(FailingTransport));
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
}
