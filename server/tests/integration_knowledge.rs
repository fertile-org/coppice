mod common;

use axum::{http::StatusCode, Router};
use coppice_server::knowledge::extractor::{ExtractionProvider, MockExtractionProvider};
use coppice_server::knowledge::retrieval::retrieve;
use coppice_server::knowledge::{embedder::EmbeddingProvider, embedding_provider};
use coppice_server::services::context_budget::{record_usage, render_knowledge, ByteTokenCounter};
use coppice_server::workers::knowledge_worker;
use coppice_server::AppState;
use std::sync::Arc;
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

async fn process_one_knowledge_job(state: &Arc<AppState>) -> anyhow::Result<bool> {
    let embedder = embedding_provider(&state.config.knowledge.embedding)?;
    let extractor: Arc<dyn ExtractionProvider> = Arc::new(MockExtractionProvider);
    knowledge_worker::process_one(state, "integration-knowledge", &embedder, &extractor).await
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
    let (state, _app, _cookie, _csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();
    let indexes: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT indexname FROM pg_indexes
        WHERE schemaname = current_schema()
          AND indexname IN (
            'knowledge_items_retrieval_state_idx',
            'knowledge_revisions_project_scope_idx',
            'knowledge_revisions_agent_scope_idx'
          )
        ORDER BY indexname
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(indexes.len(), 3);

    let plan: Vec<String> = sqlx::query_scalar(
        r#"
        EXPLAIN (COSTS OFF)
        WITH eligible AS MATERIALIZED (
            SELECT i.id, r.project_id
            FROM knowledge_items i
            JOIN knowledge_revisions r ON r.id = i.active_revision_id
            WHERE i.status = 'approved' AND i.superseded_by IS NULL
        )
        SELECT * FROM eligible ORDER BY id LIMIT 20
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(plan.join("\n").contains("eligible"));
}
