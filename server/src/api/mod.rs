pub mod auth;
mod health;

use axum::Router;
use std::sync::Arc;
use crate::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(health::routes())
        .merge(auth::public_routes())
        .merge(auth::protected_routes())
        .with_state(state)
}
