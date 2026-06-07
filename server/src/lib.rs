pub mod api;

use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    // extended in later tasks
}

pub async fn test_state() -> Arc<AppState> {
    Arc::new(AppState {})
}

pub fn app(state: Arc<AppState>) -> Router {
    api::router(state)
}
