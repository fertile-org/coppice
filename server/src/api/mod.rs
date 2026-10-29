mod agent_runs;
mod agents;
mod attachments;
pub mod auth;
mod boards;
mod chat;
mod code_reviews;
mod comments;
mod connectors;
mod health;
mod init_wizard;
mod jobs;
mod knowledge;
mod knowledge_compaction;
mod logs;
mod marketplace;
mod mentions;
mod notifications;
mod plugins;
mod repos;
mod settings;
mod ticket_files;
mod tickets;
mod tool_connectors;
mod tools;
mod users;
mod ws;

use crate::middleware::{csrf, host_guard, request_log, session};
use crate::AppState;
use axum::http::StatusCode;
use axum::{middleware, Router};
use std::sync::Arc;

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
        .merge(ticket_files::routes())
        .merge(comments::routes())
        .merge(mentions::routes())
        .merge(attachments::routes(state.attachments.max_bytes()))
        .merge(agents::routes())
        .merge(connectors::routes())
        .merge(agent_runs::routes())
        .merge(chat::routes())
        .merge(jobs::routes())
        .merge(knowledge::routes())
        .merge(knowledge_compaction::routes())
        .merge(settings::routes())
        .merge(logs::routes())
        .merge(users::routes())
        .merge(notifications::routes())
        .merge(tools::routes())
        .merge(tool_connectors::routes())
        .merge(plugins::routes())
        .merge(marketplace::routes())
        .merge(init_wizard::routes())
        .layer(middleware::from_fn(csrf::csrf_middleware))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            session::session_middleware,
        ));

    let ws = ws::routes().layer(middleware::from_fn_with_state(
        state.clone(),
        session::session_middleware,
    ));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(ws)
        // Before the layers. Axum only wraps routes that already exist, and
        // the desktop shell forwards unknown /api paths back into this
        // fallback so the request log still sees the 404.
        .fallback(StatusCode::NOT_FOUND)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            host_guard::desktop_host_guard,
        ))
        .layer(middleware::from_fn(request_log::log_request))
        .with_state(state)
}
