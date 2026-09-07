mod common;

use axum::http::StatusCode;
use std::time::Duration;
use tower::ServiceExt;

#[tokio::test]
async fn create_session_requires_agent_project_optional() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;

    let missing_agent = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            r#"{"projectId":null}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(missing_agent.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            &format!(r#"{{"agentId":"{agent_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body: serde_json::Value = common::json_body(created).await;
    assert_eq!(body["agentId"], agent_id);
    assert!(body["projectId"].is_null());
    assert_eq!(body["status"], "active");
}

#[tokio::test]
async fn post_message_runs_mock_chat_turn_and_persists_reply() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, env) =
        common::bootstrap_and_login_with_workers("backend_engineer/chat_turn").await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            &format!(r#"{{"agentId":"{agent_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"What is the cwd policy?"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);
    let post_body: serde_json::Value = common::json_body(posted).await;
    let run_id = post_body["runId"].as_str().expect("runId");
    assert_eq!(post_body["message"]["role"], "human");
    assert_eq!(post_body["message"]["agentRunId"], run_id);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut agent_message = None;
    while tokio::time::Instant::now() < deadline {
        let list = app
            .clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/chat/sessions/{session_id}/messages"),
                "",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body: serde_json::Value = common::json_body(list).await;
        let messages = body["messages"].as_array().unwrap();
        if let Some(msg) = messages.iter().find(|m| m["role"] == "agent") {
            agent_message = Some(msg.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let agent_message = agent_message.expect("agent message persisted");
    assert!(agent_message["body"]
        .as_str()
        .unwrap()
        .contains("Mock chat reply"));
    assert_eq!(agent_message["agentRunId"], run_id);

    let run = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/agent-runs/{run_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(run.status(), StatusCode::OK);
    let run_body: serde_json::Value = common::json_body(run).await;
    assert!(run_body["run"]["ticketId"].is_null());
    assert_eq!(run_body["run"]["chatSessionId"], session_id);
    assert_eq!(run_body["run"]["status"], "succeeded");
    let worktree = run_body["run"]["worktreePath"].as_str().unwrap();
    assert!(
        worktree.contains("/chat/") && worktree.contains(session_id),
        "expected managed chat cwd, got {worktree}"
    );
    assert_ne!(worktree, "/");
    assert!(!worktree.contains("TICKET-"));
    assert!(
        std::path::Path::new(worktree).starts_with(env.worktrees.path()),
        "cwd should be under WORKTREES_PATH"
    );
}

#[tokio::test]
async fn conversation_profile_refuses_write_capable_connectors() {
    use coppice_server::domain::context_profile::ContextProfile;
    use coppice_server::providers::{
        cursor::CursorProvider, AgentProvider, AgentRunInput, ProviderError,
    };
    use coppice_config::CursorProviderConfig;

    let provider = CursorProvider::new(CursorProviderConfig {
        enabled: true,
        command: "agent".into(),
        model_providers: vec![],
        run_timeout_secs: 30,
    });
    let err = provider
        .run(AgentRunInput {
            agent_id: "a".into(),
            agent_key: "backend_engineer".into(),
            agent_role: "BE".into(),
            job_type: "chat_turn".into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::Conversation,
            context_path: "/tmp/.agent/context.md".into(),
            run_id: None,
            artifacts_dir: None,
            stream: None,
            cancel_rx: None,
            model_provider: None,
            model: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: true,
        })
        .await
        .expect_err("cursor must fail closed for read-only chat");
    assert!(matches!(err, ProviderError::InvalidInput(_)));
    assert!(err.to_string().contains("read-only"));
}
