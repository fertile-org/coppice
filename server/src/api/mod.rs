pub mod auth;
mod agent_runs;
mod ws;
mod agents;
mod connectors;
mod attachments;
mod comments;
mod health;
mod jobs;
mod projects;
mod repos;
mod tickets;
mod users;

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
        .merge(connectors::routes())
        .merge(agent_runs::routes())
        .merge(jobs::routes())
        .merge(users::routes())
        .layer(middleware::from_fn(csrf::csrf_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), session::session_middleware));

    let ws = ws::routes().layer(middleware::from_fn_with_state(
        state.clone(),
        session::session_middleware,
    ));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(ws)
        .with_state(state)
}
