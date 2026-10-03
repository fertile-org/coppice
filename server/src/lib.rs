pub mod agent_templates;
pub mod api;
pub mod config;
pub mod crypto;
pub mod db;
pub mod domain;
pub mod events;
pub mod knowledge;
pub mod mcp;
pub mod plugins;
pub mod middleware;
pub mod providers;
pub mod sessions;
pub mod sandbox;
pub mod services;
pub mod storage;
pub mod util;
pub mod workers;

use axum::Router;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use storage::AttachmentStore;

pub use config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub db: Option<PgPool>,
    pub attachments: AttachmentStore,
    pub connector_registry: Arc<crate::providers::ConnectorRegistry>,
    pub agent_health: Arc<crate::services::agent_health::AgentHealthRegistry>,
    pub run_streams: Arc<crate::sessions::run_registry::RunStreamRegistry>,
    pub event_bus: Arc<crate::events::bus::EventBus>,
    pub opencode_runs: Arc<crate::sessions::opencode_run_server::OpenCodeRunServers>,
    pub agent_templates: HashMap<String, String>,
    pub secret_store: crate::crypto::SecretStore,
    pub skills: Arc<crate::plugins::skills::SkillCatalog>,
    pub tools: Arc<crate::mcp::registry::ToolRegistry>,
    pub plugin_mcp: Arc<crate::mcp::proxy::McpServerPool>,
}

impl AppState {
    pub fn attachment_store_from_config(config: &AppConfig) -> AttachmentStore {
        AttachmentStore::new(
            &config.storage.artifacts_dir,
            config.storage.max_upload_bytes,
        )
    }

    pub fn connector_registry_from_config(
        config: &AppConfig,
        opencode_runs: Arc<crate::sessions::opencode_run_server::OpenCodeRunServers>,
    ) -> Arc<crate::providers::ConnectorRegistry> {
        Arc::new(crate::providers::ConnectorRegistry::from_config(
            config,
            opencode_runs,
        ))
    }

    pub fn test_opencode_runs() -> Arc<crate::sessions::opencode_run_server::OpenCodeRunServers> {
        crate::sessions::opencode_run_server::OpenCodeRunServers::new(
            crate::providers::descriptor(coppice_connectors::OPENCODE)
                .expect("opencode descriptor")
                .binary
                .into(),
            "127.0.0.1".into(),
        )
    }

    pub fn default_connector_id(&self) -> &str {
        &self.config.agent.default_connector
    }

    /// Writes the embedded built-in plugins into `config.mcp.builtin_plugins_dir`
    /// and loads the skill catalog from there. Called once at startup.
    pub fn builtin_skills_from_config(
        config: &AppConfig,
    ) -> anyhow::Result<Arc<crate::plugins::skills::SkillCatalog>> {
        let dir = std::path::Path::new(&config.mcp.builtin_plugins_dir);
        crate::plugins::builtin::materialize_builtin(dir).map_err(|e| {
            anyhow::anyhow!(
                "failed to write built-in plugins to {}: {e}",
                dir.display()
            )
        })?;
        Ok(Arc::new(crate::plugins::skills::load_builtin(dir)?))
    }

    /// Records the default plugin dir (fatal on failure), scans all plugin
    /// dirs and loads enabled plugin skills; scan failures are logged so the
    /// server still starts.
    pub async fn init_plugins(
        db: &PgPool,
        config: &AppConfig,
        skills: &crate::plugins::skills::SkillCatalog,
    ) -> anyhow::Result<()> {
        let service = crate::services::plugin_service::PluginService::new(db);
        service
            .ensure_default_dir(&config.plugins.dir)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to set up default plugin dir {}: {e}",
                    config.plugins.dir
                )
            })?;
        if let Err(err) = service.rescan().await {
            tracing::warn!(error = %err, "plugin scan failed");
        }
        if let Err(err) = service.refresh_catalog(skills).await {
            tracing::warn!(error = %err, "loading plugin skills failed");
        }
        Ok(())
    }

    /// Built-in skills materialized once into a process-lifetime temp dir, so
    /// tests never write into the repo or a configured plugins dir.
    pub fn test_skills() -> Arc<crate::plugins::skills::SkillCatalog> {
        static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        let dir = DIR.get_or_init(|| {
            let dir = tempfile::tempdir().expect("builtin plugins tempdir");
            crate::plugins::builtin::materialize_builtin(dir.path()).expect("materialize builtins");
            dir
        });
        Arc::new(crate::plugins::skills::load_builtin(dir.path()).expect("load builtins"))
    }

    pub fn plugin_mcp_from_config(config: &AppConfig) -> Arc<crate::mcp::proxy::McpServerPool> {
        Arc::new(crate::mcp::proxy::McpServerPool::new(
            crate::mcp::proxy::Transports::builtin(),
            crate::mcp::proxy::PoolConfig::from_plugins(&config.plugins),
        ))
    }

    pub fn list_timeout_from_config(config: &AppConfig) -> std::time::Duration {
        std::time::Duration::from_secs(config.plugins.mcp_list_timeout_secs)
    }

    /// MCP gateway router over the core, skill, and (with a database) plugin MCP tool sources.
    pub fn build_tool_registry(
        db: Option<&PgPool>,
        secret_store: &crate::crypto::SecretStore,
        plugin_mcp: Arc<crate::mcp::proxy::McpServerPool>,
        list_timeout: std::time::Duration,
    ) -> Arc<crate::mcp::registry::ToolRegistry> {
        use crate::mcp::source::{CoreToolSource, SkillToolSource, ToolSource};
        let mut sources: Vec<Arc<dyn ToolSource>> =
            vec![Arc::new(CoreToolSource), Arc::new(SkillToolSource)];
        if let Some(db) = db {
            let catalog =
                crate::mcp::proxy::DbPluginServerCatalog::new(db.clone(), secret_store.clone());
            sources.push(Arc::new(crate::mcp::proxy::PluginMcpSource::new(
                plugin_mcp,
                Arc::new(catalog),
                list_timeout * 4 / 5,
            )));
        }
        Arc::new(crate::mcp::registry::ToolRegistry::new(sources).with_list_timeout(list_timeout))
    }

    pub fn load_agent_templates() -> HashMap<String, String> {
        let dir = crate::agent_templates::templates_dir();
        crate::agent_templates::load(&dir).expect("failed to load agent_templates from disk")
    }
}

pub async fn test_state() -> Arc<AppState> {
    let config = AppConfig::load_defaults().expect("test config");
    let secret_store = crate::crypto::SecretStore::from_master_key(&config.secrets.master_key);
    let opencode_runs = AppState::test_opencode_runs();
    let plugin_mcp = AppState::plugin_mcp_from_config(&config);
    let tools = AppState::build_tool_registry(
        None,
        &secret_store,
        plugin_mcp.clone(),
        AppState::list_timeout_from_config(&config),
    );
    Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        connector_registry: AppState::connector_registry_from_config(&config, opencode_runs.clone()),
        agent_health: Arc::new(crate::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(crate::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(crate::events::bus::EventBus::new()),
        opencode_runs,
        agent_templates: AppState::load_agent_templates(),
        secret_store,
        skills: AppState::test_skills(),
        tools,
        plugin_mcp,
        config,
        db: None,
    })
}

pub fn app(state: Arc<AppState>) -> Router {
    api::router(state)
}
