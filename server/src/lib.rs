pub mod agent_templates;
pub mod api;
pub mod config;
pub mod db;
pub mod domain;
pub mod events;
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
    pub opencode_serve: Option<Arc<crate::sessions::opencode_serve::OpenCodeServeManager>>,
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
        opencode_serve: Option<Arc<crate::sessions::opencode_serve::OpenCodeServeManager>>,
    ) -> Arc<crate::providers::ConnectorRegistry> {
        Arc::new(crate::providers::ConnectorRegistry::from_config(
            config,
            opencode_serve,
        ))
    }

    pub fn default_connector_id(&self) -> &str {
        &self.config.agent.default_connector
    }
}

pub async fn test_state() -> Arc<AppState> {
    let config = AppConfig::load_defaults().expect("test config");
    Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        connector_registry: AppState::connector_registry_from_config(&config, None),
        agent_health: Arc::new(crate::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(crate::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(crate::events::bus::EventBus::new()),
        opencode_serve: None,
        config,
        db: None,
    })
}

pub fn app(state: Arc<AppState>) -> Router {
    api::router(state)
}
