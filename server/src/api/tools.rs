use crate::api::auth::pool_from_state;
use crate::middleware::admin::AdminUser;
use crate::services::backup_service::{BackupError, BackupService};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Multipart, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024 * 1024;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tools/backup/export", get(export_backup))
        .route("/api/tools/backup/import", post(import_backup))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportBackupResponse {
    ok: bool,
    message: String,
}

fn map_error(err: BackupError) -> (StatusCode, String) {
    match err {
        BackupError::InvalidArchive(_) | BackupError::UnsupportedVersion(_) => {
            (StatusCode::BAD_REQUEST, err.to_string())
        }
        BackupError::ToolUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn export_backup(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
) -> Result<Response, StatusCode> {
    let temp_dir = tempfile::tempdir().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "export".into())
        .replace(':', "-");
    let zip_path = temp_dir.path().join(format!("coppice-backup-{stamp}.zip"));

    match BackupService::export_to_path(&state.config, &zip_path).await {
        Ok(()) => {}
        Err(err) => {
            let (status, message) = map_error(err);
            return Ok((
                status,
                Json(serde_json::json!({ "message": message })),
            )
                .into_response());
        }
    }

    let bytes = tokio::fs::read(&zip_path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let filename = format!("coppice-backup-{stamp}.zip");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn import_backup(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
    mut multipart: Multipart,
) -> Result<Json<ImportBackupResponse>, (StatusCode, String)> {
    let pool = pool_from_state(&state).map_err(|s| (s, "database unavailable".into()))?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut confirm = false;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid multipart".into()))?
    {
        match field.name() {
            Some("file") => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|_| (StatusCode::BAD_REQUEST, "failed to read file".into()))?;
                if data.len() > MAX_IMPORT_BYTES {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "backup file exceeds maximum size".into(),
                    ));
                }
                file_bytes = Some(data.to_vec());
            }
            Some("confirm") => {
                let value = field
                    .text()
                    .await
                    .map_err(|_| (StatusCode::BAD_REQUEST, "invalid confirm field".into()))?;
                confirm = value == "true" || value == "REPLACE_ALL";
            }
            _ => {}
        }
    }

    if !confirm {
        return Err((
            StatusCode::BAD_REQUEST,
            "confirmation required: set confirm to REPLACE_ALL".into(),
        ));
    }

    let bytes = file_bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "missing file field".into(),
    ))?;

    match BackupService::import_from_bytes(&state.config, pool, &bytes).await {
        Ok(()) => Ok(Json(ImportBackupResponse {
            ok: true,
            message:
                "Import completed. Sign in again if your session was reset; restart the server if anything looks inconsistent."
                    .into(),
        })),
        Err(err) => {
            let (status, message) = map_error(err);
            Err((status, message))
        }
    }
}
