mod common;

use axum::Router;
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::mcp::token::{NewRunToolScope, TokenService};
use coppice_server::services::run_service::RunService;
use coppice_server::AppState;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
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
