use crate::api::auth::{pool_from_state, AuthUser};
use crate::services::comment_service::{CommentError, CommentService};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/attachments", post(upload_attachment))
        .route("/api/attachments/{attachment_id}", get(get_attachment))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentUploadResponse {
    id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
}

fn map_error(err: CommentError) -> StatusCode {
    match err {
        CommentError::AttachmentNotFound => StatusCode::NOT_FOUND,
        CommentError::Validation(_) => StatusCode::BAD_REQUEST,
        CommentError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn upload_attachment(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<AttachmentUploadResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CommentService::new(pool);

    let mut filename = String::from("file");
    let mut content_type = String::from("application/octet-stream");
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        if field.name() != Some("file") {
            continue;
        }

        if let Some(name) = field.file_name() {
            filename = name.to_string();
        }
        if let Some(ct) = field.content_type() {
            content_type = ct.to_string();
        }
        bytes = Some(
            field
                .bytes()
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .to_vec(),
        );
        break;
    }

    let bytes = bytes.ok_or(StatusCode::BAD_REQUEST)?;
    if bytes.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let id = Uuid::new_v4();
    let storage_path = state
        .attachments
        .save(id, &filename, &content_type, &bytes)
        .map_err(|err| {
            if err.to_string().contains("file too large") {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    let attachment = service
        .create_attachment(
            id,
            &filename,
            &content_type,
            bytes.len() as i64,
            storage_path.to_string_lossy().as_ref(),
            user.id,
        )
        .await
        .map_err(map_error)?;

    Ok((
        StatusCode::CREATED,
        Json(AttachmentUploadResponse {
            id: attachment.id,
            filename: attachment.filename,
            content_type: attachment.content_type,
            size_bytes: attachment.size_bytes,
        }),
    ))
}

async fn get_attachment(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(attachment_id): Path<Uuid>,
) -> Result<Response, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CommentService::new(pool);
    let attachment = service
        .get_attachment(attachment_id)
        .await
        .map_err(map_error)?;

    let contents = tokio::fs::read(&attachment.storage_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, attachment.content_type),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{}\"", attachment.filename),
            ),
        ],
        Body::from(contents),
    )
        .into_response())
}
