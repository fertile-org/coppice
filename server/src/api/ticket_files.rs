use crate::api::auth::{pool_from_state, AuthUser};
use crate::services::ticket_file_service::{TicketFileError, TicketFileService};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderName, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

/// Tells the SPA whether the file came from the ticket worktree or the repo checkout.
const FILE_SOURCE_HEADER: HeaderName = HeaderName::from_static("x-coppice-file-source");
/// Percent-encoded repo-relative path that was served (ASCII-safe for headers).
const FILE_PATH_HEADER: HeaderName = HeaderName::from_static("x-coppice-file-path");

fn encode_file_path_header(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route(
        "/api/tickets/{ticket_id}/files/content",
        get(get_file_content),
    )
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

fn map_error(err: TicketFileError) -> StatusCode {
    match err {
        TicketFileError::TicketNotFound | TicketFileError::NotFound => StatusCode::NOT_FOUND,
        TicketFileError::Ambiguous => StatusCode::CONFLICT,
        TicketFileError::NoRepo | TicketFileError::RepoNotReady | TicketFileError::InvalidPath => {
            StatusCode::BAD_REQUEST
        }
        TicketFileError::Unsupported => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        TicketFileError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        TicketFileError::Database(_) | TicketFileError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn get_file_content(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Query(query): Query<FileQuery>,
) -> Result<Response, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = TicketFileService::new(pool, state.config.agent.worktrees_path.clone().into());
    let file = service
        .read(ticket_id, &query.path)
        .await
        .map_err(map_error)?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, file.content_type.to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            // Repo files are untrusted: an SVG opened directly must not run script.
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; sandbox".to_string(),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (FILE_SOURCE_HEADER, file.source.as_str().to_string()),
            (
                FILE_PATH_HEADER,
                encode_file_path_header(&file.resolved_path),
            ),
        ],
        Body::from(file.bytes),
    )
        .into_response())
}
