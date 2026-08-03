use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::knowledge::{
    confidence_from_str, confidence_to_str, scope_from_str, scope_to_str, source_type_from_str,
    source_type_to_str, status_from_str, status_to_str, type_from_str, type_to_str,
    KnowledgeItemView, KnowledgeRevisionInput,
};
use crate::middleware::admin::AdminUser;
use crate::services::knowledge_service::{
    KnowledgeError, KnowledgeListFilter, KnowledgePage, KnowledgeRevisionPatch, KnowledgeService,
};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/knowledge", get(list_knowledge).post(create_knowledge))
        .route("/api/knowledge/inbox", get(list_inbox))
        .route(
            "/api/knowledge/{item_id}",
            get(get_knowledge).patch(edit_knowledge),
        )
        .route("/api/knowledge/{item_id}/approve", post(approve_knowledge))
        .route("/api/knowledge/{item_id}/reject", post(reject_knowledge))
        .route(
            "/api/knowledge/{item_id}/supersede",
            post(supersede_knowledge),
        )
        .route("/api/knowledge/{item_id}/mark-stale", post(mark_stale))
        .route("/api/knowledge/{item_id}/expire", post(expire_knowledge))
        .route(
            "/api/agent-runs/{run_id}/knowledge-used",
            get(knowledge_used),
        )
}

#[derive(Debug)]
struct KnowledgeApiError {
    status: StatusCode,
    message: String,
}

impl KnowledgeApiError {
    fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Knowledge operation failed.".into(),
        }
    }

    fn validation(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
}

impl From<KnowledgeError> for KnowledgeApiError {
    fn from(error: KnowledgeError) -> Self {
        match error {
            KnowledgeError::NotFound => Self {
                status: StatusCode::NOT_FOUND,
                message: error.to_string(),
            },
            KnowledgeError::VersionConflict { .. } | KnowledgeError::Capacity(_) => Self {
                status: StatusCode::CONFLICT,
                message: error.to_string(),
            },
            KnowledgeError::Validation(_) => Self {
                status: StatusCode::BAD_REQUEST,
                message: error.to_string(),
            },
            KnowledgeError::Database(database_error) => {
                tracing::error!(error = %database_error, "knowledge database error");
                Self::internal()
            }
        }
    }
}

impl IntoResponse for KnowledgeApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "message": self.message })),
        )
            .into_response()
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    status: Option<String>,
    project_id: Option<Uuid>,
    knowledge_type: Option<String>,
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevisionBody {
    scope: String,
    project_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    knowledge_type: String,
    title: String,
    content: String,
    #[serde(default = "default_source_type")]
    source_type: String,
    source_id: Option<Uuid>,
    source_run_id: Option<Uuid>,
    #[serde(default = "default_confidence")]
    confidence: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionBody {
    expected_version: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RejectBody {
    expected_version: i32,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditBody {
    expected_version: i32,
    scope: Option<String>,
    project_id: Option<Option<Uuid>>,
    agent_id: Option<Option<Uuid>>,
    knowledge_type: Option<String>,
    title: Option<String>,
    content: Option<String>,
    confidence: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SupersedeBody {
    expected_version: i32,
    replacement: RevisionBody,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpireBody {
    expected_version: i32,
    expires_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeResponse {
    id: Uuid,
    version: i32,
    status: String,
    revision_id: Uuid,
    revision_number: i32,
    active_revision_id: Option<Uuid>,
    scope: String,
    project_id: Option<Uuid>,
    project_name: Option<String>,
    agent_id: Option<Uuid>,
    agent_name: Option<String>,
    knowledge_type: String,
    title: String,
    content: String,
    source_type: String,
    source_id: Option<Uuid>,
    source_run_id: Option<Uuid>,
    confidence: String,
    approved_by: Option<Uuid>,
    approved_at: Option<String>,
    approval_mode: Option<String>,
    policy_decision: Option<String>,
    policy_reason: Option<String>,
    rejection_reason: Option<String>,
    expires_at: Option<String>,
    supersedes_item_id: Option<Uuid>,
    superseded_by: Option<Uuid>,
    stale_at: Option<String>,
    embedding_status: String,
    embedding_error: Option<String>,
    usage_count: i64,
    last_used_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeListResponse {
    items: Vec<KnowledgeResponse>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeUsageResponse {
    item_id: Uuid,
    revision_id: Uuid,
    rank: i32,
    similarity: f64,
    token_count: i32,
    rendered_content: String,
    title: String,
    knowledge_type: String,
    scope: String,
    source_type: String,
    source_id: Option<Uuid>,
    included_at: String,
}

#[derive(Serialize)]
struct KnowledgeUsageListResponse {
    items: Vec<KnowledgeUsageResponse>,
}

fn default_source_type() -> String {
    "human_note".into()
}

fn default_confidence() -> String {
    "medium".into()
}

fn revision_input(body: RevisionBody) -> Result<KnowledgeRevisionInput, KnowledgeApiError> {
    Ok(KnowledgeRevisionInput {
        scope: scope_from_str(&body.scope)
            .ok_or_else(|| KnowledgeApiError::validation("invalid scope"))?,
        project_id: body.project_id,
        agent_id: body.agent_id,
        knowledge_type: type_from_str(&body.knowledge_type)
            .ok_or_else(|| KnowledgeApiError::validation("invalid knowledgeType"))?,
        title: body.title,
        content: body.content,
        source_type: source_type_from_str(&body.source_type)
            .ok_or_else(|| KnowledgeApiError::validation("invalid sourceType"))?,
        source_id: body.source_id,
        source_run_id: body.source_run_id,
        confidence: confidence_from_str(&body.confidence)
            .ok_or_else(|| KnowledgeApiError::validation("invalid confidence"))?,
    })
}

fn page_filter(query: ListQuery) -> Result<KnowledgeListFilter, KnowledgeApiError> {
    Ok(KnowledgeListFilter {
        status: query
            .status
            .as_deref()
            .map(|value| {
                status_from_str(value)
                    .ok_or_else(|| KnowledgeApiError::validation("invalid status"))
            })
            .transpose()?,
        project_id: query.project_id,
        knowledge_type: query
            .knowledge_type
            .as_deref()
            .map(|value| {
                type_from_str(value)
                    .ok_or_else(|| KnowledgeApiError::validation("invalid knowledgeType"))
            })
            .transpose()?,
        cursor: query.cursor,
        limit: query.limit,
    })
}

fn page_response(page: KnowledgePage) -> KnowledgeListResponse {
    KnowledgeListResponse {
        items: page.items.into_iter().map(item_response).collect(),
        next_cursor: page.next_cursor,
    }
}

fn item_response(item: KnowledgeItemView) -> KnowledgeResponse {
    let format = |value: Option<OffsetDateTime>| {
        value.map(|timestamp| timestamp.format(&Rfc3339).unwrap_or_default())
    };
    KnowledgeResponse {
        id: item.id,
        version: item.version,
        status: status_to_str(item.status).into(),
        revision_id: item.revision_id,
        revision_number: item.revision_number,
        active_revision_id: item.active_revision_id,
        scope: scope_to_str(item.scope).into(),
        project_id: item.project_id,
        project_name: item.project_name,
        agent_id: item.agent_id,
        agent_name: item.agent_name,
        knowledge_type: type_to_str(item.knowledge_type).into(),
        title: item.title,
        content: item.content,
        source_type: source_type_to_str(item.source_type).into(),
        source_id: item.source_id,
        source_run_id: item.source_run_id,
        confidence: confidence_to_str(item.confidence).into(),
        approved_by: item.approved_by,
        approved_at: format(item.approved_at),
        approval_mode: item.approval_mode,
        policy_decision: item.policy_decision,
        policy_reason: item.policy_reason,
        rejection_reason: item.rejection_reason,
        expires_at: format(item.expires_at),
        supersedes_item_id: item.supersedes_item_id,
        superseded_by: item.superseded_by,
        stale_at: format(item.stale_at),
        embedding_status: item.embedding_status,
        embedding_error: item.embedding_error,
        usage_count: item.usage_count,
        last_used_at: format(item.last_used_at),
        created_at: item.created_at.format(&Rfc3339).unwrap_or_default(),
        updated_at: item.updated_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn pool(state: &AppState) -> Result<&sqlx::PgPool, KnowledgeApiError> {
    pool_from_state(state).map_err(|_| KnowledgeApiError::internal())
}

async fn list_knowledge(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<KnowledgeListResponse>, KnowledgeApiError> {
    let page = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .list(page_filter(query)?)
        .await?;
    Ok(Json(page_response(page)))
}

async fn list_inbox(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Query(mut query): Query<ListQuery>,
) -> Result<Json<KnowledgeListResponse>, KnowledgeApiError> {
    query.status = Some("pending".into());
    let page = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .list(page_filter(query)?)
        .await?;
    Ok(Json(page_response(page)))
}

async fn get_knowledge(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<KnowledgeResponse>, KnowledgeApiError> {
    let item = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .get(item_id)
        .await?;
    Ok(Json(item_response(item)))
}

async fn create_knowledge(
    State(state): State<Arc<AppState>>,
    AdminUser(auth): AdminUser,
    Json(body): Json<RevisionBody>,
) -> Result<(StatusCode, Json<KnowledgeResponse>), KnowledgeApiError> {
    let item = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .create_manual(auth.user.id, revision_input(body)?)
        .await?;
    Ok((StatusCode::CREATED, Json(item_response(item))))
}

async fn approve_knowledge(
    State(state): State<Arc<AppState>>,
    AdminUser(auth): AdminUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<VersionBody>,
) -> Result<Json<KnowledgeResponse>, KnowledgeApiError> {
    let item = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .approve(item_id, body.expected_version, auth.user.id)
        .await?;
    Ok(Json(item_response(item)))
}

async fn edit_knowledge(
    State(state): State<Arc<AppState>>,
    AdminUser(auth): AdminUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<EditBody>,
) -> Result<Json<KnowledgeResponse>, KnowledgeApiError> {
    let patch = KnowledgeRevisionPatch {
        scope: body
            .scope
            .as_deref()
            .map(|value| {
                scope_from_str(value).ok_or_else(|| KnowledgeApiError::validation("invalid scope"))
            })
            .transpose()?,
        project_id: body.project_id,
        agent_id: body.agent_id,
        knowledge_type: body
            .knowledge_type
            .as_deref()
            .map(|value| {
                type_from_str(value)
                    .ok_or_else(|| KnowledgeApiError::validation("invalid knowledgeType"))
            })
            .transpose()?,
        title: body.title,
        content: body.content,
        confidence: body
            .confidence
            .as_deref()
            .map(|value| {
                confidence_from_str(value)
                    .ok_or_else(|| KnowledgeApiError::validation("invalid confidence"))
            })
            .transpose()?,
    };
    let item = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .edit(item_id, body.expected_version, auth.user.id, patch)
        .await?;
    Ok(Json(item_response(item)))
}

async fn reject_knowledge(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<RejectBody>,
) -> Result<Json<KnowledgeResponse>, KnowledgeApiError> {
    let item = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .reject(item_id, body.expected_version, body.reason.as_deref())
        .await?;
    Ok(Json(item_response(item)))
}

async fn mark_stale(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<VersionBody>,
) -> Result<Json<KnowledgeResponse>, KnowledgeApiError> {
    let item = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .mark_stale(item_id, body.expected_version)
        .await?;
    Ok(Json(item_response(item)))
}

async fn expire_knowledge(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<ExpireBody>,
) -> Result<Json<KnowledgeResponse>, KnowledgeApiError> {
    let expires_at = body
        .expires_at
        .as_deref()
        .map(|value| OffsetDateTime::parse(value, &Rfc3339))
        .transpose()
        .map_err(|_| KnowledgeApiError::validation("expiresAt must be RFC3339"))?
        .unwrap_or_else(OffsetDateTime::now_utc);
    let item = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .expire(item_id, body.expected_version, expires_at)
        .await?;
    Ok(Json(item_response(item)))
}

async fn supersede_knowledge(
    State(state): State<Arc<AppState>>,
    AdminUser(auth): AdminUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<SupersedeBody>,
) -> Result<(StatusCode, Json<KnowledgeResponse>), KnowledgeApiError> {
    let item = KnowledgeService::new(pool(&state)?, &state.config.knowledge)
        .supersede(
            item_id,
            body.expected_version,
            auth.user.id,
            revision_input(body.replacement)?,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(item_response(item))))
}

async fn knowledge_used(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(run_id): Path<Uuid>,
) -> Result<Json<KnowledgeUsageListResponse>, KnowledgeApiError> {
    let pool = pool(&state)?;
    let run_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_runs WHERE id = $1)")
            .bind(run_id)
            .fetch_one(pool)
            .await
            .map_err(|_| KnowledgeApiError::internal())?;
    if !run_exists {
        return Err(KnowledgeApiError {
            status: StatusCode::NOT_FOUND,
            message: "agent run not found".into(),
        });
    }
    let rows = sqlx::query(
        r#"
        SELECT u.item_id, u.revision_id, u.rank, u.similarity, u.token_count,
               u.rendered_content, u.included_at, r.title, r.knowledge_type,
               r.scope, r.source_type, r.source_id
        FROM knowledge_usage_logs u
        JOIN knowledge_revisions r ON r.id = u.revision_id
        WHERE u.run_id = $1
        ORDER BY u.rank, u.revision_id
        "#,
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|_| KnowledgeApiError::internal())?;
    let items = rows
        .into_iter()
        .map(|row| -> Result<KnowledgeUsageResponse, sqlx::Error> {
            let included_at: OffsetDateTime = row.try_get("included_at")?;
            Ok(KnowledgeUsageResponse {
                item_id: row.try_get("item_id")?,
                revision_id: row.try_get("revision_id")?,
                rank: row.try_get("rank")?,
                similarity: row.try_get("similarity")?,
                token_count: row.try_get("token_count")?,
                rendered_content: row.try_get("rendered_content")?,
                title: row.try_get("title")?,
                knowledge_type: row.try_get("knowledge_type")?,
                scope: row.try_get("scope")?,
                source_type: row.try_get("source_type")?,
                source_id: row.try_get("source_id")?,
                included_at: included_at.format(&Rfc3339).unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| KnowledgeApiError::internal())?;
    Ok(Json(KnowledgeUsageListResponse { items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_team_scope() {
        let result = revision_input(RevisionBody {
            scope: "team".into(),
            project_id: None,
            agent_id: None,
            knowledge_type: "test_command".into(),
            title: "Test".into(),
            content: "make test".into(),
            source_type: default_source_type(),
            source_id: None,
            source_run_id: None,
            confidence: default_confidence(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn source_type_enum_stays_used_in_api_contract() {
        assert_eq!(
            source_type_to_str(crate::domain::knowledge::KnowledgeSourceType::HumanNote),
            "human_note"
        );
    }
}
