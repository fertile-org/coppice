mod common;

use axum::{http::StatusCode, Router};
use coppice_migrations::MIGRATOR;
use coppice_server::domain::knowledge::{
    KnowledgeConfidence, KnowledgeRevisionInput, KnowledgeScope, KnowledgeSourceType, KnowledgeType,
};
use coppice_server::knowledge::retrieval::{retrieve, RetrievalScope, RETRIEVAL_QUERY_SQL};
use coppice_server::services::context_budget::{record_usage, render_knowledge, ByteTokenCounter};
use coppice_server::services::knowledge_service::{
    activate_revision, KnowledgeError, KnowledgeListFilter, KnowledgeRevisionPatch,
    KnowledgeService,
};
use coppice_server::AppState;
use serde_json::Value;
use sqlx::migrate::Migrator;
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

async fn create_candidate(
    app: &Router,
    board_id: &str,
    cookie: &str,
    csrf: &str,
    title: &str,
) -> serde_json::Value {
    let body = serde_json::json!({
        "scope": "board",
        "boardId": board_id,
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

async fn create_board_named(app: &Router, name: &str, cookie: &str, csrf: &str) -> String {
    let response = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/boards",
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
    board_id: &str,
    expected_version: i64,
    title: &str,
    content: &str,
) -> serde_json::Value {
    serde_json::json!({
        "expectedVersion": expected_version,
        "replacement": {
            "scope": "board",
            "boardId": board_id,
            "knowledgeType": "test_command",
            "title": title,
            "content": content,
            "sourceType": "human_note",
            "confidence": "high"
        }
    })
}

#[derive(Clone, Copy)]
struct RetrievalSeed<'a> {
    label: &'a str,
    status: &'a str,
    scope: &'a str,
    board_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    confidence: &'a str,
    expired: bool,
    activate: bool,
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
            id, item_id, revision_number, scope, board_id, agent_id,
            knowledge_type, title, content, source_type, confidence
        ) VALUES ($1, $2, 1, $3, $4, $5, 'test_command', $6, $7, 'human_note', $8)
        "#,
    )
    .bind(revision_id)
    .bind(item_id)
    .bind(seed.scope)
    .bind(seed.board_id)
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
    (item_id, revision_id)
}

async fn seed_retrieval_cardinality(
    tx: &mut Transaction<'_, Postgres>,
    target_board_id: Uuid,
    other_board_id: Uuid,
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
            board_id UUID NOT NULL
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
            ordinal, item_id, revision_id, status, board_id
        )
        SELECT ordinal,
               gen_random_uuid(),
               gen_random_uuid(),
               CASE WHEN ordinal <= $4 THEN 'approved' ELSE 'rejected' END,
               CASE WHEN ordinal <= $3 THEN $1 ELSE $2 END
        FROM generate_series(1, $5) AS ordinal
        "#,
    )
    .bind(target_board_id)
    .bind(other_board_id)
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
            id, item_id, revision_number, scope, board_id, knowledge_type,
            title, content, source_type, confidence
        )
        SELECT revision_id, item_id, 1, 'board', board_id, 'test_command',
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
    sqlx::query("ANALYZE knowledge_items, knowledge_revisions")
        .execute(&mut **tx)
        .await
        .unwrap();
}

async fn explain_production_retrieval(
    tx: &mut Transaction<'_, Postgres>,
    board_id: Uuid,
    analyze: bool,
) -> Value {
    let options = if analyze {
        "ANALYZE, BUFFERS, VERBOSE, FORMAT JSON"
    } else {
        "VERBOSE, FORMAT JSON"
    };
    let sql = format!("EXPLAIN ({options}) {RETRIEVAL_QUERY_SQL}");
    sqlx::query_scalar(&sql)
        .bind(vec![board_id])
        .bind(Uuid::new_v4())
        .bind("low")
        .bind(vec!["retrieval".to_string(), "plan".to_string()])
        .bind(Vec::<String>::new())
        .bind(200_i64)
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
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let created = create_candidate(&app, &board_id, &cookie, &csrf, "Unit test command").await;
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
    assert_eq!(approved["activeRevisionId"], approved["revisionId"]);

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
    let old_active = approved["activeRevisionId"].as_str().unwrap().to_string();

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
    assert_eq!(edited["activeRevisionId"], edited["revisionId"]);

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
}

#[tokio::test]
async fn edit_distinguishes_omitted_and_explicitly_null_scope_ids() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
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
            "boardId": board_id,
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
    assert_eq!(preserved["boardId"], board_id);
    assert_eq!(preserved["agentId"], agent_id);

    let (status, cleared) = mutate(
        &app,
        "PATCH",
        &format!("/api/knowledge/{item_id}"),
        serde_json::json!({
            "expectedVersion": 2,
            "scope": "workspace",
            "boardId": null,
            "agentId": null
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["scope"], "workspace");
    assert!(cleared["boardId"].is_null());
    assert!(cleared["agentId"].is_null());
}

#[tokio::test]
async fn knowledge_list_has_stable_hard_limited_pages_and_auth_failures() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    for title in ["Pagination one", "Pagination two", "Pagination three"] {
        create_candidate(&app, &board_id, &cookie, &csrf, title).await;
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
            board_id: None,
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
            board_id: None,
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
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let live = create_candidate(&app, &board_id, &cookie, &csrf, "Live candidate").await;
    let terminal = create_candidate(
        &app,
        &board_id,
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
    limited_config.retrieval.max_active_per_board = 1;
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
async fn cross_scope_edit_moves_active_capacity_to_the_new_board() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_a = common::create_test_board(&app, &cookie, &csrf).await;
    let board_b = create_board_named(&app, "Capacity Board B", &cookie, &csrf).await;
    let item = create_candidate(&app, &board_a, &cookie, &csrf, "Cross-scope capacity").await;
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

    let pool = state.db.as_ref().unwrap();
    let admin_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'admin@localhost'")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut config = state.config.knowledge.clone();
    config.retrieval.max_active_per_board = 1;
    let service = KnowledgeService::new(pool, &config);
    let edited = service
        .edit(
            item_id,
            2,
            admin_id,
            KnowledgeRevisionPatch {
                scope: Some(KnowledgeScope::Board),
                board_id: Some(Some(Uuid::parse_str(&board_b).unwrap())),
                content: Some("Moved to board B.".into()),
                ..KnowledgeRevisionPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(Some(edited.revision_id), edited.active_revision_id);

    let overflow = |board_id: &str| KnowledgeRevisionInput {
        scope: KnowledgeScope::Board,
        board_id: Some(Uuid::parse_str(board_id).unwrap()),
        agent_id: None,
        knowledge_type: KnowledgeType::TestCommand,
        title: format!("Overflow {board_id}"),
        content: "Capacity probe.".into(),
        source_type: KnowledgeSourceType::HumanNote,
        source_id: None,
        source_run_id: None,
        confidence: KnowledgeConfidence::High,
    };
    let error = service
        .create_manual(admin_id, overflow(&board_b))
        .await
        .expect_err("board B holds the moved item");
    assert!(matches!(error, KnowledgeError::Capacity(_)));
    service
        .create_manual(admin_id, overflow(&board_a))
        .await
        .expect("board A released its slot when the item moved");
}

#[tokio::test]
async fn activation_revalidates_capacity_before_replacing_the_active_revision() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let first = create_candidate(&app, &board_id, &cookie, &csrf, "Capacity occupant").await;
    let second = create_candidate(&app, &board_id, &cookie, &csrf, "Activation candidate").await;

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

    let mut config = state.config.knowledge.clone();
    config.retrieval.max_active_per_board = 1;
    let second_item_id = Uuid::parse_str(second["id"].as_str().unwrap()).unwrap();
    let second_revision_id = Uuid::parse_str(second["revisionId"].as_str().unwrap()).unwrap();
    let mut tx = state.db.as_ref().unwrap().begin().await.unwrap();
    let error = activate_revision(&mut tx, second_item_id, second_revision_id, &config)
        .await
        .expect_err("activation must revalidate capacity under its transaction lock");
    assert!(matches!(error, KnowledgeError::Capacity(_)));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn supersession_completes_when_the_replacement_is_approved() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let original = create_candidate(&app, &board_id, &cookie, &csrf, "Old rule").await;
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

    let replacement_body = serde_json::json!({
        "expectedVersion": 2,
        "replacement": {
            "scope": "board",
            "boardId": board_id,
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
                "scope": "board",
                "boardId": board_id,
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
async fn supersession_approval_swaps_retrieval_to_the_replacement_atomically() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let board_uuid = Uuid::parse_str(&board_id).unwrap();
    let original =
        create_candidate(&app, &board_id, &cookie, &csrf, "Original superseded rule").await;
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

    let (status, replacement) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        supersede_body(
            &board_id,
            2,
            "Replacement superseding rule",
            "Run make test-smoke before review.",
        ),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let replacement_id = replacement["id"].as_str().unwrap();
    let replacement_uuid = Uuid::parse_str(replacement_id).unwrap();

    let pool = state.db.as_ref().unwrap();
    let mut retrieval_config = state.config.knowledge.retrieval.clone();
    retrieval_config.top_k = 20;
    let query = ("superseding rule", "Run make test-smoke before review.");
    let before = retrieve(
        pool,
        &RetrievalScope::Boards(vec![board_uuid]),
        Uuid::new_v4(),
        query.0,
        query.1,
        &retrieval_config,
    )
    .await
    .unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].item_id, original_uuid);

    let (status, approved) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{replacement_id}/approve"),
        serde_json::json!({"expectedVersion": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["activeRevisionId"], approved["revisionId"]);

    let original_link: Option<Uuid> =
        sqlx::query_scalar("SELECT superseded_by FROM knowledge_items WHERE id = $1")
            .bind(original_uuid)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(original_link, Some(replacement_uuid));

    let after = retrieve(
        pool,
        &RetrievalScope::Boards(vec![board_uuid]),
        Uuid::new_v4(),
        query.0,
        query.1,
        &retrieval_config,
    )
    .await
    .unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].item_id, replacement_uuid);
    assert!(after[0].score > 0.0);
}

#[tokio::test]
async fn supersession_rejects_a_new_candidate_for_an_already_retired_original() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let original = create_candidate(
        &app,
        &board_id,
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

    let replacement_body = |expected_version, title: &str| {
        serde_json::json!({
            "expectedVersion": expected_version,
            "replacement": {
                "scope": "board",
                "boardId": board_id,
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
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let original =
        create_candidate(&app, &board_id, &cookie, &csrf, "Singular replacement rule").await;
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

    let replacement_body = |expected_version, title: &str| {
        serde_json::json!({
            "expectedVersion": expected_version,
            "replacement": {
                "scope": "board",
                "boardId": board_id,
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
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let original = create_candidate(
        &app,
        &board_id,
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

    let (status, first) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        supersede_body(
            &board_id,
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
            &board_id,
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
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let board_uuid = Uuid::parse_str(&board_id).unwrap();
    let original = create_candidate(
        &app,
        &board_id,
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

    let (status, abandoned) = mutate(
        &app,
        "POST",
        &format!("/api/knowledge/{original_id}/supersede"),
        supersede_body(
            &board_id,
            2,
            "Rejected never-approved candidate",
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
        &format!("/api/knowledge/{abandoned_id}/reject"),
        serde_json::json!({"expectedVersion": 1, "reason": "abandoned before approval"}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let approve_path = format!("/api/knowledge/{abandoned_id}/approve");
    let supersede_path = format!("/api/knowledge/{original_id}/supersede");
    let ((approve_status, _), (create_status, _)) =
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                mutate(
                    &app,
                    "POST",
                    &approve_path,
                    serde_json::json!({"expectedVersion": 2}),
                    &cookie,
                    &csrf,
                ),
                mutate(
                    &app,
                    "POST",
                    &supersede_path,
                    supersede_body(
                        &board_id,
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

    let retrieved = retrieve(
        pool,
        &RetrievalScope::Boards(vec![board_uuid]),
        Uuid::new_v4(),
        "supersession race",
        "",
        &state.config.knowledge.retrieval,
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

    type ItemRow = (Uuid, String, i32, Option<Uuid>, Option<String>);
    let group_one: Vec<ItemRow> = sqlx::query_as(
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
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let other_board_id = create_board_named(&app, "Other Knowledge Board", &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Knowledge Worker",
        &cookie,
        &csrf,
    )
    .await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let item = create_candidate(&app, &board_id, &cookie, &csrf, "Use smoke tests").await;
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

    let found = retrieve(
        state.db.as_ref().unwrap(),
        &RetrievalScope::Boards(vec![Uuid::parse_str(&board_id).unwrap()]),
        Uuid::parse_str(&agent_id).unwrap(),
        "Run tests",
        "",
        &state.config.knowledge.retrieval,
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].score > 0.0, "\"run\" matches the item content");
    let wrong_board = retrieve(
        state.db.as_ref().unwrap(),
        &RetrievalScope::Boards(vec![Uuid::parse_str(&other_board_id).unwrap()]),
        Uuid::parse_str(&agent_id).unwrap(),
        "Run tests",
        "",
        &state.config.knowledge.retrieval,
    )
    .await
    .unwrap();
    assert!(wrong_board.is_empty());

    let mut type_filtered = state.config.knowledge.retrieval.clone();
    type_filtered.allowed_types = vec!["bug_pattern".into()];
    let excluded_by_type = retrieve(
        state.db.as_ref().unwrap(),
        &RetrievalScope::Boards(vec![Uuid::parse_str(&board_id).unwrap()]),
        Uuid::parse_str(&agent_id).unwrap(),
        "Run tests",
        "",
        &type_filtered,
    )
    .await
    .unwrap();
    assert!(excluded_by_type.is_empty());

    type_filtered.allowed_types = vec!["test_command".into()];
    let included_by_type = retrieve(
        state.db.as_ref().unwrap(),
        &RetrievalScope::Boards(vec![Uuid::parse_str(&board_id).unwrap()]),
        Uuid::parse_str(&agent_id).unwrap(),
        "Run tests",
        "",
        &type_filtered,
    )
    .await
    .unwrap();
    assert_eq!(included_by_type.len(), 1);

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
    let board_id = Uuid::parse_str(&common::create_test_board(&app, &cookie, &csrf).await).unwrap();
    let other_board_id =
        Uuid::parse_str(&create_board_named(&app, "Retrieval Matrix Other", &cookie, &csrf).await)
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
        label: "valid board",
        status: "approved",
        scope: "board",
        board_id: Some(board_id),
        agent_id: None,
        confidence: "high",
        expired: false,
        activate: true,
    };

    let valid_board = seed_retrieval_item(pool, base).await.0;
    let valid_workspace = seed_retrieval_item(
        pool,
        RetrievalSeed {
            label: "valid workspace",
            scope: "workspace",
            board_id: None,
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
            label: "wrong board",
            board_id: Some(other_board_id),
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
        &RetrievalScope::Boards(vec![board_id]),
        agent_id,
        "valid",
        "",
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
        [valid_board, valid_workspace, valid_agent]
            .into_iter()
            .collect()
    );
}

#[tokio::test]
async fn knowledge_query_plan_has_relational_indexes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let target_board_id =
        Uuid::parse_str(&create_board_named(&app, "Retrieval target", &cookie, &csrf).await)
            .unwrap();
    let other_board_id =
        Uuid::parse_str(&create_board_named(&app, "Retrieval noise", &cookie, &csrf).await)
            .unwrap();
    let pool = state.db.as_ref().unwrap();
    let mut tx = pool.begin().await.unwrap();
    seed_retrieval_cardinality(&mut tx, target_board_id, other_board_id, 32, 512, 4_096).await;

    // On small seeded tables the planner sometimes prefers the list index
    // (it costs indexes by their real size, so faking pg_class stats is not
    // enough). Dropping it inside the rolled-back transaction keeps the plan
    // deterministic; the database is private to this test.
    sqlx::query("DROP INDEX knowledge_items_list_idx")
        .execute(&mut *tx)
        .await
        .expect("knowledge_items_list_idx missing");

    let explain = explain_production_retrieval(&mut tx, target_board_id, true).await;
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
            name == "knowledge_revisions_board_scope_idx" || name == "knowledge_revisions_pkey"
        }),
        "expected a relational revision index scan in {index_scans:?}\n{explain:#}"
    );

    assert!(
        find_plan_node(plan, "CTE Name", "eligible").is_some(),
        "full-text ranking must run over the materialized eligible set\n{explain:#}"
    );
    tx.rollback().await.unwrap();
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
    let board_id = Uuid::new_v4();
    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
        .bind(board_id)
        .bind("M06 retrieval capacity benchmark")
        .bind(format!("m06-retrieval-benchmark-{board_id}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    seed_retrieval_cardinality(&mut tx, board_id, board_id, 10_000, 0, 0).await;

    let _warmup = explain_production_retrieval(&mut tx, board_id, true).await;
    let mut timings_ms = Vec::with_capacity(20);
    for _ in 0..20 {
        let explain = explain_production_retrieval(&mut tx, board_id, true).await;
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

async fn create_candidate_with_content(
    app: &Router,
    board_id: &str,
    cookie: &str,
    csrf: &str,
    title: &str,
    content: &str,
) -> serde_json::Value {
    let body = serde_json::json!({
        "scope": "board",
        "boardId": board_id,
        "knowledgeType": "test_command",
        "title": title,
        "content": content,
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

async fn approve_item(
    app: &Router,
    item_id: &str,
    expected_version: i64,
    cookie: &str,
    csrf: &str,
) -> serde_json::Value {
    let (status, approved) = mutate(
        app,
        "POST",
        &format!("/api/knowledge/{item_id}/approve"),
        serde_json::json!({"expectedVersion": expected_version}),
        cookie,
        csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["activeRevisionId"], approved["revisionId"]);
    approved
}

async fn get_similar(
    app: &Router,
    item_id: &str,
    query: &str,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, serde_json::Value) {
    let path = if query.is_empty() {
        format!("/api/knowledge/{item_id}/similar")
    } else {
        format!("/api/knowledge/{item_id}/similar?{query}")
    };
    let response = app
        .clone()
        .oneshot(common::json_request("GET", &path, "", cookie, csrf))
        .await
        .unwrap();
    let status = response.status();
    (status, common::json_body(response).await)
}

#[tokio::test]
async fn similar_returns_identical_approved_neighbor_for_pending() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let title = "Inbox duplicate assist title";
    let content = "Run make test-unit before review.";

    let neighbor =
        create_candidate_with_content(&app, &board_id, &cookie, &csrf, title, content).await;
    let neighbor_id = neighbor["id"].as_str().unwrap().to_string();
    let ready = approve_item(&app, &neighbor_id, 1, &cookie, &csrf).await;

    let pending =
        create_candidate_with_content(&app, &board_id, &cookie, &csrf, title, content).await;
    let pending_id = pending["id"].as_str().unwrap();

    let (status, body) = get_similar(&app, pending_id, "limit=5", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["itemId"], neighbor_id);
    assert_eq!(items[0]["revisionId"], ready["activeRevisionId"]);
    assert_eq!(items[0]["title"], title);
    assert_eq!(items[0]["knowledgeType"], "test_command");
    assert_eq!(items[0]["scope"], "board");
    assert_eq!(items[0]["status"], "approved");
    assert_eq!(items[0]["score"].as_f64().unwrap(), 1.0);
}

#[tokio::test]
async fn similar_returns_empty_for_dissimilar_pending() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;

    let neighbor = create_candidate_with_content(
        &app,
        &board_id,
        &cookie,
        &csrf,
        "Approved neighbor title",
        "Run make test-unit before review.",
    )
    .await;
    let neighbor_id = neighbor["id"].as_str().unwrap();
    approve_item(&app, neighbor_id, 1, &cookie, &csrf).await;

    let pending = create_candidate_with_content(
        &app,
        &board_id,
        &cookie,
        &csrf,
        "Totally unrelated guidance",
        "Never mix paint with diesel fuel.",
    )
    .await;
    let pending_id = pending["id"].as_str().unwrap();

    let (status, body) = get_similar(&app, pending_id, "", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn similar_ignores_neighbors_sharing_only_common_terms() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;

    for (title, content) in [
        (
            "Connector run_timeout_secs defaults",
            "Each connector has a run_timeout_secs setting in config.toml. The default is 3600 \
             seconds; a run that goes past it is cancelled and the ticket moves back to Ready. \
             Raise it for long test runs or long reviews.",
        ),
        (
            "Server checks: scoped cargo test before review",
            "Before review, run make test-smoke or a scoped cargo test -p coppice-server \
             --features embedded-test-db <filter>. Run make test only for final acceptance, \
             then make clean.",
        ),
    ] {
        let neighbor =
            create_candidate_with_content(&app, &board_id, &cookie, &csrf, title, content).await;
        approve_item(&app, neighbor["id"].as_str().unwrap(), 1, &cookie, &csrf).await;
    }

    let pending = create_candidate_with_content(
        &app,
        &board_id,
        &cookie,
        &csrf,
        "Active-run check on drag into review",
        "Dragging a ticket into In Review is refused while the ticket has an active run. The \
         server checks hasActiveRun and answers 409; cancel the run or wait for it to finish, \
         then make the move again.",
    )
    .await;
    let pending_id = pending["id"].as_str().unwrap();

    let (status, body) = get_similar(&app, pending_id, "limit=10", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0, "{body}");
}

#[tokio::test]
async fn folded_stopwords_match_coppice_unaccent() {
    use coppice_server::knowledge::fts::FOLDED_STOPWORDS;
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, _app, _cookie, _csrf) = common::bootstrap_and_login_with_state().await;
    let accented = [
        "và", "của", "là", "các", "cho", "với", "trong", "được", "có", "không", "này", "một",
        "những", "để", "khi", "thì", "đã", "sẽ", "từ", "theo", "cũng", "như", "nếu", "vào", "ra",
    ];
    let folded: Vec<String> =
        sqlx::query_scalar("SELECT coppice_unaccent(w) FROM unnest($1::text[]) WITH ORDINALITY AS t(w, n) ORDER BY n")
            .bind(&accented[..])
            .fetch_all(state.db.as_ref().unwrap())
            .await
            .unwrap();
    assert_eq!(folded, FOLDED_STOPWORDS);
}

#[tokio::test]
async fn similar_excludes_ineligible_neighbors_and_self() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let board_uuid = Uuid::parse_str(&board_id).unwrap();
    let pool = state.db.as_ref().unwrap();
    let title = "Eligibility matrix title";
    let content = "Shared eligibility content for inbox assist.";

    let eligible =
        create_candidate_with_content(&app, &board_id, &cookie, &csrf, title, content).await;
    let eligible_id = eligible["id"].as_str().unwrap().to_string();
    approve_item(&app, &eligible_id, 1, &cookie, &csrf).await;

    #[allow(clippy::too_many_arguments)]
    async fn seed_variant(
        pool: &PgPool,
        board_id: Uuid,
        title: &str,
        content: &str,
        status: &str,
        activate: bool,
        expired: bool,
        superseded_by: Option<Uuid>,
    ) -> Uuid {
        let item_id = Uuid::new_v4();
        let revision_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO knowledge_items (id, status, version, expires_at, superseded_by)
            VALUES (
                $1, $2, 1,
                CASE WHEN $3 THEN now() - interval '1 minute' ELSE NULL END,
                $4
            )
            "#,
        )
        .bind(item_id)
        .bind(status)
        .bind(expired)
        .bind(superseded_by)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO knowledge_revisions (
                id, item_id, revision_number, scope, board_id,
                knowledge_type, title, content, source_type, confidence
            ) VALUES ($1, $2, 1, 'board', $3, 'test_command', $4, $5, 'human_note', 'high')
            "#,
        )
        .bind(revision_id)
        .bind(item_id)
        .bind(board_id)
        .bind(title)
        .bind(content)
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
        .bind(activate)
        .execute(pool)
        .await
        .unwrap();
        item_id
    }

    seed_variant(
        pool, board_uuid, title, content, "rejected", true, false, None,
    )
    .await;
    seed_variant(pool, board_uuid, title, content, "stale", true, false, None).await;
    seed_variant(
        pool, board_uuid, title, content, "approved", true, true, None,
    )
    .await;
    let eligible_uuid = Uuid::parse_str(&eligible_id).unwrap();
    seed_variant(
        pool,
        board_uuid,
        title,
        content,
        "approved",
        true,
        false,
        Some(eligible_uuid),
    )
    .await;
    seed_variant(
        pool, board_uuid, title, content, "approved", false, false, None,
    )
    .await;

    let pending =
        create_candidate_with_content(&app, &board_id, &cookie, &csrf, title, content).await;
    let pending_id = pending["id"].as_str().unwrap();

    let (status, body) = get_similar(&app, pending_id, "limit=10", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["itemId"], eligible_id);
}

#[tokio::test]
async fn similar_authz_allows_member_read_and_rejects_unauthenticated() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pending = create_candidate(&app, &board_id, &cookie, &csrf, "Authz pending").await;
    let pending_id = pending["id"].as_str().unwrap();

    let unauthenticated = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/api/knowledge/{pending_id}/similar"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    sqlx::query("UPDATE users SET role = 'member' WHERE email = 'admin@localhost'")
        .execute(state.db.as_ref().unwrap())
        .await
        .unwrap();

    let (status, body) = get_similar(&app, pending_id, "", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].as_array().unwrap().is_empty());

    let forbidden_approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/knowledge/{pending_id}/approve"),
            &serde_json::json!({"expectedVersion": 1}).to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(forbidden_approve.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn similar_rejects_non_pending_and_missing_items() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let created = create_candidate(&app, &board_id, &cookie, &csrf, "Approve first").await;
    let item_id = created["id"].as_str().unwrap();
    approve_item(&app, item_id, 1, &cookie, &csrf).await;

    let (status, body) = get_similar(&app, item_id, "", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].as_str().unwrap().contains("pending"));

    let missing = Uuid::new_v4();
    let (status, _) = get_similar(&app, &missing.to_string(), "", &cookie, &csrf).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// An approved, active board note with exactly this text.
async fn seed_note(pool: &PgPool, board_id: Uuid, title: &str, content: &str) -> Uuid {
    let item_id = Uuid::new_v4();
    let revision_id = Uuid::new_v4();
    sqlx::query("INSERT INTO knowledge_items (id, status, version) VALUES ($1, 'approved', 1)")
        .bind(item_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO knowledge_revisions (
            id, item_id, revision_number, scope, board_id,
            knowledge_type, title, content, source_type, confidence
        ) VALUES ($1, $2, 1, 'board', $3, 'test_command', $4, $5, 'human_note', 'high')
        "#,
    )
    .bind(revision_id)
    .bind(item_id)
    .bind(board_id)
    .bind(title)
    .bind(content)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE knowledge_items SET current_revision_id = $2, active_revision_id = $2 WHERE id = $1",
    )
    .bind(item_id)
    .bind(revision_id)
    .execute(pool)
    .await
    .unwrap();
    revision_id
}

/// Matching notes for `query`, best first, as `(title, score)`.
async fn search(state: &AppState, board_id: Uuid, query: &str) -> Vec<(String, f64)> {
    let mut found = retrieve(
        state.db.as_ref().unwrap(),
        &RetrievalScope::Boards(vec![board_id]),
        Uuid::new_v4(),
        query,
        "",
        &state.config.knowledge.retrieval,
    )
    .await
    .unwrap();
    found.retain(|k| k.score > 0.0);
    found.into_iter().map(|k| (k.title, k.score)).collect()
}

async fn search_titles(state: &AppState, board_id: Uuid, query: &str) -> Vec<String> {
    let found = search(state, board_id, query).await;
    found.into_iter().map(|(title, _)| title).collect()
}

#[tokio::test]
async fn search_matches_other_forms_of_a_word() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board = Uuid::parse_str(&common::create_test_board(&app, &cookie, &csrf).await).unwrap();
    let pool = state.db.as_ref().unwrap();
    seed_note(pool, board, "Run test suite", "Keep this in mind.").await;
    seed_note(pool, board, "Merge after review", "Keep this in mind.").await;
    seed_note(pool, board, "Paint storage", "Keep this in mind.").await;

    assert_eq!(
        search_titles(&state, board, "tests").await,
        ["Run test suite"]
    );
    assert_eq!(
        search_titles(&state, board, "merging").await,
        ["Merge after review"]
    );
}

#[tokio::test]
async fn search_matches_parts_of_dotted_names_and_paths() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board = Uuid::parse_str(&common::create_test_board(&app, &cookie, &csrf).await).unwrap();
    let pool = state.db.as_ref().unwrap();
    seed_note(pool, board, "Edit config.toml first", "Keep this in mind.").await;
    seed_note(
        pool,
        board,
        "server/src/knowledge/fts.rs",
        "Keep this in mind.",
    )
    .await;
    seed_note(pool, board, "Paint storage", "Keep this in mind.").await;

    assert_eq!(
        search_titles(&state, board, "config toml").await,
        ["Edit config.toml first"]
    );
    assert_eq!(
        search_titles(&state, board, "fts rs").await,
        ["server/src/knowledge/fts.rs"]
    );
}

#[tokio::test]
async fn search_ranks_a_rare_word_above_many_common_words() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board = Uuid::parse_str(&common::create_test_board(&app, &cookie, &csrf).await).unwrap();
    let pool = state.db.as_ref().unwrap();
    let csrf_title = "CSRF token must be sent on mutations";
    seed_note(pool, board, csrf_title, "Send the header every time.").await;
    for n in 0..5 {
        seed_note(
            pool,
            board,
            &format!("Server test run ticket {n}"),
            "The server runs a test run for each ticket; a test run on the server closes the \
             ticket. Run the server test before the ticket run.",
        )
        .await;
    }

    let titles = search_titles(&state, board, "csrf server test run ticket").await;
    assert_eq!(titles.len(), 6);
    assert_eq!(titles[0], csrf_title);
}

#[tokio::test]
async fn search_counts_a_title_match_double() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board = Uuid::parse_str(&common::create_test_board(&app, &cookie, &csrf).await).unwrap();
    let pool = state.db.as_ref().unwrap();
    seed_note(pool, board, "Paint storage", "Keep the cans dry.").await;
    seed_note(
        pool,
        board,
        "Workshop rules",
        "Paint goes on the top shelf.",
    )
    .await;

    let found = search(&state, board, "paint").await;
    assert_eq!(found[0].0, "Paint storage");
    assert_eq!(found[1].0, "Workshop rules");
    assert!((found[0].1 - 2.0 * found[1].1).abs() < 1e-9, "{found:?}");
}

#[tokio::test]
async fn search_keeps_a_term_that_is_an_english_stopword() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board = Uuid::parse_str(&common::create_test_board(&app, &cookie, &csrf).await).unwrap();
    let pool = state.db.as_ref().unwrap();
    seed_note(pool, board, "A very careful deploy", "Keep this in mind.").await;
    seed_note(pool, board, "Paint storage", "Keep this in mind.").await;

    // Not in our stopword list, so it reaches SQL, where 'english' drops it.
    assert_eq!(
        search_titles(&state, board, "very").await,
        ["A very careful deploy"]
    );
}

#[tokio::test]
async fn search_with_no_usable_terms_scores_every_note_zero() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board = Uuid::parse_str(&common::create_test_board(&app, &cookie, &csrf).await).unwrap();
    let pool = state.db.as_ref().unwrap();
    seed_note(pool, board, "Paint storage", "Keep dry.").await;

    let found = retrieve(
        pool,
        &RetrievalScope::Boards(vec![board]),
        Uuid::new_v4(),
        "the and of",
        "",
        &state.config.knowledge.retrieval,
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].score, 0.0);
}

#[tokio::test]
async fn existing_revisions_are_searchable_after_upgrade() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, _app, _cookie, _csrf) = common::bootstrap_and_login_with_state().await;
    let admin = state.db.as_ref().unwrap();
    let database = format!("coppice_upgrade_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {database}"))
        .execute(admin)
        .await
        .unwrap();
    let pool = PgPool::connect_with(admin.connect_options().as_ref().clone().database(&database))
        .await
        .unwrap();

    let before_search_quality = Migrator {
        migrations: MIGRATOR
            .iter()
            .filter(|m| m.version <= 60)
            .cloned()
            .collect::<Vec<_>>()
            .into(),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    before_search_quality.run(&pool).await.unwrap();

    let item_id = Uuid::new_v4();
    let revision_id = Uuid::new_v4();
    let content = "Set it in config.toml, then restart.";
    sqlx::query("INSERT INTO knowledge_items (id, status, version) VALUES ($1, 'approved', 1)")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO knowledge_revisions (
            id, item_id, revision_number, scope,
            knowledge_type, title, content, source_type, confidence
        ) VALUES ($1, $2, 1, 'workspace', 'test_command', 'Connector settings', $3,
                  'human_note', 'high')
        "#,
    )
    .bind(revision_id)
    .bind(item_id)
    .bind(content)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE knowledge_items SET current_revision_id = $2, active_revision_id = $2 WHERE id = $1",
    )
    .bind(item_id)
    .bind(revision_id)
    .execute(&pool)
    .await
    .unwrap();
    let split_before: bool =
        sqlx::query_scalar("SELECT search_vector @@ $$'toml'$$::tsquery FROM knowledge_revisions")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!split_before, "the old vector has no split token");

    MIGRATOR.run(&pool).await.unwrap();

    let (stored, whole, part): (String, bool, bool) = sqlx::query_as(
        r#"
        SELECT content,
               search_vector @@ $$'config.toml'$$::tsquery,
               search_vector @@ $$'toml'$$::tsquery
        FROM knowledge_revisions
        WHERE id = $1 AND search_vector IS NOT NULL
        "#,
    )
    .bind(revision_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(whole, "vector keeps the whole token");
    assert!(part, "vector has the split token");
    assert_eq!(stored, content);
    let found = retrieve(
        &pool,
        &RetrievalScope::Boards(Vec::new()),
        Uuid::new_v4(),
        "toml",
        "",
        &state.config.knowledge.retrieval,
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].score > 0.0);

    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {database} WITH (FORCE)"))
        .execute(admin)
        .await
        .unwrap();
}

/// Seed 1,000 notes on `board`. Each holds the term `word{n % 32}` in its title
/// and `words` more in its content, plus `paths` file paths that match nothing.
async fn seed_latency_notes(pool: &PgPool, board: Uuid, stride: i32, words: i32, paths: i32) {
    sqlx::query(
        r#"
        WITH seed AS (
            SELECT n,
                   md5($1::text || 'item' || n)::uuid AS item_id,
                   md5($1::text || 'rev' || n)::uuid AS revision_id
            FROM generate_series(1, 1000) AS n
        ), items AS (
            INSERT INTO knowledge_items (id, status, version)
            SELECT item_id, 'approved', 1 FROM seed
        )
        INSERT INTO knowledge_revisions (
            id, item_id, revision_number, scope, board_id,
            knowledge_type, title, content, source_type, confidence
        )
        SELECT s.revision_id, s.item_id, 1, 'board', $1, 'test_command',
               'Latency note word' || (s.n % 32),
               (SELECT string_agg('word' || ((s.n + w * $2) % 32), ' ')
                FROM generate_series(1, $3) AS w)
               || ' '
               || (SELECT string_agg('server/src/file' || w || '.rs', ' ')
                   FROM generate_series(1, $4) AS w),
               'human_note', 'high'
        FROM seed s
        "#,
    )
    .bind(board)
    .bind(stride)
    .bind(words)
    .bind(paths)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        UPDATE knowledge_items i
        SET current_revision_id = r.id, active_revision_id = r.id
        FROM knowledge_revisions r
        WHERE r.item_id = i.id AND r.board_id = $1
        "#,
    )
    .bind(board)
    .execute(pool)
    .await
    .unwrap();
}

/// Prints sizes and sorted timings for a 32-term query; returns the slowest.
async fn report_latency(state: &AppState, board: Uuid, label: &str) -> f64 {
    let pool = state.db.as_ref().unwrap();
    let (bytes, matching): (f64, f64) = sqlx::query_as(
        r#"
        SELECT avg(length(content))::float8,
               (count(*) FILTER (WHERE search_vector @@ $$'word0'$$::tsquery))::float8
                   / count(*)::float8
        FROM knowledge_revisions
        WHERE board_id = $1
        "#,
    )
    .bind(board)
    .fetch_one(pool)
    .await
    .unwrap();
    let query = (0..32)
        .map(|n| format!("word{n}"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut timings_ms = Vec::new();
    for _ in 0..10 {
        let started = std::time::Instant::now();
        let found = search(state, board, &query).await;
        timings_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(!found.is_empty());
    }
    timings_ms.sort_by(f64::total_cmp);
    eprintln!(
        "{label}: 1,000 items, 32 terms, {bytes:.0}-byte notes, a term matches {:.0}% (ms): \
         {timings_ms:.1?}",
        matching * 100.0
    );
    timings_ms[timings_ms.len() - 1]
}

#[tokio::test]
#[ignore = "timing check; run by hand with --ignored --nocapture"]
async fn retrieval_latency_at_board_capacity() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();
    let realistic = create_board_named(&app, "Latency realistic", &cookie, &csrf).await;
    let realistic = Uuid::parse_str(&realistic).unwrap();
    let pessimistic = create_board_named(&app, "Latency pessimistic", &cookie, &csrf).await;
    let pessimistic = Uuid::parse_str(&pessimistic).unwrap();
    seed_latency_notes(pool, realistic, 11, 2, 22).await;
    seed_latency_notes(pool, pessimistic, 10, 50, 50).await;

    let worst = report_latency(&state, realistic, "realistic (target 250ms)").await;
    report_latency(&state, pessimistic, "pessimistic (informational)").await;
    assert!(worst < 250.0, "worst {worst:.1}ms is over the 250ms target");
}

#[tokio::test]
async fn full_run_context_lists_matching_titles_only() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().unwrap();
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Index Worker",
        &cookie,
        &csrf,
    )
    .await;
    let mut ids = Vec::new();
    for title in ["Use smoke checks", "Rotate credentials"] {
        let item = create_candidate(&app, &board_id, &cookie, &csrf, title).await;
        let item_id = item["id"].as_str().unwrap().to_string();
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
        ids.push(item_id);
    }
    let board_uuid = Uuid::parse_str(&board_id).unwrap();
    let agent_uuid = Uuid::parse_str(&agent_id).unwrap();

    let index = coppice_server::knowledge::index::build_run_index(
        pool,
        board_uuid,
        agent_uuid,
        "Add smoke coverage",
        "",
        &state.config.knowledge,
    )
    .await
    .unwrap()
    .expect("a matching entry is listed");
    assert!(index.contains("Use smoke checks"), "{index}");
    assert!(index.contains(&ids[0]), "{index}");
    assert!(!index.contains("Rotate credentials"), "{index}");
    assert!(!index.contains("make test-unit"), "content stays out: {index}");

    let nothing = coppice_server::knowledge::index::build_run_index(
        pool,
        board_uuid,
        agent_uuid,
        "Unrelated zebra work",
        "",
        &state.config.knowledge,
    )
    .await
    .unwrap();
    assert!(nothing.is_none());

    let mut off = state.config.knowledge.clone();
    off.enabled = false;
    let disabled = coppice_server::knowledge::index::build_run_index(
        pool,
        board_uuid,
        agent_uuid,
        "Add smoke coverage",
        "",
        &off,
    )
    .await
    .unwrap();
    assert!(disabled.is_none());

    let logged: i64 = sqlx::query_scalar("SELECT count(*) FROM knowledge_usage_logs")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(logged, 0);
}

#[tokio::test]
async fn editing_an_item_resets_its_open_flag_count() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().expect("db pool");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let created = create_candidate(&app, &board_id, &cookie, &csrf, "Flag me").await;
    let item_id = created["id"].as_str().unwrap();
    let approved = approve_item(&app, item_id, 1, &cookie, &csrf).await;
    let revision_id: Uuid = approved["activeRevisionId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    for (verdict, run) in [("outdated", Uuid::new_v4()), ("confirmed", Uuid::new_v4())] {
        sqlx::query(
            "INSERT INTO knowledge_feedback (id, item_id, revision_id, run_id, verdict) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(item_id.parse::<Uuid>().unwrap())
        .bind(revision_id)
        .bind(run)
        .bind(verdict)
        .execute(&pool)
        .await
        .unwrap();
    }
    let (status, view) = mutate(
        &app,
        "GET",
        &format!("/api/knowledge/{item_id}"),
        serde_json::Value::Null,
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["openFlagCount"], 1, "confirmed does not count");

    let (status, edited) = mutate(
        &app,
        "PATCH",
        &format!("/api/knowledge/{item_id}"),
        serde_json::json!({"expectedVersion": approved["version"], "title": "Flag me, fixed"}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["openFlagCount"], 0);
}
