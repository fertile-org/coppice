use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "coppice-server" }))
}
