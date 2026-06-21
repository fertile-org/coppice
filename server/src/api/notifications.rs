use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::notification::{Notification, NotificationType};
use crate::events::bus::AppEvent;
use crate::services::notification_service::{
    NotificationError, NotificationFilter, NotificationService,
};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/notifications", get(list_notifications))
        .route("/api/notifications/unread-count", get(unread_count))
        .route("/api/notifications/mark-all-read", post(mark_all_read))
        .route("/api/notifications/{id}/read", post(mark_read))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    filter: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationResponse {
    id: Uuid,
    #[serde(rename = "type")]
    kind: String,
    title: String,
    body: Option<String>,
    ticket_id: Option<Uuid>,
    run_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    comment_id: Option<Uuid>,
    mention_id: Option<Uuid>,
    read_at: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResponse {
    items: Vec<NotificationResponse>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnreadCountResponse {
    count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MarkAllReadResponse {
    marked: u64,
}

fn notification_to_response(n: Notification) -> NotificationResponse {
    NotificationResponse {
        id: n.id,
        kind: notification_type_str(n.kind),
        title: n.title,
        body: n.body,
        ticket_id: n.ticket_id,
        run_id: n.run_id,
        agent_id: n.agent_id,
        comment_id: n.comment_id,
        mention_id: n.mention_id,
        read_at: n.read_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
        created_at: n.created_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn notification_type_str(kind: NotificationType) -> String {
    kind.as_str().to_string()
}

fn map_error(err: NotificationError) -> StatusCode {
    match err {
        NotificationError::NotFound => StatusCode::NOT_FOUND,
        NotificationError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_notifications(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = NotificationService::new(pool);
    let filter = NotificationFilter::parse(query.filter.as_deref());
    let page = service
        .list_for_user(user.id, filter, query.limit, query.cursor.as_deref())
        .await
        .map_err(map_error)?;
    Ok(Json(ListResponse {
        items: page.items.into_iter().map(notification_to_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

async fn unread_count(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
) -> Result<Json<UnreadCountResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = NotificationService::new(pool);
    let count = service.unread_count(user.id).await.map_err(map_error)?;
    Ok(Json(UnreadCountResponse { count }))
}

async fn mark_read(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = NotificationService::new(pool);
    service
        .mark_read(id, user.id)
        .await
        .map_err(map_error)?;
    state.event_bus.publish(AppEvent::NotificationChanged {
        recipient_user_id: Some(user.id),
    });
    Ok(StatusCode::NO_CONTENT)
}

async fn mark_all_read(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
) -> Result<Json<MarkAllReadResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = NotificationService::new(pool);
    let marked = service
        .mark_all_read(user.id)
        .await
        .map_err(map_error)?;
    state.event_bus.publish(AppEvent::NotificationChanged {
        recipient_user_id: Some(user.id),
    });
    Ok(Json(MarkAllReadResponse { marked }))
}
