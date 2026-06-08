pub mod auth;
pub mod events;
pub mod live;

use crate::AppState;
use axum::{routing::get, Router};
use std::sync::Arc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ws/agent-runs/{run_id}/live", get(live::live_ws_handler))
        .route("/ws/events", get(events::events_ws_handler))
}
