//! Admin read API for daily JSONL logs and `postgres.log`.

use crate::logs::meta;
use crate::logs::read::{self, FilterParams, LogQuery, QueryError};
use crate::middleware::admin::AdminUser;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

const INVALID_QUERY: &str = "Couldn't read that search. Check it and try again.";
const UNAVAILABLE: &str = "Logs aren't available on this server.";
const LOAD_FAILED: &str = "Couldn't load logs. Try again.";

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/logs", get(list_logs))
        .route("/api/logs/meta", get(logs_meta))
}

#[derive(Deserialize)]
struct LogsParams {
    #[serde(flatten)]
    filter: FilterParams,
    cursor: Option<String>,
    limit: Option<String>,
}

async fn list_logs(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
    Query(params): Query<LogsParams>,
) -> Response {
    let filter = match read::filter_from_params(&params.filter) {
        Ok(filter) => filter,
        Err(error) => return query_error(error),
    };
    let limit = match parse_limit(params.limit.as_deref()) {
        Ok(limit) => limit,
        Err(error) => return query_error(error),
    };
    let page = match read::query(
        std::path::Path::new(&state.config.logs.dir),
        &LogQuery {
            filter,
            cursor: params.cursor.filter(|cursor| !cursor.is_empty()),
            limit,
        },
    ) {
        Ok(page) => page,
        Err(error) => return query_error(error),
    };
    Json(page).into_response()
}

async fn logs_meta(AdminUser(_admin): AdminUser) -> Json<serde_json::Value> {
    Json(json!({
        "retentionDays": meta::RETENTION_DAYS,
        "version": meta::version(),
        "commit": meta::commit(),
        "os": meta::os_name(),
        "arch": meta::arch(),
        "osVersion": meta::os_version(),
    }))
}

fn parse_limit(limit: Option<&str>) -> Result<usize, QueryError> {
    let Some(limit) = limit.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(200);
    };
    let value: usize = limit.parse().map_err(|_| QueryError::BadFilter)?;
    if (1..=1000).contains(&value) {
        Ok(value)
    } else {
        Err(QueryError::BadFilter)
    }
}

fn query_error(error: QueryError) -> Response {
    match error {
        QueryError::BadCursor | QueryError::BadFilter => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "message": INVALID_QUERY })),
        )
            .into_response(),
        QueryError::StoreMissing => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "message": UNAVAILABLE })),
        )
            .into_response(),
        QueryError::Io(error) => {
            tracing::error!(error = %error, "reading logs failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "message": LOAD_FAILED })),
            )
                .into_response()
        }
    }
}
