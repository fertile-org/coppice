pub mod api;
pub mod config;
pub mod db;
pub mod domain;
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
    pub agent_provider: Arc<dyn crate::providers::AgentProvider>,
}

impl AppState {
    pub fn attachment_store_from_config(config: &AppConfig) -> AttachmentStore {
        AttachmentStore::new(
            &config.storage.artifacts_dir,
            config.storage.max_upload_bytes,
        )
    }

    pub fn agent_provider_from_config(
        config: &AppConfig,
    ) -> Arc<dyn crate::providers::AgentProvider> {
        match config.agent.default_provider.as_str() {
            "mock" => Arc::new(crate::providers::mock::MockProvider::default()),
            other => panic!("unknown agent provider: {other}"),
        }
    }
}

pub async fn test_state() -> Arc<AppState> {
    let config = AppConfig::load_defaults().expect("test config");
    Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        agent_provider: AppState::agent_provider_from_config(&config),
        config,
        db: None,
    })
}

pub fn app(state: Arc<AppState>) -> Router {
    api::router(state)
}
