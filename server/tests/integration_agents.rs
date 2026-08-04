mod common;

use axum::http::StatusCode;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn list_presets_has_ten_entries() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/agent-presets",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 10);
    let first = &body["items"][0];
    let template = first["systemPromptTemplate"].as_str().unwrap();
    assert!(
        template.contains("# SOUL"),
        "expected SOUL template, got: {template}"
    );
}

#[tokio::test]
async fn create_agent_from_preset() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let presets_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/agent-presets",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(presets_res.status(), StatusCode::OK);

    let presets: serde_json::Value = common::json_body(presets_res).await;
    let preset = &presets["items"][0];
    let preset_id = preset["id"].as_str().unwrap();
    let preset_role = preset["role"].as_str().unwrap();

    let create_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            &format!(r#"{{"name":"PM Bot","presetId":"{preset_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);

    let agent: serde_json::Value = common::json_body(create_res).await;
    assert_eq!(agent["role"].as_str().unwrap(), preset_role);
    assert_eq!(
        agent["presetSource"].as_str().unwrap(),
        preset["key"].as_str().unwrap()
    );
    assert_eq!(agent["name"].as_str().unwrap(), "PM Bot");
    let template = preset["systemPromptTemplate"].as_str().unwrap();
    assert_eq!(agent["systemPrompt"].as_str().unwrap(), template);
    assert_eq!(agent["connector"].as_str().unwrap(), "mock");
}

#[tokio::test]
async fn create_agent_from_preset_honors_connector_and_model() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let presets_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/agent-presets",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let presets: serde_json::Value = common::json_body(presets_res).await;
    let preset_id = presets["items"][0]["id"].as_str().unwrap();

    let create_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            &format!(
                r#"{{"name":"OpenCode PM","presetId":"{preset_id}","connector":"opencode","modelProvider":"anthropic","model":"claude-sonnet-4-20250514"}}"#
            ),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);

    let agent: serde_json::Value = common::json_body(create_res).await;
    assert_eq!(agent["connector"].as_str().unwrap(), "opencode");
    assert_eq!(agent["modelProvider"].as_str().unwrap(), "anthropic");
    assert_eq!(agent["model"].as_str().unwrap(), "claude-sonnet-4-20250514");
}

#[tokio::test]
async fn agent_with_unknown_provider_gets_missing_config_health() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;

    let create_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            r#"{"name":"OpenCode Bot","role":"Developer","systemPrompt":"You are a developer","connector":"opencode"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);

    coppice_server::workers::health_worker::run_health_pass_once(&state).await;

    let list_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/agents",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(list_res.status(), StatusCode::OK);

    let body: serde_json::Value = common::json_body(list_res).await;
    let agents = body["items"].as_array().unwrap();
    assert_eq!(agents.len(), 1);
    let agent = &agents[0];
    assert_eq!(agent["connector"].as_str().unwrap(), "opencode");
    assert_eq!(agent["health"].as_str().unwrap(), "missing_config");
    assert!(agent["healthDetail"]
        .as_str()
        .unwrap()
        .contains("not configured"));
}

#[tokio::test]
async fn list_connectors_returns_mock() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/connectors",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let ids: Vec<_> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"mock"));
}

#[tokio::test]
async fn deleting_agent_with_knowledge_provenance_returns_conflict_and_preserves_revision() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Provenance Agent",
        &cookie,
        &csrf,
    )
    .await;
    let create = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/knowledge",
            &serde_json::json!({
                "scope": "agent",
                "projectId": project_id,
                "agentId": agent_id,
                "knowledgeType": "test_command",
                "title": "Agent-specific command",
                "content": "Preserve this immutable provenance after deletion is refused.",
                "sourceType": "human_note",
                "confidence": "high"
            })
            .to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let knowledge = common::json_body(create).await;
    let item_id = knowledge["id"].as_str().unwrap();
    let revision_id = Uuid::parse_str(knowledge["revisionId"].as_str().unwrap()).unwrap();

    let deleted = app
        .clone()
        .oneshot(common::json_request(
            "DELETE",
            &format!("/api/agents/{agent_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::CONFLICT);

    let knowledge = app
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
    assert_eq!(knowledge.status(), StatusCode::OK);
    let revision_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM knowledge_revisions WHERE id = $1")
            .bind(revision_id)
            .fetch_one(state.db.as_ref().unwrap())
            .await
            .unwrap();
    assert_eq!(revision_count, 1);
}

#[tokio::test]
async fn deleting_agent_with_run_sourced_knowledge_preserves_comment_and_review_provenance() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Run Provenance Agent",
        &cookie,
        &csrf,
    )
    .await;
    let run_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
        VALUES ($1, $2, $3, 'implementation', 'succeeded', 'permissive')
        "#,
    )
    .bind(run_id)
    .bind(Uuid::parse_str(&ticket_id).unwrap())
    .bind(Uuid::parse_str(&agent_id).unwrap())
    .execute(state.db.as_ref().unwrap())
    .await
    .unwrap();

    let comment = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"This comment and review must remain attributable."}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(comment.status(), StatusCode::CREATED);
    let comment = common::json_body(comment).await;
    let comment_id = comment["id"].as_str().unwrap();

    let mut knowledge_ids = Vec::new();
    for source_type in ["comment", "review"] {
        let create = app
            .clone()
            .oneshot(common::json_request(
                "POST",
                "/api/knowledge",
                &serde_json::json!({
                    "scope": "project",
                    "projectId": project_id,
                    "knowledgeType": "review_feedback",
                    "title": format!("Preserved {source_type} provenance"),
                    "content": "Keep the exact source comment and originating run.",
                    "sourceType": source_type,
                    "sourceId": comment_id,
                    "sourceRunId": run_id,
                    "confidence": "high"
                })
                .to_string(),
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::CREATED);
        let knowledge = common::json_body(create).await;
        assert_eq!(knowledge["sourceType"], source_type);
        assert_eq!(knowledge["sourceId"], comment_id);
        assert_eq!(knowledge["sourceRunId"], run_id.to_string());
        knowledge_ids.push(knowledge["id"].as_str().unwrap().to_string());
    }

    let deleted = app
        .clone()
        .oneshot(common::json_request(
            "DELETE",
            &format!("/api/agents/{agent_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::CONFLICT);

    for item_id in knowledge_ids {
        let knowledge = app
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
        assert_eq!(knowledge.status(), StatusCode::OK);
    }
    let run_count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(state.db.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(run_count, 1);
}

#[tokio::test]
async fn deleting_agent_preserves_run_knowledge_usage_audit() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Usage Audit Agent",
        &cookie,
        &csrf,
    )
    .await;
    let knowledge = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/knowledge",
            &serde_json::json!({
                "scope": "project",
                "projectId": project_id,
                "knowledgeType": "test_command",
                "title": "Audited command",
                "content": "Keep the run and usage snapshot that consumed this revision.",
                "sourceType": "human_note",
                "confidence": "high"
            })
            .to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(knowledge.status(), StatusCode::CREATED);
    let knowledge = common::json_body(knowledge).await;
    let item_id = Uuid::parse_str(knowledge["id"].as_str().unwrap()).unwrap();
    let revision_id = Uuid::parse_str(knowledge["revisionId"].as_str().unwrap()).unwrap();
    let run_id = Uuid::new_v4();
    let pool = state.db.as_ref().unwrap();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
        VALUES ($1, $2, $3, 'implementation', 'succeeded', 'permissive')
        "#,
    )
    .bind(run_id)
    .bind(Uuid::parse_str(&ticket_id).unwrap())
    .bind(Uuid::parse_str(&agent_id).unwrap())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO knowledge_usage_logs (
            id, run_id, item_id, revision_id, rank, similarity,
            token_count, rendered_content
        ) VALUES ($1, $2, $3, $4, 1, 0.9, 12, 'immutable usage snapshot')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(run_id)
    .bind(item_id)
    .bind(revision_id)
    .execute(pool)
    .await
    .unwrap();

    let deleted = app
        .clone()
        .oneshot(common::json_request(
            "DELETE",
            &format!("/api/agents/{agent_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::CONFLICT);

    let run_count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .unwrap();
    let usage_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM knowledge_usage_logs WHERE run_id = $1")
            .bind(run_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(run_count, 1);
    assert_eq!(usage_count, 1);
}
