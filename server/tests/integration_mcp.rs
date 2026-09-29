mod common;

use axum::Router;
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::mcp::token::{NewRunToolScope, TokenService};
use coppice_server::services::knowledge_compaction_service::KnowledgeCompactionService;
use coppice_server::services::run_service::RunService;
use coppice_server::services::workspace_settings_service::WorkspaceSettingsService;
use coppice_server::AppState;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

struct RunFixture {
    state: Arc<AppState>,
    app: Router,
    cookie: String,
    csrf: String,
    pool: PgPool,
    scope: NewRunToolScope,
    _git_dir: tempfile::TempDir,
}

async fn run_fixture() -> RunFixture {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;

    let pool = state.db.clone().expect("db pool");
    let ticket_uuid: Uuid = ticket_id.parse().unwrap();
    let agent_uuid: Uuid = agent_id.parse().unwrap();
    let run = RunService::new(&pool)
        .start_run_for_agent(
            ticket_uuid,
            agent_uuid,
            "work_on_ticket",
            Default::default(),
        )
        .await
        .expect("start run");

    let scope = NewRunToolScope {
        run_id: run.id,
        agent_id: agent_uuid,
        ticket_id: Some(ticket_uuid),
        chat_session_id: None,
        board_id: Some(board_id.parse().unwrap()),
        profile: ContextProfile::HumanAgent,
        job_type: "work_on_ticket".into(),
        compaction_ticket_ids: vec![Uuid::new_v4()],
    };
    RunFixture {
        state,
        app,
        cookie,
        csrf,
        pool,
        scope,
        _git_dir: git_dir,
    }
}

#[tokio::test]
async fn token_mint_verify_roundtrip() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let tokens = TokenService::new(&fx.pool);
    let plaintext = tokens
        .mint(&fx.scope, Duration::from_secs(60))
        .await
        .expect("mint");

    let scope = tokens
        .verify(&plaintext)
        .await
        .expect("verify")
        .expect("scope");
    assert_eq!(scope.run_id, fx.scope.run_id);
    assert_eq!(scope.agent_id, fx.scope.agent_id);
    assert_eq!(scope.ticket_id, fx.scope.ticket_id);
    assert_eq!(scope.board_id, fx.scope.board_id);
    assert_eq!(scope.profile, ContextProfile::HumanAgent);
    assert_eq!(scope.job_type, "work_on_ticket");
    assert_eq!(scope.compaction_ticket_ids, fx.scope.compaction_ticket_ids);

    let stored: String = sqlx::query_scalar("SELECT token_hash FROM run_tool_tokens WHERE id = $1")
        .bind(scope.token_id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_ne!(stored, plaintext);
}

#[tokio::test]
async fn token_revoked_is_rejected() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let tokens = TokenService::new(&fx.pool);
    let plaintext = tokens
        .mint(&fx.scope, Duration::from_secs(60))
        .await
        .expect("mint");
    tokens
        .revoke_for_run(fx.scope.run_id)
        .await
        .expect("revoke");
    assert!(tokens.verify(&plaintext).await.expect("verify").is_none());
}

#[tokio::test]
async fn token_expired_is_rejected() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let tokens = TokenService::new(&fx.pool);
    let plaintext = tokens
        .mint(&fx.scope, Duration::from_secs(0))
        .await
        .expect("mint");
    assert!(tokens.verify(&plaintext).await.expect("verify").is_none());
}

#[tokio::test]
async fn token_unknown_is_rejected() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let tokens = TokenService::new(&fx.pool);
    assert!(tokens.verify("deadbeef").await.expect("verify").is_none());
}

async fn rpc(url: &str, token: &str, method: &str, params: Value) -> Value {
    reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .expect("mcp request")
        .json()
        .await
        .expect("mcp json")
}

async fn serve(fx: &RunFixture) -> String {
    let addr = common::spawn_test_server(fx.app.clone()).await;
    format!("http://{addr}/mcp")
}

async fn tool_call_rows(pool: &PgPool, run_id: Uuid) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT tool, source, status FROM run_tool_calls WHERE run_id = $1 ORDER BY created_at",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn mcp_requires_bearer() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let url = serve(&fx).await;
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"});
    let client = reqwest::Client::new();

    let res = client.post(&url).json(&body).send().await.unwrap();
    assert_eq!(res.status(), 401);
    assert_eq!(res.headers()["www-authenticate"], "Bearer");

    let res = client
        .post(&url)
        .bearer_auth("wrong")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn mcp_get_is_405() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let url = serve(&fx).await;
    let client = reqwest::Client::new();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 405);
    assert_eq!(client.delete(&url).send().await.unwrap().status(), 405);
}

#[tokio::test]
async fn mcp_notification_is_202() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let url = serve(&fx).await;
    let token =
        common::mint_test_token(&fx.state, fx.scope.run_id, ContextProfile::HumanAgent).await;
    let res = reqwest::Client::new()
        .post(&url)
        .bearer_auth(token)
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 202);
}

#[tokio::test]
async fn mcp_tools_list_is_profile_scoped() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let url = serve(&fx).await;
    let token =
        common::mint_test_token(&fx.state, fx.scope.run_id, ContextProfile::HumanChat).await;
    let res = rpc(&url, &token, "tools/list", json!({})).await;
    let names: Vec<&str> = res["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"board_agents"));
    assert!(names.contains(&"result_submit"));
    assert!(!names.contains(&"comment_post"));
}

#[tokio::test]
async fn mcp_board_agents_lists_enabled_agents() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Backend One",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let url = serve(&fx).await;
    let token =
        common::mint_test_token(&fx.state, fx.scope.run_id, ContextProfile::HumanAgent).await;
    let res = rpc(
        &url,
        &token,
        "tools/call",
        json!({"name": "board_agents", "arguments": {}}),
    )
    .await;
    assert_eq!(res["result"]["isError"], false);
    let text = res["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    let agents = parsed["agents"].as_array().unwrap();
    assert!(agents
        .iter()
        .any(|a| a["key"] == "backend_engineer" && a["name"] == "Backend One"));
    assert!(agents.iter().all(|a| a["role"].is_string()));

    let rows = tool_call_rows(&fx.pool, fx.scope.run_id).await;
    assert_eq!(
        rows,
        vec![("board_agents".into(), "core".into(), "ok".into())]
    );
}

#[tokio::test]
async fn mcp_denied_tool_is_logged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let url = serve(&fx).await;
    let token =
        common::mint_test_token(&fx.state, fx.scope.run_id, ContextProfile::HumanChat).await;
    let res = rpc(
        &url,
        &token,
        "tools/call",
        json!({"name": "comment_post", "arguments": {"body": "hi"}}),
    )
    .await;
    assert_eq!(res["result"]["isError"], true);
    let rows = tool_call_rows(&fx.pool, fx.scope.run_id).await;
    assert_eq!(
        rows,
        vec![("comment_post".into(), "core".into(), "denied".into())]
    );
}

#[tokio::test]
async fn mcp_unimplemented_tool_hides_internal_details() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = run_fixture().await;
    let url = serve(&fx).await;
    let token = common::mint_test_token(&fx.state, fx.scope.run_id, ContextProfile::Full).await;
    let res = rpc(
        &url,
        &token,
        "tools/call",
        json!({"name": "skill_list", "arguments": {}}),
    )
    .await;
    assert_eq!(res["result"]["isError"], true);
    assert_eq!(res["result"]["content"][0]["text"], "internal error");
    let rows = tool_call_rows(&fx.pool, fx.scope.run_id).await;
    assert_eq!(
        rows,
        vec![("skill_list".into(), "skill".into(), "error".into())]
    );
}

// ---- Task 4: per-run token lifecycle + mock tool calls ----

async fn wait_for_run_status(pool: &PgPool, run_id: Uuid, wanted: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(pool)
            .await
            .unwrap();
        if status == wanted {
            return;
        }
        assert!(
            !matches!(status.as_str(), "failed" | "cancelled" | "blocked") || status == wanted,
            "run ended as {status}, wanted {wanted}"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for run {run_id} to be {wanted}; last={status}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

struct TokenRow {
    profile: String,
    chat_session_id: Option<Uuid>,
    ticket_id: Option<Uuid>,
    board_id: Option<Uuid>,
    compaction_ticket_ids: Vec<Uuid>,
    revoked: bool,
}

async fn token_rows(pool: &PgPool, run_id: Uuid) -> Vec<TokenRow> {
    let rows: Vec<(String, Option<Uuid>, Option<Uuid>, Option<Uuid>, Vec<Uuid>, bool)> =
        sqlx::query_as(
            "SELECT context_profile, chat_session_id, ticket_id, board_id, \
             compaction_ticket_ids, revoked_at IS NOT NULL \
             FROM run_tool_tokens WHERE run_id = $1",
        )
        .bind(run_id)
        .fetch_all(pool)
        .await
        .unwrap();
    rows.into_iter()
        .map(
            |(profile, chat_session_id, ticket_id, board_id, compaction_ticket_ids, revoked)| {
                TokenRow {
                    profile,
                    chat_session_id,
                    ticket_id,
                    board_id,
                    compaction_ticket_ids,
                    revoked,
                }
            },
        )
        .collect()
}

/// Ticket run through the worker with the gateway listening; returns the run id.
async fn run_ticket_with_tool_calls(
) -> (Arc<AppState>, Uuid, common::AgentTestEnv, tempfile::TempDir) {
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_gateway("mcp/ticket_tool_call").await;
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/run-agent"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body = common::json_body(res).await;
    let run_id: Uuid = body["run"]["id"].as_str().unwrap().parse().unwrap();
    let pool = state.db.clone().unwrap();
    wait_for_run_status(&pool, run_id, "succeeded").await;
    (state, run_id, env, git_dir)
}

#[tokio::test]
async fn mock_run_executes_tool_calls_through_gateway() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, run_id, _env, _git) = run_ticket_with_tool_calls().await;
    let pool = state.db.clone().unwrap();
    let rows = tool_call_rows(&pool, run_id).await;
    assert_eq!(
        rows,
        vec![("board_agents".into(), "core".into(), "ok".into())]
    );
    let tokens = token_rows(&pool, run_id).await;
    assert_eq!(tokens.len(), 1);
    assert!(tokens[0].ticket_id.is_some());
    assert!(tokens[0].board_id.is_some());
}

#[tokio::test]
async fn token_revoked_after_run_finishes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, run_id, _env, _git) = run_ticket_with_tool_calls().await;
    let pool = state.db.clone().unwrap();
    // Revocation happens right after the connector returns, before the run is
    // marked succeeded, so it must already be visible here.
    let tokens = token_rows(&pool, run_id).await;
    assert_eq!(tokens.len(), 1);
    assert!(tokens[0].revoked, "token must be revoked once the run finished");
}

#[tokio::test]
async fn chat_turn_and_compaction_get_scoped_tokens() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_gateway("mcp/chat_tool_call").await;
    let pool = state.db.clone().unwrap();
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Backend", &cookie, &csrf)
            .await;

    // Chat turn.
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            &format!(r#"{{"agentId":"{agent_id}","boardId":"{board_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let session_id = common::json_body(res).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"hello"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let chat_run: Uuid = common::json_body(res).await["runId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    wait_for_run_status(&pool, chat_run, "succeeded").await;
    let tokens = token_rows(&pool, chat_run).await;
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].profile, "conversation");
    assert_eq!(tokens[0].chat_session_id, Some(session_id.parse().unwrap()));
    assert_eq!(tokens[0].board_id, Some(board_id.parse().unwrap()));
    assert!(tokens[0].revoked);
    assert_eq!(
        tool_call_rows(&pool, chat_run).await,
        vec![("board_agents".into(), "core".into(), "ok".into())]
    );

    // Knowledge compaction.
    std::env::set_var("MOCK_AGENT_RESPONSE", "mcp/compact_tool_call");
    let admin_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'admin@localhost'")
        .fetch_one(&pool)
        .await
        .unwrap();
    WorkspaceSettingsService::new(&pool)
        .set_knowledge_compaction_agent(Some(agent_id.parse().unwrap()), admin_id)
        .await
        .unwrap();
    let ticket_id: Uuid = common::create_test_ticket(&app, &board_id, &cookie, &csrf)
        .await
        .parse()
        .unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_id)
        .execute(&pool)
        .await
        .unwrap();
    let batch = KnowledgeCompactionService::new(&pool, &state.config.knowledge)
        .start_manual(admin_id)
        .await
        .expect("start batch");
    let compaction_run = batch.run_id.expect("batch run");
    wait_for_run_status(&pool, compaction_run, "succeeded").await;
    let tokens = token_rows(&pool, compaction_run).await;
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].profile, "knowledge_compaction");
    assert_eq!(tokens[0].compaction_ticket_ids, vec![ticket_id]);
    assert!(tokens[0].revoked);
}
