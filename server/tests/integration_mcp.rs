mod common;

use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::mcp::token::{NewRunToolScope, TokenService};
use coppice_server::services::run_service::RunService;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

struct RunFixture {
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
