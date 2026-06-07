pub mod api;
pub mod config;
pub mod db;
pub mod domain;
pub mod middleware;
pub mod providers;
pub mod services;

use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;

pub use config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub db: Option<PgPool>,
}

pub async fn test_state() -> Arc<AppState> {
    let config = AppConfig::load(None).expect("test config");
    Arc::new(AppState { config, db: None })
}

pub fn app(state: Arc<AppState>) -> Router {
    api::router(state)
}
