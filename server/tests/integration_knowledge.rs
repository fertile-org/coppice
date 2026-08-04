mod common;

use axum::{http::StatusCode, Router};
use coppice_server::knowledge::extractor::{ExtractionProvider, MockExtractionProvider};
use coppice_server::knowledge::retrieval::{retrieve, RETRIEVAL_QUERY_SQL};
use coppice_server::knowledge::{embedder::EmbeddingProvider, embedding_provider};
use coppice_server::services::context_budget::{record_usage, render_knowledge, ByteTokenCounter};
use coppice_server::services::knowledge_job_service::KnowledgeJobService;
use coppice_server::services::knowledge_service::{
    KnowledgeError, KnowledgeRevisionPatch, KnowledgeService,
};
use coppice_server::workers::knowledge_worker;
use coppice_server::AppState;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
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

fn unit_vector_literal() -> String {
    format!("[1{}]", ",0".repeat(1_535))
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
    service
        .mark_error(&terminal_stale_claim, "late terminal failure")
        .await
        .unwrap();
    let after_terminal_error: (String, Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT status, locked_by, claim_token FROM knowledge_jobs WHERE id = $1")
            .bind(stale_claim.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(after_terminal_error, expected_running);

    service
        .mark_error(&stale_claim, "late retryable failure")
        .await
        .unwrap();
    let after_retryable_error: (String, Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT status, locked_by, claim_token FROM knowledge_jobs WHERE id = $1")
            .bind(stale_claim.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(after_retryable_error, expected_running);

    service.mark_completed(&stale_claim).await.unwrap();
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
