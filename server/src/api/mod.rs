pub mod auth;
mod agents;
mod attachments;
mod comments;
mod health;
mod projects;
mod repos;
mod tickets;

use axum::{middleware, Router};
use std::sync::Arc;
use crate::middleware::{csrf, session};
use crate::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    let public = Router::new()
        .merge(health::routes())
        .merge(auth::public_routes());

    let protected = auth::protected_routes()
        .merge(projects::routes())
        .merge(repos::routes())
        .merge(tickets::routes())
        .merge(comments::routes())
        .merge(attachments::routes())
        .merge(agents::routes())
        .layer(middleware::from_fn(csrf::csrf_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), session::session_middleware));

    Router::new()
        .merge(public)
        .merge(protected)
        .with_state(state)
}
