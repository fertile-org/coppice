mod common;

use async_trait::async_trait;
use axum::{http::StatusCode, Router};
use coppice_server::domain::knowledge::{
    KnowledgeConfidence, KnowledgeRevisionInput, KnowledgeScope, KnowledgeSourceType, KnowledgeType,
};
use coppice_server::knowledge::embedder::EmbeddingError;
use coppice_server::knowledge::extractor::{
    ExtractedCandidate, ExtractionError, ExtractionInput, ExtractionProvider,
    MockExtractionProvider,
};
use coppice_server::knowledge::retrieval::{has_eligible, retrieve, RETRIEVAL_QUERY_SQL};
use coppice_server::knowledge::{embedder::EmbeddingProvider, embedding_provider};
use coppice_server::services::context_budget::{record_usage, render_knowledge, ByteTokenCounter};
use coppice_server::services::knowledge_job_service::KnowledgeJobService;
use coppice_server::services::knowledge_service::{
    activate_embedded_revision, KnowledgeError, KnowledgeListFilter, KnowledgeRevisionPatch,
    KnowledgeService,
};
use coppice_server::workers::knowledge_worker;
use coppice_server::AppState;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

async fn create_candidate(
    app: &Router,
    project_id: &str,
    cookie: &str,
    csrf: &str,
    title: &str,
) -> serde_json::Value {
    let body = serde_json::json!({
        "scope": "project",
        "projectId": project_id,
        "knowledgeType": "test_command",
        "title": title,
        "content": "Run make test-unit before review.",
        "sourceType": "human_note",
        "confidence": "high"
    });
    let response = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/knowledge",
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    common::json_body(response).await
}

async fn create_project_named(app: &Router, name: &str, cookie: &str, csrf: &str) -> String {
    let response = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/projects",
            &serde_json::json!({"name": name}).to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    common::json_body(response).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn mutate(
    app: &Router,
    method: &str,
    path: &str,
    body: serde_json::Value,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(common::json_request(
            method,
            path,
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    let status = response.status();
    (status, common::json_body(response).await)
}

fn supersede_body(
    project_id: &str,
    expected_version: i64,
    title: &str,
    content: &str,
) -> serde_json::Value {
    serde_json::json!({
        "expectedVersion": expected_version,
        "replacement": {
            "scope": "project",
            "projectId": project_id,
            "knowledgeType": "test_command",
            "title": title,
            "content": content,
            "sourceType": "human_note",
            "confidence": "high"
        }
    })
}

async fn process_one_knowledge_job(state: &Arc<AppState>) -> anyhow::Result<bool> {
    let embedder = embedding_provider(&state.config.knowledge.embedding)?;
    let extractor: Arc<dyn ExtractionProvider> = Arc::new(MockExtractionProvider);
    knowledge_worker::process_one(state, "integration-knowledge", &embedder, &extractor).await
}

struct ReclaimingEmbeddingProvider {
    pool: PgPool,
    revision_id: Uuid,
}

#[async_trait]
impl EmbeddingProvider for ReclaimingEmbeddingProvider {
    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        sqlx::query(
            "UPDATE knowledge_jobs SET locked_at = now() - interval '301 seconds' WHERE revision_id = $1 AND status = 'running'",
        )
        .bind(self.revision_id)
        .execute(&self.pool)
        .await
        .map_err(|error| EmbeddingError::Request(error.to_string()))?;
        KnowledgeJobService::new(&self.pool)
            .claim_next("fresh-embedding-worker", 300)
            .await
            .map_err(|error| EmbeddingError::Request(error.to_string()))?
            .ok_or_else(|| EmbeddingError::Request("failed to reclaim embedding job".into()))?;
        Ok(vec![std::iter::once(1.0)
            .chain(std::iter::repeat_n(0.0, 1_535))
            .collect()])
    }

    fn provider_name(&self) -> &str {
        "reclaiming-test"
    }

    fn model_name(&self) -> &str {
        "reclaiming-test-1536"
    }

    fn dimension(&self) -> usize {
        1_536
    }
}

struct ReclaimingExtractionProvider {
    pool: PgPool,
    ticket_id: Uuid,
}

#[async_trait]
impl ExtractionProvider for ReclaimingExtractionProvider {
    async fn extract(
        &self,
        _input: &ExtractionInput,
    ) -> Result<Vec<ExtractedCandidate>, ExtractionError> {
        sqlx::query(
            "UPDATE knowledge_jobs SET locked_at = now() - interval '301 seconds' WHERE ticket_id = $1 AND status = 'running'",
        )
        .bind(self.ticket_id)
        .execute(&self.pool)
        .await
        .map_err(|error| ExtractionError::InvalidInput(error.to_string()))?;
        KnowledgeJobService::new(&self.pool)
            .claim_next("fresh-extraction-worker", 300)
            .await
            .map_err(|error| ExtractionError::InvalidInput(error.to_string()))?
            .ok_or_else(|| {
                ExtractionError::InvalidInput("failed to reclaim extraction job".into())
            })?;
        Ok(vec![ExtractedCandidate {
            knowledge_type: KnowledgeType::ReviewFeedback,
            title: "Stale extraction candidate".into(),
            content: "A stale claim must never persist this candidate.".into(),
            confidence: KnowledgeConfidence::High,
            should_require_human_approval: true,
            source_type: KnowledgeSourceType::AgentSummary,
            source_id: None,
        }])
    }
}

struct CommentReviewExtractionProvider;

#[async_trait]
impl ExtractionProvider for CommentReviewExtractionProvider {
    async fn extract(
        &self,
        input: &ExtractionInput,
    ) -> Result<Vec<ExtractedCandidate>, ExtractionError> {
        let comment = input
            .comments
            .iter()
            .find(|comment| comment.source_type == KnowledgeSourceType::Comment)
            .ok_or_else(|| ExtractionError::InvalidInput("comment source missing".into()))?;
        let review = input
            .comments
            .iter()
            .find(|comment| comment.source_type == KnowledgeSourceType::Review)
            .ok_or_else(|| ExtractionError::InvalidInput("review source missing".into()))?;
        Ok(vec![
            ExtractedCandidate {
                knowledge_type: KnowledgeType::BugPattern,
                title: "Comment-sourced pattern".into(),
                content: comment.body.clone(),
                confidence: KnowledgeConfidence::High,
                should_require_human_approval: true,
                source_type: KnowledgeSourceType::Comment,
                source_id: Some(comment.id),
            },
            ExtractedCandidate {
                knowledge_type: KnowledgeType::ReviewFeedback,
                title: "Review-sourced feedback".into(),
                content: review.body.clone(),
                confidence: KnowledgeConfidence::High,
                should_require_human_approval: true,
                source_type: KnowledgeSourceType::Review,
                source_id: Some(review.id),
            },
        ])
    }
}

struct OrderedCommentExtractionProvider {
    expected_comment_ids: [Uuid; 2],
    max_source_bytes: usize,
}

struct BoundedExtractionProvider {
    max_source_bytes: usize,
}

#[async_trait]
impl ExtractionProvider for BoundedExtractionProvider {
    async fn extract(
        &self,
        input: &ExtractionInput,
    ) -> Result<Vec<ExtractedCandidate>, ExtractionError> {
        let total_bytes = input.title.len()
            + input.description.len()
            + input
                .comments
                .iter()
                .map(|comment| comment.body.len())
                .sum::<usize>();
        if total_bytes > self.max_source_bytes {
            return Err(ExtractionError::InvalidInput(format!(
                "source snapshot used {total_bytes} bytes, above {}",
                self.max_source_bytes
            )));
        }
        Ok(Vec::new())
    }
}

#[async_trait]
impl ExtractionProvider for OrderedCommentExtractionProvider {
    async fn extract(
        &self,
        input: &ExtractionInput,
    ) -> Result<Vec<ExtractedCandidate>, ExtractionError> {
        let total_bytes = input.title.len()
            + input.description.len()
            + input
                .comments
                .iter()
                .map(|comment| comment.body.len())
                .sum::<usize>();
        if total_bytes > self.max_source_bytes {
            return Err(ExtractionError::InvalidInput(format!(
                "source snapshot used {total_bytes} bytes, above {}",
                self.max_source_bytes
            )));
        }
        let comment_ids = input
            .comments
            .iter()
            .map(|comment| comment.id)
            .collect::<Vec<_>>();
        if comment_ids != self.expected_comment_ids {
            return Err(ExtractionError::InvalidInput(
                format!(
                    "expected retained comments in chronological order {:?}, got {comment_ids:?}",
                    self.expected_comment_ids
                ),
            ));
        }
        Ok(Vec::new())
    }
}

fn unit_vector_literal() -> String {
    format!("[1{}]", ",0".repeat(1_535))
}

#[derive(Clone, Copy)]
struct RetrievalSeed<'a> {
    label: &'a str,
    status: &'a str,
    scope: &'a str,
    project_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    confidence: &'a str,
    expired: bool,
    activate: bool,
    store_embedding: bool,
}

async fn seed_retrieval_item(pool: &PgPool, seed: RetrievalSeed<'_>) -> (Uuid, Uuid) {
    let item_id = Uuid::new_v4();
    let revision_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO knowledge_items (id, status, version, expires_at)
        VALUES ($1, $2, 1, CASE WHEN $3 THEN now() - interval '1 minute' ELSE NULL END)
        "#,
    )
    .bind(item_id)
    .bind(seed.status)
    .bind(seed.expired)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO knowledge_revisions (
            id, item_id, revision_number, scope, project_id, agent_id,
            knowledge_type, title, content, source_type, confidence
        ) VALUES ($1, $2, 1, $3, $4, $5, 'test_command', $6, $7, 'human_note', $8)
        "#,
    )
    .bind(revision_id)
    .bind(item_id)
    .bind(seed.scope)
    .bind(seed.project_id)
    .bind(seed.agent_id)
    .bind(seed.label)
    .bind(format!("Retrieval matrix entry: {}", seed.label))
    .bind(seed.confidence)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        UPDATE knowledge_items
        SET current_revision_id = $2,
            active_revision_id = CASE WHEN $3 THEN $2 ELSE NULL END
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(revision_id)
    .bind(seed.activate)
    .execute(pool)
    .await
    .unwrap();
    if seed.store_embedding {
        sqlx::query(
            r#"
            INSERT INTO knowledge_embeddings (
                revision_id, provider, model, embedding_dimension, embedding
            ) VALUES ($1, 'matrix', 'unit-vector', 1536, $2::vector)
            "#,
        )
        .bind(revision_id)
        .bind(unit_vector_literal())
        .execute(pool)
        .await
        .unwrap();
    }
    (item_id, revision_id)
}

async fn seed_retrieval_cardinality(
    tx: &mut Transaction<'_, Postgres>,
    target_project_id: Uuid,
    other_project_id: Uuid,
    target_approved: i32,
    other_approved: i32,
    rejected: i32,
) {
    sqlx::query(
        r#"
        CREATE TEMP TABLE knowledge_plan_seed (
            ordinal INT PRIMARY KEY,
            item_id UUID NOT NULL,
            revision_id UUID NOT NULL,
            status TEXT NOT NULL,
            project_id UUID NOT NULL
        ) ON COMMIT DROP
        "#,
    )
    .execute(&mut **tx)
    .await
    .unwrap();

    let approved = target_approved + other_approved;
    let total = approved + rejected;
    sqlx::query(
        r#"
        INSERT INTO knowledge_plan_seed (
            ordinal, item_id, revision_id, status, project_id
        )
        SELECT ordinal,
               gen_random_uuid(),
               gen_random_uuid(),
               CASE WHEN ordinal <= $4 THEN 'approved' ELSE 'rejected' END,
               CASE WHEN ordinal <= $3 THEN $1 ELSE $2 END
        FROM generate_series(1, $5) AS ordinal
        "#,
    )
    .bind(target_project_id)
    .bind(other_project_id)
    .bind(target_approved)
    .bind(approved)
    .bind(total)
    .execute(&mut **tx)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO knowledge_items (id, status, version)
        SELECT item_id, status, 1 FROM knowledge_plan_seed
        "#,
    )
    .execute(&mut **tx)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO knowledge_revisions (
            id, item_id, revision_number, scope, project_id, knowledge_type,
            title, content, source_type, confidence
        )
        SELECT revision_id, item_id, 1, 'project', project_id, 'test_command',
               'Retrieval plan seed ' || ordinal,
               'Use the bounded retrieval plan.', 'human_note', 'high'
        FROM knowledge_plan_seed
        "#,
    )
    .execute(&mut **tx)
    .await
    .unwrap();
    sqlx::query(
        r#"
        UPDATE knowledge_items item
        SET current_revision_id = seed.revision_id,
            active_revision_id = CASE
                WHEN seed.status = 'approved' THEN seed.revision_id
                ELSE NULL
            END
        FROM knowledge_plan_seed seed
        WHERE item.id = seed.item_id
        "#,
    )
    .execute(&mut **tx)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO knowledge_embeddings (
            revision_id, provider, model, embedding_dimension, embedding
        )
        SELECT revision_id, 'benchmark', 'unit-vector', 1536, $1::vector
        FROM knowledge_plan_seed
        WHERE status = 'approved'
        "#,
    )
    .bind(unit_vector_literal())
    .execute(&mut **tx)
    .await
    .unwrap();
    sqlx::query("ANALYZE knowledge_items, knowledge_revisions, knowledge_embeddings")
        .execute(&mut **tx)
        .await
        .unwrap();
}

async fn explain_production_retrieval(
    tx: &mut Transaction<'_, Postgres>,
    project_id: Uuid,
    analyze: bool,
) -> Value {
    let options = if analyze {
        "ANALYZE, BUFFERS, VERBOSE, FORMAT JSON"
    } else {
        "VERBOSE, FORMAT JSON"
    };
    let sql = format!("EXPLAIN ({options}) {RETRIEVAL_QUERY_SQL}");
    sqlx::query_scalar(&sql)
        .bind(project_id)
        .bind(Uuid::new_v4())
        .bind("low")
        .bind(unit_vector_literal())
        .bind(-1.0_f64)
        .bind(20_i64)
        .bind(Vec::<String>::new())
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

fn collect_index_scan_names(value: &Value, names: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            let node_type = object
                .get("Node Type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if node_type.contains("Index") {
                if let Some(name) = object.get("Index Name").and_then(Value::as_str) {
                    names.push(name.to_string());
                }
            }
            for child in object.values() {
                collect_index_scan_names(child, names);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_index_scan_names(child, names);
            }
        }
        _ => {}
    }
}

fn find_plan_node<'a>(value: &'a Value, key: &str, expected: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => {
            if object.get(key).and_then(Value::as_str) == Some(expected) {
                return Some(value);
            }
            object
                .values()
                .find_map(|child| find_plan_node(child, key, expected))
        }
        Value::Array(array) => array
            .iter()
            .find_map(|child| find_plan_node(child, key, expected)),
        _ => None,
    }
}

#[tokio::test]
async fn lifecycle_is_concurrency_safe_and_preserves_active_revision() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let created = create_candidate(&app, &project_id, &cookie, &csrf, "Unit test command").await;
    let item_id = created["id"].as_str().unwrap();
    assert_eq!(created["status"], "pending");
    assert_eq!(created["version"], 1);

    let (status, approved) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{item_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["embeddingStatus"], "pending");

    let (status, conflict) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{item_id}/reject"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(conflict["message"]
        .as_str()
        .unwrap()
        .contains("current version is 2"));

    assert!(process_one_knowledge_job(&state).await.unwrap());
    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/knowledge/{item_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let ready = common::json_body(response).await;
    assert_eq!(ready["embeddingStatus"], "ready");
    let old_active = ready["activeRevisionId"].as_str().unwrap().to_string();

    let (status, edited) = mutate(
        &app,
        "PATCH",
        &format!("/api/knowledge/{item_id}"),
        serde_json::json!({
            "expectedVersion": 2,
            "title": "Updated unit test command",
            "content": "Run make test-smoke before review."
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["version"], 3);
    assert_ne!(edited["revisionId"], old_active);
    assert_eq!(edited["activeRevisionId"], old_active);
    assert_eq!(edited["embeddingStatus"], "pending");

    assert!(process_one_knowledge_job(&state).await.unwrap());
    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/knowledge/{item_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let replaced = common::json_body(response).await;
    assert_eq!(replaced["activeRevisionId"], replaced["revisionId"]);

    let (status, rejected) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{item_id}/reject"),
        serde_json::json!({"expectedVersion": 3, "reason": "obsolete"}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["status"], "rejected");
    assert!(rejected["activeRevisionId"].is_null());
    assert_eq!(rejected["rejectionReason"], "obsolete");

    let (status, reapproved) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{item_id}/approve"),
        serde_json::json!({"expectedVersion": 4}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reapproved["status"], "approved");
    assert_eq!(reapproved["activeRevisionId"], reapproved["revisionId"]);
    assert_eq!(reapproved["embeddingStatus"], "ready");
}

#[tokio::test]
async fn edit_distinguishes_omitted_and_explicitly_null_scope_ids() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Scoped Knowledge Agent",
        &cookie,
        &csrf,
    )
    .await;
    let created = mutate(
        &app,
        "POST",
        "/api/knowledge",
        serde_json::json!({
            "scope": "agent",
            "projectId": project_id,
            "agentId": agent_id,
            "knowledgeType": "test_command",
            "title": "Scoped command",
            "content": "Run the scoped command.",
            "sourceType": "human_note",
            "confidence": "high"
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(created.0, StatusCode::CREATED);
    let item_id = created.1["id"].as_str().unwrap();

    let (status, preserved) = mutate(
        &app,
        "PATCH",
        &format!("/api/knowledge/{item_id}"),
        serde_json::json!({
            "expectedVersion": 1,
            "title": "Renamed scoped command"
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preserved["scope"], "agent");
    assert_eq!(preserved["projectId"], project_id);
    assert_eq!(preserved["agentId"], agent_id);

    let (status, cleared) = mutate(
        &app,
        "PATCH",
        &format!("/api/knowledge/{item_id}"),
        serde_json::json!({
            "expectedVersion": 2,
            "scope": "workspace",
            "projectId": null,
            "agentId": null
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["scope"], "workspace");
    assert!(cleared["projectId"].is_null());
    assert!(cleared["agentId"].is_null());
}

#[tokio::test]
async fn knowledge_list_has_stable_hard_limited_pages_and_auth_failures() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    for title in ["Pagination one", "Pagination two", "Pagination three"] {
        create_candidate(&app, &project_id, &cookie, &csrf, title).await;
    }

    let unauthenticated = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/knowledge")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let malformed_cursor = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/knowledge?cursor=not-a-cursor",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(malformed_cursor.status(), StatusCode::BAD_REQUEST);

    let mut bounded_config = state.config.knowledge.clone();
    bounded_config.retrieval.max_page_size = 2;
    bounded_config.retrieval.default_page_size = 2;
    let service = KnowledgeService::new(state.db.as_ref().unwrap(), &bounded_config);
    let first = service
        .list(KnowledgeListFilter {
            status: None,
            project_id: None,
            knowledge_type: None,
            cursor: None,
            limit: Some(1_000),
        })
        .await
        .unwrap();
    assert_eq!(first.items.len(), 2);
    let cursor = first.next_cursor.expect("hard-limited first page cursor");
    let first_ids = first
        .items
        .into_iter()
        .map(|item| item.id)
        .collect::<std::collections::HashSet<_>>();
    let second = service
        .list(KnowledgeListFilter {
            status: None,
            project_id: None,
            knowledge_type: None,
            cursor: Some(cursor),
            limit: Some(1_000),
        })
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(second.next_cursor.is_none());
    assert!(second
        .items
        .iter()
        .all(|item| !first_ids.contains(&item.id)));

    sqlx::query("UPDATE users SET role = 'member' WHERE email = 'admin@localhost'")
        .execute(state.db.as_ref().unwrap())
        .await
        .unwrap();
    let forbidden = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/knowledge",
            &serde_json::json!({
                "scope": "workspace",
                "knowledgeType": "test_command",
                "title": "Member write",
                "content": "Members cannot mutate governed knowledge.",
                "sourceType": "human_note",
                "confidence": "high"
            })
            .to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn terminal_knowledge_edits_do_not_consume_active_capacity() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let live = create_candidate(&app, &project_id, &cookie, &csrf, "Live candidate").await;
    let terminal = create_candidate(
        &app,
        &project_id,
        &cookie,
        &csrf,
        "Rejected candidate to revise",
    )
    .await;
    let terminal_id = terminal["id"].as_str().unwrap();
    let (status, rejected) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{terminal_id}/reject"),
        serde_json::json!({"expectedVersion": 1, "reason": "needs revision"}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["status"], "rejected");

    let pool = state.db.as_ref().unwrap();
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users ORDER BY created_at, id LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut limited_config = state.config.knowledge.clone();
    limited_config.retrieval.max_active_per_project = 1;
    let service = KnowledgeService::new(pool, &limited_config);
    let terminal_uuid = Uuid::parse_str(terminal_id).unwrap();
    let edited = service
        .edit(
            terminal_uuid,
            2,
            user_id,
            KnowledgeRevisionPatch {
                title: Some("Revised rejected history".into()),
                ..KnowledgeRevisionPatch::default()
            },
        )
        .await
        .expect("editing terminal history must not consume active capacity");
    assert_eq!(
        edited.status,
        coppice_server::domain::knowledge::KnowledgeStatus::Rejected
    );
    assert_eq!(edited.version, 3);

    let approval = service.approve(terminal_uuid, 3, user_id).await;
    assert!(matches!(approval, Err(KnowledgeError::Capacity(_))));
    assert_eq!(live["status"], "pending");
}

#[tokio::test]
async fn cross_scope_edit_reserves_both_active_and_current_capacity() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_a = common::create_test_project(&app, &cookie, &csrf).await;
    let project_b = create_project_named(&app, "Capacity Project B", &cookie, &csrf).await;
    let item = create_candidate(&app, &project_a, &cookie, &csrf, "Cross-scope capacity").await;
    let item_id = Uuid::parse_str(item["id"].as_str().unwrap()).unwrap();
    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{item_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let pool = state.db.as_ref().unwrap();
    let admin_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'admin@localhost'")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut config = state.config.knowledge.clone();
    config.retrieval.max_active_per_project = 1;
    let service = KnowledgeService::new(pool, &config);
    let edited = service
        .edit(
            item_id,
            2,
            admin_id,
            KnowledgeRevisionPatch {
                scope: Some(KnowledgeScope::Project),
                project_id: Some(Some(Uuid::parse_str(&project_b).unwrap())),
                content: Some("Replacement embedding has not completed.".into()),
                ..KnowledgeRevisionPatch::default()
            },
        )
        .await
        .unwrap();
    assert_ne!(edited.revision_id, edited.active_revision_id.unwrap());

    for project_id in [&project_a, &project_b] {
        let error = service
            .create_manual(
                admin_id,
                KnowledgeRevisionInput {
                    scope: KnowledgeScope::Project,
                    project_id: Some(Uuid::parse_str(project_id).unwrap()),
                    agent_id: None,
                    knowledge_type: KnowledgeType::TestCommand,
                    title: format!("Overflow {project_id}"),
                    content: "This project already has a reserved active slot.".into(),
                    source_type: KnowledgeSourceType::HumanNote,
                    source_id: None,
                    source_run_id: None,
                    confidence: KnowledgeConfidence::High,
                },
            )
            .await
            .expect_err("active and current scopes must both reserve capacity");
        assert!(matches!(error, KnowledgeError::Capacity(_)));
    }
}

#[tokio::test]
async fn activation_revalidates_capacity_before_replacing_the_active_revision() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let first = create_candidate(&app, &project_id, &cookie, &csrf, "Capacity occupant").await;
    let second = create_candidate(&app, &project_id, &cookie, &csrf, "Activation candidate").await;

    for candidate in [&first, &second] {
        let (status, _) = mutate(
            &app,
            "POST",
            &format!(
                "/api/knowledge/{}/approve",
                candidate["id"].as_str().unwrap()
            ),
            serde_json::json!({"expectedVersion": 1}),
            &cookie,
            &csrf,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let mut config = state.config.knowledge.clone();
    config.retrieval.max_active_per_project = 1;
    let second_item_id = Uuid::parse_str(second["id"].as_str().unwrap()).unwrap();
    let second_revision_id = Uuid::parse_str(second["revisionId"].as_str().unwrap()).unwrap();
    let mut tx = state.db.as_ref().unwrap().begin().await.unwrap();
    sqlx::query(
        r#"
        INSERT INTO knowledge_embeddings (
            revision_id, provider, model, embedding_dimension, embedding
        ) VALUES ($1, 'test', 'test-1536', 1536, $2::vector)
        "#,
    )
    .bind(second_revision_id)
    .bind(unit_vector_literal())
    .execute(&mut *tx)
    .await
    .unwrap();
    let error = activate_embedded_revision(&mut tx, second_item_id, second_revision_id, &config)
        .await
        .expect_err("activation must revalidate capacity under its transaction lock");
    assert!(matches!(error, KnowledgeError::Capacity(_)));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn supersession_waits_for_replacement_embedding() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let original = create_candidate(&app, &project_id, &cookie, &csrf, "Old rule").await;
    let original_id = original["id"].as_str().unwrap();
    let (_, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    process_one_knowledge_job(&state).await.unwrap();

    let replacement_body = serde_json::json!({
        "expectedVersion": 2,
        "replacement": {
            "scope": "project",
            "projectId": project_id,
            "knowledgeType": "test_command",
            "title": "New rule",
            "content": "Run make test-smoke.",
            "sourceType": "human_note",
            "confidence": "high"
        }
    });
    let (status, replacement) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        replacement_body,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let replacement_id = replacement["id"].as_str().unwrap();
    assert_eq!(replacement["status"], "pending");

    let (stale_status, stale_conflict) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        serde_json::json!({
            "expectedVersion": 2,
            "replacement": {
                "scope": "project",
                "projectId": project_id,
                "knowledgeType": "test_command",
                "title": "Conflicting rule",
                "content": "This stale supersession must not be created.",
                "sourceType": "human_note",
                "confidence": "high"
            }
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(stale_status, StatusCode::CONFLICT);
    assert!(stale_conflict["message"]
        .as_str()
        .unwrap()
        .contains("current version is 3"));

    let original_before: Option<Uuid> =
        sqlx::query_scalar("SELECT superseded_by FROM knowledge_items WHERE id = $1")
            .bind(Uuid::parse_str(original_id).unwrap())
            .fetch_one(state.db.as_ref().unwrap())
            .await
            .unwrap();
    assert!(original_before.is_none());

    mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{replacement_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    process_one_knowledge_job(&state).await.unwrap();

    let original_after: Option<Uuid> =
        sqlx::query_scalar("SELECT superseded_by FROM knowledge_items WHERE id = $1")
            .bind(Uuid::parse_str(original_id).unwrap())
            .fetch_one(state.db.as_ref().unwrap())
            .await
            .unwrap();
    assert_eq!(
        original_after,
        Some(Uuid::parse_str(replacement_id).unwrap())
    );
}

#[tokio::test]
async fn supersession_reapproval_activates_embedded_replacement_atomically() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let project_uuid = Uuid::parse_str(&project_id).unwrap();
    let original = create_candidate(
        &app,
        &project_id,
        &cookie,
        &csrf,
        "Original superseded rule",
    )
    .await;
    let original_id = original["id"].as_str().unwrap();
    let original_uuid = Uuid::parse_str(original_id).unwrap();

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let (status, replacement) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        serde_json::json!({
            "expectedVersion": 2,
            "replacement": {
                "scope": "project",
                "projectId": project_id,
                "knowledgeType": "test_command",
                "title": "Replacement superseding rule",
                "content": "Run make test-smoke before review.",
                "sourceType": "human_note",
                "confidence": "high"
            }
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let replacement_id = replacement["id"].as_str().unwrap();
    let replacement_uuid = Uuid::parse_str(replacement_id).unwrap();

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{replacement_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{replacement_id}/reject"),
        serde_json::json!({"expectedVersion": 2, "reason": "review again"}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    assert!(process_one_knowledge_job(&state).await.unwrap());
    let pool = state.db.as_ref().unwrap();
    let blocked: (Option<Uuid>, String, Option<Uuid>, bool) = sqlx::query_as(
        r#"
        SELECT original.superseded_by, replacement.status,
               replacement.active_revision_id,
               EXISTS (
                   SELECT 1 FROM knowledge_embeddings
                   WHERE revision_id = replacement.current_revision_id
               )
        FROM knowledge_items original
        JOIN knowledge_items replacement ON replacement.id = $2
        WHERE original.id = $1
        "#,
    )
    .bind(original_uuid)
    .bind(replacement_uuid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(blocked, (None, "rejected".into(), None, true));

    let provider: Arc<dyn EmbeddingProvider> =
        embedding_provider(&state.config.knowledge.embedding).unwrap();
    let query = provider
        .embed(&["Run make test-smoke before review.".into()])
        .await
        .unwrap();
    let mut retrieval_config = state.config.knowledge.retrieval.clone();
    retrieval_config.minimum_similarity = -1.0;
    retrieval_config.top_k = 20;
    let before_reapproval = retrieve(
        pool,
        project_uuid,
        Uuid::new_v4(),
        &query[0],
        &retrieval_config,
    )
    .await
    .unwrap();
    assert_eq!(before_reapproval.len(), 1);
    assert_eq!(before_reapproval[0].item_id, original_uuid);

    let (status, reapproved) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{replacement_id}/approve"),
        serde_json::json!({"expectedVersion": 3}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reapproved["status"], "approved");
    assert_eq!(reapproved["activeRevisionId"], reapproved["revisionId"]);

    let activated: (Option<Uuid>, String, bool) = sqlx::query_as(
        r#"
        SELECT original.superseded_by, replacement.status,
               replacement.active_revision_id = replacement.current_revision_id
        FROM knowledge_items original
        JOIN knowledge_items replacement ON replacement.id = $2
        WHERE original.id = $1
        "#,
    )
    .bind(original_uuid)
    .bind(replacement_uuid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(activated, (Some(replacement_uuid), "approved".into(), true));

    let after_reapproval = retrieve(
        pool,
        project_uuid,
        Uuid::new_v4(),
        &query[0],
        &retrieval_config,
    )
    .await
    .unwrap();
    assert_eq!(after_reapproval.len(), 1);
    assert_eq!(after_reapproval[0].item_id, replacement_uuid);
}

#[tokio::test]
async fn supersession_rejects_a_new_candidate_for_an_already_retired_original() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let original = create_candidate(
        &app,
        &project_id,
        &cookie,
        &csrf,
        "Permanently retired original",
    )
    .await;
    let original_id = original["id"].as_str().unwrap();
    let original_uuid = Uuid::parse_str(original_id).unwrap();

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let replacement_body = |expected_version, title: &str| {
        serde_json::json!({
            "expectedVersion": expected_version,
            "replacement": {
                "scope": "project",
                "projectId": project_id,
                "knowledgeType": "test_command",
                "title": title,
                "content": "An activated replacement permanently retires its original.",
                "sourceType": "human_note",
                "confidence": "high"
            }
        })
    };
    let (status, first) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        replacement_body(2, "Activated replacement"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first_id = first["id"].as_str().unwrap();
    let first_uuid = Uuid::parse_str(first_id).unwrap();

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{first_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{first_id}/reject"),
        serde_json::json!({"expectedVersion": 2, "reason": "retired replacement"}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let pool = state.db.as_ref().unwrap();
    let original_state: (i32, Option<Uuid>) =
        sqlx::query_as("SELECT version, superseded_by FROM knowledge_items WHERE id = $1")
            .bind(original_uuid)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(original_state, (4, Some(first_uuid)));

    let (status, conflict) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        replacement_body(i64::from(original_state.0), "Impossible second replacement"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(conflict["message"]
        .as_str()
        .unwrap()
        .contains("already been superseded"));

    let children: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM knowledge_items WHERE supersedes_item_id = $1")
            .bind(original_uuid)
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(children, vec![first_uuid]);
}

#[tokio::test]
async fn supersession_rejects_a_second_live_replacement_at_the_current_version() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let original = create_candidate(
        &app,
        &project_id,
        &cookie,
        &csrf,
        "Singular replacement rule",
    )
    .await;
    let original_id = original["id"].as_str().unwrap();
    let original_uuid = Uuid::parse_str(original_id).unwrap();

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let replacement_body = |expected_version, title: &str| {
        serde_json::json!({
            "expectedVersion": expected_version,
            "replacement": {
                "scope": "project",
                "projectId": project_id,
                "knowledgeType": "test_command",
                "title": title,
                "content": "Only one replacement may remain live.",
                "sourceType": "human_note",
                "confidence": "high"
            }
        })
    };
    let (status, first) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        replacement_body(2, "First live replacement"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first_id = first["id"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/knowledge/{original_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let current_original = common::json_body(response).await;
    let current_version = current_original["version"].as_i64().unwrap();
    assert_eq!(current_version, 3);

    let (status, conflict) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        replacement_body(current_version, "Competing live replacement"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(conflict["message"]
        .as_str()
        .unwrap()
        .contains("already has a live replacement"));

    let pool = state.db.as_ref().unwrap();
    let counts: (i64, i64) = sqlx::query_as(
        r#"
        SELECT count(*), count(*) FILTER (WHERE status IN ('pending', 'approved'))
        FROM knowledge_items
        WHERE supersedes_item_id = $1
        "#,
    )
    .bind(original_uuid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1));

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{first_id}/reject"),
        serde_json::json!({"expectedVersion": 1, "reason": "abandoned"}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, second) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        replacement_body(current_version, "Successor after abandonment"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_ne!(second["id"], first["id"]);

    let counts: (i64, i64) = sqlx::query_as(
        r#"
        SELECT count(*), count(*) FILTER (WHERE status IN ('pending', 'approved'))
        FROM knowledge_items
        WHERE supersedes_item_id = $1
        "#,
    )
    .bind(original_uuid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(counts, (2, 1));

    let direct_insert_error =
        sqlx::query("INSERT INTO knowledge_items (id, supersedes_item_id) VALUES ($1, $2)")
            .bind(Uuid::new_v4())
            .bind(original_uuid)
            .execute(pool)
            .await
            .expect_err("the database must reject a second live replacement");
    assert_eq!(
        direct_insert_error
            .as_database_error()
            .and_then(|error| error.constraint()),
        Some("knowledge_items_one_live_replacement_idx")
    );

    let (status, conflict) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{first_id}/approve"),
        serde_json::json!({"expectedVersion": 2}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(conflict["message"]
        .as_str()
        .unwrap()
        .contains("already has a live replacement"));
}

#[tokio::test]
async fn supersession_stale_never_activated_candidate_allows_a_successor() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let original = create_candidate(
        &app,
        &project_id,
        &cookie,
        &csrf,
        "Original with stale candidate",
    )
    .await;
    let original_id = original["id"].as_str().unwrap();
    let original_uuid = Uuid::parse_str(original_id).unwrap();

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let (status, first) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        supersede_body(
            &project_id,
            2,
            "Never activated stale candidate",
            "This candidate may be abandoned before activation.",
        ),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first_id = first["id"].as_str().unwrap();

    let (status, stale) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{first_id}/mark-stale"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stale["status"], "stale");
    assert!(stale["activeRevisionId"].is_null());

    let (status, successor) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        supersede_body(
            &project_id,
            3,
            "Successor after stale abandonment",
            "A stale never-activated candidate does not retire the original.",
        ),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_ne!(successor["id"], first["id"]);

    let states: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM knowledge_items WHERE supersedes_item_id = $1 ORDER BY status",
    )
    .bind(original_uuid)
    .fetch_all(state.db.as_ref().unwrap())
    .await
    .unwrap();
    assert_eq!(states, vec!["pending".to_string(), "stale".to_string()]);
}

#[tokio::test]
async fn supersession_concurrent_reapproval_and_successor_creation_do_not_deadlock() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let project_uuid = Uuid::parse_str(&project_id).unwrap();
    let original = create_candidate(
        &app,
        &project_id,
        &cookie,
        &csrf,
        "Original in supersession race",
    )
    .await;
    let original_id = original["id"].as_str().unwrap();
    let original_uuid = Uuid::parse_str(original_id).unwrap();

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let (status, abandoned) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        supersede_body(
            &project_id,
            2,
            "Rejected embedded candidate",
            "Reapproval races with creation of a successor.",
        ),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let abandoned_id = abandoned["id"].as_str().unwrap().to_string();
    let abandoned_uuid = Uuid::parse_str(&abandoned_id).unwrap();

    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{abandoned_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{abandoned_id}/reject"),
        serde_json::json!({"expectedVersion": 2, "reason": "abandoned before embedding"}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(process_one_knowledge_job(&state).await.unwrap());

    let approve_path = format!("/api/knowledge/{abandoned_id}/approve");
    let supersede_path = format!("/api/knowledge/{original_id}/supersede");
    let ((approve_status, _), (create_status, _)) =
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                mutate(
                    &app,
                    "POST",
                    &approve_path,
                    serde_json::json!({"expectedVersion": 3}),
                    &cookie,
                    &csrf,
                ),
                mutate(
                    &app,
                    "POST",
                    &supersede_path,
                    supersede_body(
                        &project_id,
                        3,
                        "Concurrent successor",
                        "Exactly one racing candidate may remain live.",
                    ),
                    &cookie,
                    &csrf,
                )
            )
        })
        .await
        .expect("concurrent supersession mutations must not deadlock");

    let reapproval_won = approve_status == StatusCode::OK && create_status == StatusCode::CONFLICT;
    let successor_won =
        approve_status == StatusCode::CONFLICT && create_status == StatusCode::CREATED;
    assert!(
        reapproval_won || successor_won,
        "expected one success and one 409, got approve={approve_status}, create={create_status}"
    );

    let pool = state.db.as_ref().unwrap();
    let live_children: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM knowledge_items
        WHERE supersedes_item_id = $1 AND status IN ('pending', 'approved')
        "#,
    )
    .bind(original_uuid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(live_children, 1);

    let original_link: Option<Uuid> =
        sqlx::query_scalar("SELECT superseded_by FROM knowledge_items WHERE id = $1")
            .bind(original_uuid)
            .fetch_one(pool)
            .await
            .unwrap();
    let abandoned_state: (String, Option<Uuid>) =
        sqlx::query_as("SELECT status, active_revision_id FROM knowledge_items WHERE id = $1")
            .bind(abandoned_uuid)
            .fetch_one(pool)
            .await
            .unwrap();

    let provider: Arc<dyn EmbeddingProvider> =
        embedding_provider(&state.config.knowledge.embedding).unwrap();
    let query = provider.embed(&["supersession race".into()]).await.unwrap();
    let mut retrieval_config = state.config.knowledge.retrieval.clone();
    retrieval_config.minimum_similarity = -1.0;
    retrieval_config.top_k = 20;
    let retrieved = retrieve(
        pool,
        project_uuid,
        Uuid::new_v4(),
        &query[0],
        &retrieval_config,
    )
    .await
    .unwrap();
    assert_eq!(retrieved.len(), 1);
    if reapproval_won {
        assert_eq!(original_link, Some(abandoned_uuid));
        assert_eq!(abandoned_state.0, "approved");
        assert!(abandoned_state.1.is_some());
        assert_eq!(retrieved[0].item_id, abandoned_uuid);
    } else {
        assert!(original_link.is_none());
        assert_eq!(abandoned_state, ("rejected".into(), None));
        assert_eq!(retrieved[0].item_id, original_uuid);
    }
}

#[tokio::test]
async fn supersession_migration_repairs_legacy_competing_children_deterministically() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, _app, _cookie, _csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("DROP INDEX knowledge_items_one_live_replacement_idx")
        .execute(&mut *tx)
        .await
        .unwrap();

    sqlx::raw_sql(
        r#"
        INSERT INTO knowledge_items (
            id, status, version, supersedes_item_id, superseded_by, created_at, updated_at
        ) VALUES
            ('10000000-0000-0000-0000-000000000001', 'approved', 5, NULL, NULL, now(), now()),
            ('10000000-0000-0000-0000-000000000002', 'approved', 5, NULL, '30000000-0000-0000-0000-000000000001', now(), now()),
            ('10000000-0000-0000-0000-000000000003', 'approved', 5, NULL, '40000000-0000-0000-0000-000000000001', now(), now()),
            ('20000000-0000-0000-0000-000000000001', 'approved', 7, '10000000-0000-0000-0000-000000000001', NULL, '2026-01-01', '2026-01-01'),
            ('20000000-0000-0000-0000-000000000002', 'approved', 7, '10000000-0000-0000-0000-000000000001', NULL, '2026-01-01', '2026-01-01'),
            ('20000000-0000-0000-0000-000000000003', 'pending', 7, '10000000-0000-0000-0000-000000000001', NULL, '2025-01-01', '2025-01-01'),
            ('30000000-0000-0000-0000-000000000001', 'rejected', 7, '10000000-0000-0000-0000-000000000002', NULL, '2025-01-01', '2025-01-01'),
            ('30000000-0000-0000-0000-000000000002', 'approved', 7, '10000000-0000-0000-0000-000000000002', NULL, '2026-01-01', '2026-01-01'),
            ('30000000-0000-0000-0000-000000000003', 'pending', 7, '10000000-0000-0000-0000-000000000002', NULL, '2026-01-02', '2026-01-02'),
            ('40000000-0000-0000-0000-000000000001', 'pending', 7, '10000000-0000-0000-0000-000000000003', NULL, '2026-01-02', '2026-01-02'),
            ('40000000-0000-0000-0000-000000000002', 'approved', 7, '10000000-0000-0000-0000-000000000003', NULL, '2026-01-01', '2026-01-01');

        INSERT INTO knowledge_revisions (
            id, item_id, revision_number, scope, knowledge_type, title, content,
            source_type, confidence
        ) VALUES
            ('50000000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000001', 1, 'workspace', 'test_command', 'winner', 'winner', 'human_note', 'high'),
            ('50000000-0000-0000-0000-000000000002', '20000000-0000-0000-0000-000000000002', 1, 'workspace', 'test_command', 'competitor', 'competitor', 'human_note', 'high'),
            ('50000000-0000-0000-0000-000000000003', '40000000-0000-0000-0000-000000000002', 1, 'workspace', 'test_command', 'linked competitor', 'linked competitor', 'human_note', 'high');

        UPDATE knowledge_items i
        SET current_revision_id = r.id, active_revision_id = r.id
        FROM knowledge_revisions r
        WHERE r.item_id = i.id;
        "#,
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!(
        "../migrations/014_singular_knowledge_supersession.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();

    let original_one: (Option<Uuid>, i32) = sqlx::query_as(
        "SELECT superseded_by, version FROM knowledge_items WHERE id = '10000000-0000-0000-0000-000000000001'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        original_one,
        (
            Some(Uuid::parse_str("20000000-0000-0000-0000-000000000001").unwrap()),
            6,
        )
    );

    let group_one: Vec<(Uuid, String, i32, Option<Uuid>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT id, status, version, active_revision_id, rejection_reason
        FROM knowledge_items
        WHERE supersedes_item_id = '10000000-0000-0000-0000-000000000001'
        ORDER BY id
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(group_one[0].1, "approved");
    assert_eq!(group_one[0].2, 7);
    assert!(group_one[0].3.is_some());
    for row in &group_one[1..] {
        assert_eq!(row.1, "rejected");
        assert_eq!(row.2, 8);
        assert!(row.3.is_none());
        assert_eq!(
            row.4.as_deref(),
            Some("migration 014: competing live supersession candidate quarantined")
        );
    }

    let group_two: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, status FROM knowledge_items
        WHERE supersedes_item_id = '10000000-0000-0000-0000-000000000002'
        ORDER BY id
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(group_two[0].1, "rejected");
    assert_eq!(group_two[1].1, "rejected");
    assert_eq!(group_two[2].1, "rejected");

    let group_three: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, status FROM knowledge_items
        WHERE supersedes_item_id = '10000000-0000-0000-0000-000000000003'
        ORDER BY id
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(group_three[0].1, "pending");
    assert_eq!(group_three[1].1, "rejected");

    let duplicate_error = sqlx::query(
        r#"
        INSERT INTO knowledge_items (id, supersedes_item_id)
        VALUES ('60000000-0000-0000-0000-000000000001',
                '10000000-0000-0000-0000-000000000001')
        "#,
    )
    .execute(&mut *tx)
    .await
    .expect_err("migration must install the singular live replacement index");
    assert_eq!(
        duplicate_error
            .as_database_error()
            .and_then(|error| error.constraint()),
        Some("knowledge_items_one_live_replacement_idx")
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn retrieval_is_scoped_and_usage_is_logged_once() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let other_project_id =
        create_project_named(&app, "Other Knowledge Project", &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Knowledge Worker",
        &cookie,
        &csrf,
    )
    .await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let item = create_candidate(&app, &project_id, &cookie, &csrf, "Use smoke tests").await;
    let item_id = item["id"].as_str().unwrap();
    mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{item_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    process_one_knowledge_job(&state).await.unwrap();

    let provider: Arc<dyn EmbeddingProvider> =
        embedding_provider(&state.config.knowledge.embedding).unwrap();
    let query = provider.embed(&["Run tests".into()]).await.unwrap();
    let found = retrieve(
        state.db.as_ref().unwrap(),
        Uuid::parse_str(&project_id).unwrap(),
        Uuid::parse_str(&agent_id).unwrap(),
        &query[0],
        &state.config.knowledge.retrieval,
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);
    let wrong_project = retrieve(
        state.db.as_ref().unwrap(),
        Uuid::parse_str(&other_project_id).unwrap(),
        Uuid::parse_str(&agent_id).unwrap(),
        &query[0],
        &state.config.knowledge.retrieval,
    )
    .await
    .unwrap();
    assert!(wrong_project.is_empty());

    let mut type_filtered = state.config.knowledge.retrieval.clone();
    type_filtered.allowed_types = vec!["bug_pattern".into()];
    assert!(!has_eligible(
        state.db.as_ref().unwrap(),
        Uuid::parse_str(&project_id).unwrap(),
        Uuid::parse_str(&agent_id).unwrap(),
        &type_filtered,
    )
    .await
    .unwrap());
    let excluded_by_type = retrieve(
        state.db.as_ref().unwrap(),
        Uuid::parse_str(&project_id).unwrap(),
        Uuid::parse_str(&agent_id).unwrap(),
        &query[0],
        &type_filtered,
    )
    .await
    .unwrap();
    assert!(excluded_by_type.is_empty());

    type_filtered.allowed_types = vec!["test_command".into()];
    assert!(has_eligible(
        state.db.as_ref().unwrap(),
        Uuid::parse_str(&project_id).unwrap(),
        Uuid::parse_str(&agent_id).unwrap(),
        &type_filtered,
    )
    .await
    .unwrap());

    let run_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id
        ) VALUES ($1, $2, $3, 'work_on_ticket', 'succeeded', 'permissive-default')
        "#,
    )
    .bind(run_id)
    .bind(Uuid::parse_str(&ticket_id).unwrap())
    .bind(Uuid::parse_str(&agent_id).unwrap())
    .execute(state.db.as_ref().unwrap())
    .await
    .unwrap();
    let section = render_knowledge(&found, 4_000, &ByteTokenCounter);
    assert_eq!(section.entries.len(), 1);
    record_usage(state.db.as_ref().unwrap(), run_id, &section.entries)
        .await
        .unwrap();
    record_usage(state.db.as_ref().unwrap(), run_id, &section.entries)
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/agent-runs/{run_id}/knowledge-used"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let usage = common::json_body(response).await;
    assert_eq!(usage["items"].as_array().unwrap().len(), 1);
    assert_eq!(usage["items"][0]["itemId"], item_id);
    assert!(usage["items"][0]["renderedContent"]
        .as_str()
        .unwrap()
        .contains("UNTRUSTED KNOWLEDGE"));
}

#[tokio::test]
async fn retrieval_excludes_every_ineligible_lifecycle_and_scope_variant() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id =
        Uuid::parse_str(&common::create_test_project(&app, &cookie, &csrf).await).unwrap();
    let other_project_id = Uuid::parse_str(
        &create_project_named(&app, "Retrieval Matrix Other", &cookie, &csrf).await,
    )
    .unwrap();
    let agent_id = Uuid::parse_str(
        &common::create_agent_with_preset_key(
            &app,
            "backend_engineer",
            "Retrieval Matrix Agent",
            &cookie,
            &csrf,
        )
        .await,
    )
    .unwrap();
    let other_agent_id = Uuid::parse_str(
        &common::create_agent_with_preset_key(
            &app,
            "frontend_engineer",
            "Retrieval Matrix Other Agent",
            &cookie,
            &csrf,
        )
        .await,
    )
    .unwrap();
    let pool = state.db.as_ref().unwrap();
    let base = RetrievalSeed {
        label: "valid project",
        status: "approved",
        scope: "project",
        project_id: Some(project_id),
        agent_id: None,
        confidence: "high",
        expired: false,
        activate: true,
        store_embedding: true,
    };

    let valid_project = seed_retrieval_item(pool, base).await.0;
    let valid_workspace = seed_retrieval_item(
        pool,
        RetrievalSeed {
            label: "valid workspace",
            scope: "workspace",
            project_id: None,
            ..base
        },
    )
    .await
    .0;
    let valid_agent = seed_retrieval_item(
        pool,
        RetrievalSeed {
            label: "valid agent",
            scope: "agent",
            agent_id: Some(agent_id),
            ..base
        },
    )
    .await
    .0;

    for seed in [
        RetrievalSeed {
            label: "rejected",
            status: "rejected",
            ..base
        },
        RetrievalSeed {
            label: "stale",
            status: "stale",
            ..base
        },
        RetrievalSeed {
            label: "expired",
            expired: true,
            ..base
        },
        RetrievalSeed {
            label: "low confidence",
            confidence: "low",
            ..base
        },
        RetrievalSeed {
            label: "wrong project",
            project_id: Some(other_project_id),
            ..base
        },
        RetrievalSeed {
            label: "wrong agent",
            scope: "agent",
            agent_id: Some(other_agent_id),
            ..base
        },
        RetrievalSeed {
            label: "missing active revision",
            activate: false,
            ..base
        },
        RetrievalSeed {
            label: "missing embedding",
            store_embedding: false,
            ..base
        },
    ] {
        seed_retrieval_item(pool, seed).await;
    }

    let (superseded_id, _) = seed_retrieval_item(
        pool,
        RetrievalSeed {
            label: "superseded",
            ..base
        },
    )
    .await;
    let replacement_id = Uuid::new_v4();
    sqlx::query("INSERT INTO knowledge_items (id, status, version) VALUES ($1, 'pending', 1)")
        .bind(replacement_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE knowledge_items SET superseded_by = $2 WHERE id = $1")
        .bind(superseded_id)
        .bind(replacement_id)
        .execute(pool)
        .await
        .unwrap();

    let found = retrieve(
        pool,
        project_id,
        agent_id,
        &std::iter::once(1.0)
            .chain(std::iter::repeat_n(0.0, 1_535))
            .collect::<Vec<_>>(),
        &state.config.knowledge.retrieval,
    )
    .await
    .unwrap();
    let found_ids = found
        .into_iter()
        .map(|item| item.item_id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        found_ids,
        [valid_project, valid_workspace, valid_agent]
            .into_iter()
            .collect()
    );
}

#[tokio::test]
async fn done_transition_schedules_idempotent_extraction_to_pending() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_uuid)
        .execute(state.db.as_ref().unwrap())
        .await
        .unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_uuid)
        .execute(state.db.as_ref().unwrap())
        .await
        .unwrap();
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM knowledge_jobs WHERE kind = 'extract_ticket' AND ticket_id = $1",
    )
    .bind(ticket_uuid)
    .fetch_one(state.db.as_ref().unwrap())
    .await
    .unwrap();
    assert_eq!(jobs, 1);

    process_one_knowledge_job(&state).await.unwrap();
    let response = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/knowledge/inbox?projectId={project_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let inbox = common::json_body(response).await;
    assert_eq!(inbox["items"].as_array().unwrap().len(), 1);
    assert_eq!(inbox["items"][0]["sourceId"], ticket_id);
    assert_eq!(inbox["items"][0]["policyDecision"], "human_review");
}

#[tokio::test]
async fn extraction_preserves_typed_comment_and_review_source_ids() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_id = Uuid::parse_str(&ticket_id).unwrap();
    let comment_id = Uuid::new_v4();
    let review_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO ticket_comments (id, ticket_id, author_type, body, intent, created_at)
        VALUES
            ($1, $3, 'human', 'Ordinary diagnostic comment.', 'progress_update', now() - interval '1 second'),
            ($2, $3, 'agent', 'Review-specific correction.', 'review_feedback', now())
        "#,
    )
    .bind(comment_id)
    .bind(review_id)
    .bind(ticket_id)
    .execute(state.db.as_ref().unwrap())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO knowledge_jobs (id, kind, ticket_id) VALUES ($1, 'extract_ticket', $2)",
    )
    .bind(Uuid::new_v4())
    .bind(ticket_id)
    .execute(state.db.as_ref().unwrap())
    .await
    .unwrap();

    let embedder = embedding_provider(&state.config.knowledge.embedding).unwrap();
    let extractor: Arc<dyn ExtractionProvider> = Arc::new(CommentReviewExtractionProvider);
    assert!(
        knowledge_worker::process_one(&state, "source-aware", &embedder, &extractor)
            .await
            .unwrap()
    );

    let rows = sqlx::query(
        r#"
        SELECT source_type, source_id
        FROM knowledge_revisions
        WHERE source_id IN ($1, $2)
        ORDER BY source_type
        "#,
    )
    .bind(comment_id)
    .bind(review_id)
    .fetch_all(state.db.as_ref().unwrap())
    .await
    .unwrap();
    let sources = rows
        .iter()
        .map(|row| {
            (
                sqlx::Row::get::<String, _>(row, "source_type"),
                sqlx::Row::get::<Uuid, _>(row, "source_id"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sources,
        vec![("comment".into(), comment_id), ("review".into(), review_id)]
    );
}

#[tokio::test]
async fn extraction_byte_budget_prioritizes_the_newest_comments() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_id = Uuid::parse_str(&ticket_id).unwrap();
    let oldest_comment_id = Uuid::new_v4();
    let newest_comment_id = Uuid::new_v4();
    let pool = state.db.as_ref().unwrap();
    sqlx::query(
        r#"
        INSERT INTO ticket_comments (id, ticket_id, author_type, body, intent, created_at)
        VALUES
            ($1, $3, 'human', $4, 'progress_update', now() - interval '1 second'),
            ($2, $3, 'human', 'newest durable evidence', 'review_feedback', now())
        "#,
    )
    .bind(oldest_comment_id)
    .bind(newest_comment_id)
    .bind(ticket_id)
    .bind("old source material ".repeat(20))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_id)
        .execute(pool)
        .await
        .unwrap();

    let max_source_bytes = 128;
    let mut bounded_state = state.as_ref().clone();
    bounded_state.config.knowledge.extraction.max_source_bytes = max_source_bytes;
    let embedder = embedding_provider(&bounded_state.config.knowledge.embedding).unwrap();
    let extractor: Arc<dyn ExtractionProvider> = Arc::new(OrderedCommentExtractionProvider {
        expected_comment_ids: [oldest_comment_id, newest_comment_id],
        max_source_bytes,
    });
    assert!(
        knowledge_worker::process_one(
            &bounded_state,
            "latest-source-worker",
            &embedder,
            &extractor,
        )
        .await
        .unwrap()
    );
}

#[tokio::test]
async fn extraction_byte_budget_includes_title_and_description() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_id = Uuid::parse_str(&ticket_id).unwrap();
    let pool = state.db.as_ref().unwrap();
    sqlx::query("UPDATE tickets SET title = $2, description = $3 WHERE id = $1")
        .bind(ticket_id)
        .bind("long title ".repeat(40))
        .bind("long description ".repeat(40))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_id)
        .execute(pool)
        .await
        .unwrap();

    let max_source_bytes = 64;
    let mut bounded_state = state.as_ref().clone();
    bounded_state.config.knowledge.extraction.max_source_bytes = max_source_bytes;
    let embedder = embedding_provider(&bounded_state.config.knowledge.embedding).unwrap();
    let extractor: Arc<dyn ExtractionProvider> =
        Arc::new(BoundedExtractionProvider { max_source_bytes });
    assert!(
        knowledge_worker::process_one(
            &bounded_state,
            "bounded-source-worker",
            &embedder,
            &extractor,
        )
        .await
        .unwrap()
    );
}

#[tokio::test]
async fn knowledge_query_plan_has_relational_indexes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let target_project_id =
        Uuid::parse_str(&create_project_named(&app, "Retrieval target", &cookie, &csrf).await)
            .unwrap();
    let other_project_id =
        Uuid::parse_str(&create_project_named(&app, "Retrieval noise", &cookie, &csrf).await)
            .unwrap();
    let pool = state.db.as_ref().unwrap();
    let mut tx = pool.begin().await.unwrap();
    seed_retrieval_cardinality(&mut tx, target_project_id, other_project_id, 32, 512, 4_096).await;

    let explain = explain_production_retrieval(&mut tx, target_project_id, true).await;
    let plan = &explain[0]["Plan"];
    let mut index_scans = Vec::new();
    collect_index_scan_names(plan, &mut index_scans);
    assert!(
        index_scans
            .iter()
            .any(|name| name == "knowledge_items_retrieval_state_idx"),
        "expected the approved-state eligibility index in {index_scans:?}\n{explain:#}"
    );
    assert!(
        index_scans.iter().any(|name| {
            name == "knowledge_revisions_project_scope_idx" || name == "knowledge_revisions_pkey"
        }),
        "expected a relational revision index scan in {index_scans:?}\n{explain:#}"
    );

    let serialized_plan = plan.to_string();
    let eligible_producer = find_plan_node(plan, "Subplan Name", "CTE eligible")
        .expect("materialized eligible CTE producer");
    assert!(
        !eligible_producer.to_string().contains("<=>"),
        "vector distance must not run inside relational eligibility"
    );
    assert!(serialized_plan.contains("<=>"));
    assert!(find_plan_node(plan, "CTE Name", "eligible").is_some());
    assert!(
        !serialized_plan.contains("knowledge_embeddings_hnsw_cosine_idx"),
        "production plan must not rank globally before eligibility"
    );
    tx.rollback().await.unwrap();

    coppice_server::knowledge::validate_schema_dimension(pool, 1536)
        .await
        .unwrap();
    let mismatch = coppice_server::knowledge::validate_schema_dimension(pool, 512)
        .await
        .expect_err("configured dimension mismatch must stop startup");
    assert!(mismatch.to_string().contains("vector(1536)"));
}

#[tokio::test]
#[ignore = "run with make benchmark-m06-knowledge-retrieval on the default Compose stack"]
async fn knowledge_retrieval_capacity_p95_benchmark() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let database_url = std::env::var("COPPICE_RETRIEVAL_BENCHMARK_DATABASE_URL")
        .expect("benchmark target must provide its isolated default-Compose database URL");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, name, slug) VALUES ($1, $2, $3)")
        .bind(project_id)
        .bind("M06 retrieval capacity benchmark")
        .bind(format!("m06-retrieval-benchmark-{project_id}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    seed_retrieval_cardinality(&mut tx, project_id, project_id, 10_000, 0, 0).await;

    let _warmup = explain_production_retrieval(&mut tx, project_id, true).await;
    let mut timings_ms = Vec::with_capacity(20);
    for _ in 0..20 {
        let explain = explain_production_retrieval(&mut tx, project_id, true).await;
        timings_ms.push(
            explain[0]["Execution Time"]
                .as_f64()
                .expect("Postgres execution time"),
        );
    }
    timings_ms.sort_by(f64::total_cmp);
    let p95_index = (timings_ms.len() * 95).div_ceil(100) - 1;
    let p95_ms = timings_ms[p95_index];
    eprintln!("M06 retrieval timings (ms): {timings_ms:?}; p95={p95_ms:.3}ms");
    assert!(
        p95_ms < 250.0,
        "10,000-row retrieval p95 {p95_ms:.3}ms exceeded the 250ms target"
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn reclaimed_embedding_claim_cannot_persist_embedding_or_activation() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let item = create_candidate(
        &app,
        &project_id,
        &cookie,
        &csrf,
        "Fence stale embedding writes",
    )
    .await;
    let item_id = item["id"].as_str().unwrap();
    let revision_id = Uuid::parse_str(item["revisionId"].as_str().unwrap()).unwrap();
    let (status, _) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{item_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let pool = state.db.as_ref().unwrap().clone();
    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(ReclaimingEmbeddingProvider {
        pool: pool.clone(),
        revision_id,
    });
    let extractor: Arc<dyn ExtractionProvider> = Arc::new(MockExtractionProvider);
    let result =
        knowledge_worker::process_one(&state, "stale-embedding-worker", &embedder, &extractor)
            .await;
    assert!(result.is_err(), "a worker that lost its claim must fail");

    let embedding_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM knowledge_embeddings WHERE revision_id = $1")
            .bind(revision_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let active_revision_id: Option<Uuid> =
        sqlx::query_scalar("SELECT active_revision_id FROM knowledge_items WHERE id = $1")
            .bind(Uuid::parse_str(item_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(embedding_count, 0);
    assert_eq!(active_revision_id, None);
}

#[tokio::test]
async fn reclaimed_extraction_claim_cannot_persist_candidates() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_id = Uuid::parse_str(&ticket_id).unwrap();
    let pool = state.db.as_ref().unwrap().clone();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_id)
        .execute(&pool)
        .await
        .unwrap();

    let embedder = embedding_provider(&state.config.knowledge.embedding).unwrap();
    let extractor: Arc<dyn ExtractionProvider> = Arc::new(ReclaimingExtractionProvider {
        pool: pool.clone(),
        ticket_id,
    });
    let result =
        knowledge_worker::process_one(&state, "stale-extraction-worker", &embedder, &extractor)
            .await;
    assert!(result.is_err(), "a worker that lost its claim must fail");

    let candidate_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM knowledge_items WHERE extraction_job_id = (SELECT id FROM knowledge_jobs WHERE ticket_id = $1)",
    )
    .bind(ticket_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(candidate_count, 0);
}

#[tokio::test]
async fn stale_knowledge_worker_cannot_overwrite_new_owner_state() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let pool = state.db.as_ref().unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_uuid)
        .execute(pool)
        .await
        .unwrap();

    let service = KnowledgeJobService::new(pool);
    let worker_id = "knowledge-worker-0";
    let stale_claim = service
        .claim_next(worker_id, 300)
        .await
        .unwrap()
        .expect("extraction job");
    sqlx::query(
        "UPDATE knowledge_jobs SET locked_at = now() - interval '301 seconds' WHERE id = $1",
    )
    .bind(stale_claim.id)
    .execute(pool)
    .await
    .unwrap();

    let fresh_claim = service
        .claim_next(worker_id, 300)
        .await
        .unwrap()
        .expect("reclaimed extraction job");
    assert_eq!(fresh_claim.id, stale_claim.id);
    assert_eq!(fresh_claim.locked_by, worker_id);
    assert_eq!(stale_claim.locked_by, worker_id);
    assert_ne!(fresh_claim.claim_token, stale_claim.claim_token);

    let expected_running = (
        "running".to_string(),
        Some(worker_id.to_string()),
        Some(fresh_claim.claim_token),
    );
    let mut terminal_stale_claim = stale_claim.clone();
    terminal_stale_claim.max_attempts = terminal_stale_claim.attempts;
    let terminal_error = service
        .mark_error(&terminal_stale_claim, "late terminal failure")
        .await
        .expect_err("stale terminal failure must report claim loss");
    assert!(terminal_error.to_string().contains("claim"));
    let after_terminal_error: (String, Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT status, locked_by, claim_token FROM knowledge_jobs WHERE id = $1")
            .bind(stale_claim.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(after_terminal_error, expected_running);

    let retry_error = service
        .mark_error(&stale_claim, "late retryable failure")
        .await
        .expect_err("stale retry must report claim loss");
    assert!(retry_error.to_string().contains("claim"));
    let after_retryable_error: (String, Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT status, locked_by, claim_token FROM knowledge_jobs WHERE id = $1")
            .bind(stale_claim.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(after_retryable_error, expected_running);

    let completion_error = service
        .mark_completed(&stale_claim)
        .await
        .expect_err("stale completion must report claim loss");
    assert!(completion_error.to_string().contains("claim"));
    let after_stale_completion: (String, Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT status, locked_by, claim_token FROM knowledge_jobs WHERE id = $1")
            .bind(stale_claim.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(after_stale_completion, expected_running);

    service.mark_completed(&fresh_claim).await.unwrap();
    let completed: (String, Option<String>, Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT status, locked_by, claim_token, last_error FROM knowledge_jobs WHERE id = $1",
    )
    .bind(fresh_claim.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(completed, ("completed".into(), None, None, None));
}

#[tokio::test]
async fn current_knowledge_claim_marks_job_failed_at_max_attempts() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let pool = state.db.as_ref().unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_uuid)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE knowledge_jobs SET max_attempts = 1 WHERE ticket_id = $1")
        .bind(ticket_uuid)
        .execute(pool)
        .await
        .unwrap();

    let service = KnowledgeJobService::new(pool);
    let claim = service
        .claim_next("knowledge-worker-0", 300)
        .await
        .unwrap()
        .expect("extraction job");
    assert_eq!(claim.attempts, claim.max_attempts);
    service
        .mark_error(&claim, "terminal extraction failure")
        .await
        .unwrap();

    let failed: (String, Option<String>, Option<String>, Option<Uuid>) = sqlx::query_as(
        "SELECT status, last_error, locked_by, claim_token FROM knowledge_jobs WHERE id = $1",
    )
    .bind(claim.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        failed,
        (
            "failed".into(),
            Some("terminal extraction failure".into()),
            None,
            None,
        )
    );
}

#[tokio::test]
async fn stale_max_attempt_knowledge_claim_is_failed_without_reexecution() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let pool = state.db.as_ref().unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_uuid)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE knowledge_jobs SET max_attempts = 1 WHERE ticket_id = $1")
        .bind(ticket_uuid)
        .execute(pool)
        .await
        .unwrap();

    let service = KnowledgeJobService::new(pool);
    let claim = service
        .claim_next("knowledge-worker-0", 300)
        .await
        .unwrap()
        .expect("first extraction claim");
    assert_eq!(claim.attempts, claim.max_attempts);
    sqlx::query(
        "UPDATE knowledge_jobs SET locked_at = now() - interval '301 seconds' WHERE id = $1",
    )
    .bind(claim.id)
    .execute(pool)
    .await
    .unwrap();

    let reclaimed = service.claim_next("knowledge-worker-1", 300).await.unwrap();
    assert!(
        reclaimed.is_none(),
        "exhausted stale claim must not run again"
    );
    let failed: (String, i32, Option<String>, Option<Uuid>) = sqlx::query_as(
        "SELECT status, attempts, locked_by, claim_token FROM knowledge_jobs WHERE id = $1",
    )
    .bind(claim.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(failed, ("failed".into(), 1, None, None));
}
