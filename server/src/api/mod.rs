pub mod auth;
mod agent_runs;
mod ws;
mod agents;
mod chat;
mod connectors;
mod attachments;
mod code_reviews;
mod comments;
mod mentions;
mod health;
mod jobs;
mod knowledge;
mod knowledge_compaction;
mod notifications;
mod plugins;
mod boards;
mod repos;
mod settings;
mod tickets;
mod tools;
mod users;

use axum::{middleware, Router};
use std::sync::Arc;
use crate::middleware::{csrf, session};
use crate::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    let public = Router::new()
        .merge(health::routes())
        .merge(auth::public_routes())
        .merge(crate::mcp::server::routes());

    let protected = auth::protected_routes()
        .merge(boards::routes())
        .merge(repos::routes())
        .merge(code_reviews::routes())
        .merge(tickets::routes())
        .merge(comments::routes())
        .merge(mentions::routes())
        .merge(attachments::routes())
        .merge(agents::routes())
        .merge(connectors::routes())
        .merge(agent_runs::routes())
        .merge(chat::routes())
        .merge(jobs::routes())
        .merge(knowledge::routes())
        .merge(knowledge_compaction::routes())
        .merge(settings::routes())
        .merge(users::routes())
        .merge(notifications::routes())
        .merge(tools::routes())
        .merge(plugins::routes())
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
