pub mod api;
pub mod config;

use axum::Router;
use std::sync::Arc;

pub use config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
}

pub async fn test_state() -> Arc<AppState> {
    let config = AppConfig::load(None).expect("test config");
    Arc::new(AppState { config })
}

pub fn app(state: Arc<AppState>) -> Router {
    api::router(state)
}
