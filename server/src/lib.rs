pub mod api;
pub mod config;
pub mod db;
pub mod domain;
pub mod middleware;
pub mod providers;
pub mod services;
pub mod storage;

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
}

impl AppState {
    pub fn attachment_store_from_config(config: &AppConfig) -> AttachmentStore {
        AttachmentStore::new(
            &config.storage.artifacts_dir,
            config.storage.max_upload_bytes,
        )
    }
}

pub async fn test_state() -> Arc<AppState> {
    let config = AppConfig::load(None).expect("test config");
    Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        config,
        db: None,
    })
}

pub fn app(state: Arc<AppState>) -> Router {
    api::router(state)
}
