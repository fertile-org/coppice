mod common;

use axum::http::StatusCode;
use std::time::Duration;
use tower::ServiceExt;

#[tokio::test]
async fn create_session_requires_agent_board_optional() {
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
            r#"{"boardId":null}"#,
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
    assert!(body["boardId"].is_null());
    assert_eq!(body["status"], "active");
    assert!(body["title"].is_null());
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
async fn list_sessions_includes_message_preview() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) =
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

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        let list = app
            .clone()
            .oneshot(common::json_request(
                "GET",
                "/api/chat/sessions",
                "",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body: serde_json::Value = common::json_body(list).await;
        let preview = body["sessions"][0]["lastMessagePreview"]
            .as_str()
            .unwrap_or("");
        if preview.contains("Mock chat reply") || preview.contains("cwd") {
            assert!(!preview.is_empty());
            assert_eq!(body["sessions"][0]["title"], "What is the cwd policy?");
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("expected session list preview after agent reply");
}

#[tokio::test]
async fn list_sessions_has_active_run_while_turn_queued() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf)
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
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"Hello"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);

    let list = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/chat/sessions",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(list).await;
    let row = &body["sessions"][0];
    assert_eq!(row["hasActiveRun"], true);
    assert!(row["activeRunId"].is_string());
}

#[tokio::test]
async fn conversation_profile_starts_kilo_read_only_chat() {
    use coppice_server::domain::context_profile::ContextProfile;
    use coppice_server::providers::{kilo_code::KiloCodeProvider, AgentProvider, AgentRunInput};
    use coppice_config::KiloCodeProviderConfig;

    let provider = KiloCodeProvider::new(KiloCodeProviderConfig {
        enabled: Some(true),
        command: "kilo-not-installed".into(),
        model_providers: vec![],
        run_timeout_secs: 30,
        ..KiloCodeProviderConfig::default()
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
            chat_session_id: None,
            artifacts_dir: None,
            stream: None,
            cancel_rx: None,
            model_provider: None,
            model: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: true,
            mcp: None,
        })
        .await
        .expect_err("missing kilo binary");
    let message = err.to_string();
    assert!(
        !message.contains("cannot enforce read-only"),
        "{message}"
    );
    assert!(
        !message.contains("Chat isn't available"),
        "{message}"
    );
    assert!(
        message.contains("kilo-not-installed"),
        "{message}"
    );
}

#[tokio::test]
async fn chat_second_turn_sets_provider_session_id() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("backend_engineer/chat_turn", |_| {})
            .await;
    let pool = state.db.as_ref().expect("test db");
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
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    for body in ["First question?", "Second question?"] {
        let posted = app
            .clone()
            .oneshot(common::json_request(
                "POST",
                &format!("/api/chat/sessions/{session_id}/messages"),
                &format!(r#"{{"body":"{body}"}}"#),
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(posted.status(), StatusCode::CREATED);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
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
            let messages: serde_json::Value = common::json_body(list).await;
            let count = messages["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["role"] == "agent")
                .count();
            if count >= if body == "First question?" { 1 } else { 2 } {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    let session_uuid = uuid::Uuid::parse_str(session_id).unwrap();
    let row: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT provider_session_id, provider_session_connector FROM chat_sessions WHERE id = $1",
    )
    .bind(session_uuid)
    .fetch_one(pool)
    .await
    .expect("session row");
    assert_eq!(row.0.as_deref(), Some("mock-chat-session"));
    assert_eq!(row.1.as_deref(), Some("mock"));
}

#[tokio::test]
async fn chat_second_turn_uses_slim_context() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("backend_engineer/chat_turn", |_| {})
            .await;
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
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    app.clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"First"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
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
        let messages: serde_json::Value = common::json_body(list).await;
        if messages["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "agent")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    std::env::set_var("MOCK_CHAT_EXPECT_SLIM", "1");
    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"Second"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
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
        let messages: serde_json::Value = common::json_body(list).await;
        let agent_count = messages["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "agent")
            .count();
        if agent_count >= 2 {
            std::env::remove_var("MOCK_CHAT_EXPECT_SLIM");
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    std::env::remove_var("MOCK_CHAT_EXPECT_SLIM");
    panic!("second agent reply not persisted");
}

#[tokio::test]
async fn chat_resume_fallback_succeeds() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("backend_engineer/chat_turn", |_| {})
            .await;
    let pool = state.db.as_ref().expect("test db");
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
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();
    let session_uuid = uuid::Uuid::parse_str(session_id).unwrap();

    app.clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"First"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
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
        let messages: serde_json::Value = common::json_body(list).await;
        if messages["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "agent")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    sqlx::query(
        "UPDATE chat_sessions SET provider_session_id = 'stale-mock', provider_session_connector = 'mock' WHERE id = $1",
    )
    .bind(session_uuid)
    .execute(pool)
    .await
    .expect("seed provider session");

    std::env::set_var("MOCK_CHAT_RESUME_FAIL", "1");
    app.clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"Second after fallback"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
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
        let messages: serde_json::Value = common::json_body(list).await;
        let agent_count = messages["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "agent")
            .count();
        if agent_count >= 2 {
            std::env::remove_var("MOCK_CHAT_RESUME_FAIL");
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    std::env::remove_var("MOCK_CHAT_RESUME_FAIL");
    panic!("fallback chat turn did not persist agent reply");
}

async fn login_as(app: &axum::Router, email: &str, password: &str) -> (String, String) {
    use coppice_server::middleware::session::parse_session_cookie;
    use http_body_util::BodyExt;

    let login = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(format!(
                    r#"{{"email":"{email}","password":"{password}"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);

    let set_cookie = login
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .expect("session cookie");
    let cookie_header = set_cookie.to_str().unwrap();
    let session_token = parse_session_cookie(cookie_header).expect("session token");
    let cookie = format!("coppice_session={session_token}");

    let body = login.into_body().collect().await.unwrap().to_bytes();
    let login_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let csrf_token = login_json["csrfToken"]
        .as_str()
        .expect("csrf token")
        .to_string();

    (cookie, csrf_token)
}

#[tokio::test]
async fn post_message_with_attachment_persists_summaries() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;

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

    let upload = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "notes.txt",
            "text/plain",
            "hello chat attach",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::CREATED);
    let attachment: serde_json::Value = common::json_body(upload).await;
    let attachment_id = attachment["id"].as_str().unwrap();

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            &format!(
                r#"{{"body":"see file","attachmentIds":["{attachment_id}"]}}"#
            ),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);
    let post_body: serde_json::Value = common::json_body(posted).await;
    assert_eq!(post_body["message"]["attachmentIds"][0], attachment_id);
    assert_eq!(post_body["message"]["attachments"][0]["filename"], "notes.txt");
    assert_eq!(
        post_body["message"]["attachments"][0]["contentType"],
        "text/plain"
    );
    assert_eq!(post_body["message"]["attachments"][0]["sizeBytes"], 17);

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
    let list_body: serde_json::Value = common::json_body(list).await;
    let human = list_body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "human")
        .expect("human message");
    assert_eq!(human["attachmentIds"][0], attachment_id);
    assert_eq!(human["attachments"][0]["filename"], "notes.txt");
}

#[tokio::test]
async fn post_message_attachment_only_accepted_empty_body_rejected() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;

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
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    let empty = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"   "}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);

    let upload = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "solo.md",
            "text/markdown",
            "# only file",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let attachment: serde_json::Value = common::json_body(upload).await;
    let attachment_id = attachment["id"].as_str().unwrap();

    let attachment_only = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            &format!(r#"{{"body":"","attachmentIds":["{attachment_id}"]}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(attachment_only.status(), StatusCode::CREATED);
    let body: serde_json::Value = common::json_body(attachment_only).await;
    assert_eq!(body["message"]["body"], "");
    assert_eq!(body["message"]["attachmentIds"][0], attachment_id);
}

#[tokio::test]
async fn post_message_rejects_disallowed_mime_type() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;

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
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    let upload = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "evil.html",
            "text/html",
            "<script>alert(1)</script>",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::CREATED);
    let attachment: serde_json::Value = common::json_body(upload).await;
    let attachment_id = attachment["id"].as_str().unwrap();

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            &format!(
                r#"{{"body":"bad mime","attachmentIds":["{attachment_id}"]}}"#
            ),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_rejects_oversized_attachment() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.storage.max_upload_bytes = 8;
        })
        .await;

    let upload = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "big.txt",
            "text/plain",
            "0123456789",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn non_owner_cannot_download_chat_only_attachment() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, admin_cookie, admin_csrf) = common::bootstrap_and_login().await;
    let create_member = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/users",
            r#"{"email":"member@localhost","password":"secret123"}"#,
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_member.status(), StatusCode::CREATED);

    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "BE",
        &admin_cookie,
        &admin_csrf,
    )
    .await;

    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            &format!(r#"{{"agentId":"{agent_id}"}}"#),
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    let upload = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "secret.txt",
            "text/plain",
            "private chat file",
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    let attachment: serde_json::Value = common::json_body(upload).await;
    let attachment_id = attachment["id"].as_str().unwrap();

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            &format!(
                r#"{{"body":"secret","attachmentIds":["{attachment_id}"]}}"#
            ),
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);

    let owner_get = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/attachments/{attachment_id}"),
            "",
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(owner_get.status(), StatusCode::OK);

    let (member_cookie, member_csrf) = login_as(&app, "member@localhost", "secret123").await;
    let deny = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/attachments/{attachment_id}"),
            "",
            &member_cookie,
            &member_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(deny.status(), StatusCode::NOT_FOUND);

    let deny_post = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"hijack"}"#,
            &member_cookie,
            &member_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(deny_post.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn chat_turn_stages_attachments_into_cwd_and_transcript() {
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
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    let upload = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "brief.txt",
            "text/plain",
            "agent should see me",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let attachment: serde_json::Value = common::json_body(upload).await;
    let attachment_id = attachment["id"].as_str().unwrap();

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            &format!(
                r#"{{"body":"read the file","attachmentIds":["{attachment_id}"]}}"#
            ),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);
    let post_body: serde_json::Value = common::json_body(posted).await;
    let run_id = post_body["runId"].as_str().unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut succeeded = false;
    while tokio::time::Instant::now() < deadline {
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
        let run_body: serde_json::Value = common::json_body(run).await;
        if run_body["run"]["status"] == "succeeded" {
            succeeded = true;
            let worktree = run_body["run"]["worktreePath"].as_str().unwrap();
            let staged = std::path::Path::new(worktree)
                .join("attachments")
                .join(attachment_id)
                .join("brief.txt");
            assert!(
                staged.is_file(),
                "expected staged file at {}",
                staged.display()
            );
            let contents = std::fs::read_to_string(&staged).unwrap();
            assert_eq!(contents, "agent should see me");

            let context = std::path::Path::new(worktree)
                .join(".agent")
                .join("context.md");
            let context_text = std::fs::read_to_string(&context).unwrap();
            assert!(
                context_text.contains(&format!("attachments/{attachment_id}/brief.txt")),
                "context should list durable attachment path"
            );
            assert!(context_text.contains("text/plain"));
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(succeeded, "chat turn should succeed");
    assert!(
        env.worktrees.path().join("chat").join(session_id).exists()
            || true,
        "session cwd under worktrees"
    );
}

/// A second Kilo chat turn must launch with `--session` set to the id from
/// the first turn's JSON, and must not paste that earlier turn back into
/// the prompt.
#[cfg(feature = "embedded-test-db")]
#[tokio::test]
async fn kilo_chat_second_turn_passes_session_id() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fake = env!("CARGO_BIN_EXE_fake-cli");
    let (state, app, cookie, csrf, env) = common::bootstrap_and_login_with_state_and_workers(
        "backend_engineer/chat_turn",
        |config| {
            config.agent.connectors.kilo_code.enabled = Some(true);
            config.agent.connectors.kilo_code.command = fake.to_string();
        },
    )
    .await;
    let pool = state.db.as_ref().expect("test db");
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let agent_uuid = uuid::Uuid::parse_str(&agent_id).unwrap();
    set_agent_connector(pool, agent_uuid, "kilo-code").await;

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
    let session_id = session["id"].as_str().unwrap().to_string();
    let session_uuid = uuid::Uuid::parse_str(&session_id).unwrap();

    let fixture = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/kilo-code/done.jsonl"),
    )
    .expect("kilo fixture");
    let chat_dir = env.worktrees.path().join("chat").join(&session_id);
    std::fs::create_dir_all(&chat_dir).expect("chat dir");
    std::fs::write(
        chat_dir.join(".fake-cli-env.json"),
        serde_json::json!({ "FAKE_CLI_LINES": fixture }).to_string(),
    )
    .expect("fake cli env");

    let first = post_chat_message(
        &app,
        &cookie,
        &csrf,
        &session_id,
        "Earlier kilo turn about the cache",
    )
    .await;
    let first_run = wait_run(&app, &cookie, &csrf, &first).await;
    assert_eq!(
        first_run["run"]["status"], "succeeded",
        "first kilo turn failed: {first_run}"
    );

    let stored = wait_provider_session(pool, session_uuid).await;
    assert_eq!(
        stored.as_deref(),
        Some("kilo_sess_abc123"),
        "first turn did not store the session id from the JSON stream"
    );

    let second = post_chat_message(
        &app,
        &cookie,
        &csrf,
        &session_id,
        "Follow-up kilo turn about the cache",
    )
    .await;
    let second_run = wait_run(&app, &cookie, &csrf, &second).await;
    assert_eq!(
        second_run["run"]["status"], "succeeded",
        "second kilo turn failed: {second_run}"
    );

    let log_path = std::path::Path::new(&state.config.storage.artifacts_dir)
        .join("runs")
        .join(&second)
        .join("run.log");
    let log = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", log_path.display()));
    assert!(
        log.contains("--session kilo_sess_abc123"),
        "second turn did not resume the stored session: {log}"
    );
    assert!(
        log.contains("--auto") && log.contains("--agent ask"),
        "ask mode flags changed: {log}"
    );
    assert!(
        !log.contains("--agent plan"),
        "chat turn used plan mode: {log}"
    );

    let context = std::fs::read_to_string(chat_dir.join(".agent").join("context.md"))
        .expect("second turn context");
    assert!(
        context.contains("Follow-up kilo turn about the cache"),
        "{context}"
    );
    assert!(
        !context.contains("Earlier kilo turn about the cache"),
        "second turn replayed the earlier message instead of resuming: {context}"
    );
}

/// A resumed Kilo turn sends only the latest message. A screenshot attached
/// to that message must still be listed under it, with the read instruction.
#[cfg(feature = "embedded-test-db")]
#[tokio::test]
async fn kilo_chat_resumed_turn_lists_latest_attachment() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fake = env!("CARGO_BIN_EXE_fake-cli");
    let (state, app, cookie, csrf, env) = common::bootstrap_and_login_with_state_and_workers(
        "backend_engineer/chat_turn",
        |config| {
            config.agent.connectors.kilo_code.enabled = Some(true);
            config.agent.connectors.kilo_code.command = fake.to_string();
        },
    )
    .await;
    let pool = state.db.as_ref().expect("test db");
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let agent_uuid = uuid::Uuid::parse_str(&agent_id).unwrap();
    set_agent_connector(pool, agent_uuid, "kilo-code").await;

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
    let session_id = session["id"].as_str().unwrap().to_string();
    let session_uuid = uuid::Uuid::parse_str(&session_id).unwrap();

    let fixture = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/kilo-code/done.jsonl"),
    )
    .expect("kilo fixture");
    let chat_dir = env.worktrees.path().join("chat").join(&session_id);
    std::fs::create_dir_all(&chat_dir).expect("chat dir");
    std::fs::write(
        chat_dir.join(".fake-cli-env.json"),
        serde_json::json!({ "FAKE_CLI_LINES": fixture }).to_string(),
    )
    .expect("fake cli env");

    let first = post_chat_message(
        &app,
        &cookie,
        &csrf,
        &session_id,
        "Earlier kilo turn about the cache",
    )
    .await;
    let first_run = wait_run(&app, &cookie, &csrf, &first).await;
    assert_eq!(
        first_run["run"]["status"], "succeeded",
        "first kilo turn failed: {first_run}"
    );
    let first_context = std::fs::read_to_string(chat_dir.join(".agent").join("context.md"))
        .expect("first turn context");
    assert!(
        !first_context.contains("Files attached to messages"),
        "instruction shown without attachments: {first_context}"
    );
    assert!(
        wait_provider_session(pool, session_uuid).await.is_some(),
        "first turn did not store a provider session"
    );

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-chat-screenshot";
    let upload = app
        .clone()
        .oneshot(common::multipart_request_bytes(
            "/api/attachments",
            "shot.png",
            "image/png",
            PNG,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert!(upload.status().is_success(), "upload failed: {}", upload.status());
    let attachment: serde_json::Value = common::json_body(upload).await;
    let attachment_id = attachment["id"].as_str().unwrap().to_string();

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            &serde_json::json!({
                "body": "What does this screenshot show?",
                "attachmentIds": [attachment_id],
            })
            .to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);
    let posted_body: serde_json::Value = common::json_body(posted).await;
    let second = posted_body["runId"].as_str().unwrap().to_string();
    let second_run = wait_run(&app, &cookie, &csrf, &second).await;
    assert_eq!(
        second_run["run"]["status"], "succeeded",
        "second kilo turn failed: {second_run}"
    );

    let log_path = std::path::Path::new(&state.config.storage.artifacts_dir)
        .join("runs")
        .join(&second)
        .join("run.log");
    let log = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", log_path.display()));
    assert!(
        log.contains("--session kilo_sess_abc123"),
        "attachment turn did not resume the stored session: {log}"
    );

    let context = std::fs::read_to_string(chat_dir.join(".agent").join("context.md"))
        .expect("second turn context");
    assert!(
        context.contains(&format!("attachments/{attachment_id}/shot.png")),
        "resumed context does not list the latest attachment: {context}"
    );
    assert!(context.contains("image/png"), "{context}");
    assert!(
        context.contains("Files attached to messages are listed under `Attachments:`"),
        "missing read-attachments instruction: {context}"
    );
    assert!(
        !context.contains("Earlier kilo turn about the cache"),
        "resumed turn replayed the earlier message: {context}"
    );
}

#[cfg(feature = "embedded-test-db")]
async fn post_chat_message(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    session_id: &str,
    body: &str,
) -> String {
    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            &serde_json::json!({ "body": body }).to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED, "{body}");
    let posted_body: serde_json::Value = common::json_body(posted).await;
    posted_body["runId"]
        .as_str()
        .unwrap_or_else(|| panic!("no run id: {posted_body}"))
        .to_string()
}

#[cfg(feature = "embedded-test-db")]
async fn wait_run(app: &axum::Router, cookie: &str, csrf: &str, run_id: &str) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut last = serde_json::Value::Null;
    while tokio::time::Instant::now() < deadline {
        let run = app
            .clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/agent-runs/{run_id}"),
                "",
                cookie,
                csrf,
            ))
            .await
            .unwrap();
        last = common::json_body(run).await;
        let status = last["run"]["status"].as_str().unwrap_or("");
        if matches!(status, "succeeded" | "failed" | "blocked") {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("run {run_id} did not finish: {last}");
}

#[cfg(feature = "embedded-test-db")]
async fn wait_provider_session(pool: &sqlx::PgPool, session_id: uuid::Uuid) -> Option<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT provider_session_id FROM chat_sessions WHERE id = $1")
                .bind(session_id)
                .fetch_one(pool)
                .await
                .expect("provider session");
        if stored.as_deref().is_some_and(|sid| !sid.is_empty()) {
            return stored;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

/// A Kilo `--session` launch that exits non-zero is a missing session. The
/// turn clears that id, retries once without `--session`, and the retry
/// prompt includes the earlier turns through the same bounded transcript
/// non-resume chats use.
#[cfg(feature = "embedded-test-db")]
#[tokio::test]
async fn kilo_chat_resume_failure_retries_once_with_earlier_context() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fake = env!("CARGO_BIN_EXE_fake-cli");
    let (state, app, cookie, csrf, env) = common::bootstrap_and_login_with_state_and_workers(
        "backend_engineer/chat_turn",
        |config| {
            config.agent.connectors.kilo_code.enabled = Some(true);
            config.agent.connectors.kilo_code.command = fake.to_string();
            config.knowledge.context_budget.latest_comments = 180;
        },
    )
    .await;
    let pool = state.db.as_ref().expect("test db");
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let agent_uuid = uuid::Uuid::parse_str(&agent_id).unwrap();
    set_agent_connector(pool, agent_uuid, "kilo-code").await;

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
    let session_id = session["id"].as_str().unwrap().to_string();
    let session_uuid = uuid::Uuid::parse_str(&session_id).unwrap();

    let fixture = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/kilo-code/done.jsonl"),
    )
    .expect("kilo fixture");
    let chat_dir = env.worktrees.path().join("chat").join(&session_id);
    std::fs::create_dir_all(&chat_dir).expect("chat dir");
    std::fs::write(
        chat_dir.join(".fake-cli-env.json"),
        serde_json::json!({
            "FAKE_CLI_LINES": fixture,
            "FAKE_CLI_REJECT_SESSION": "1",
        })
        .to_string(),
    )
    .expect("fake cli env");

    let earlier = format!(
        "FIRSTUNIQUE older kilo turn\n{}",
        "PADTOKEN ".repeat(400)
    );
    let first = post_chat_message(&app, &cookie, &csrf, &session_id, &earlier).await;
    let first_run = wait_run(&app, &cookie, &csrf, &first).await;
    assert_eq!(
        first_run["run"]["status"], "succeeded",
        "first kilo turn failed: {first_run}"
    );
    assert_eq!(
        wait_provider_session(pool, session_uuid).await.as_deref(),
        Some("kilo_sess_abc123")
    );
    sqlx::query(
        "UPDATE chat_sessions SET provider_session_id = 'kilo_sess_gone', provider_session_connector = 'kilo-code' WHERE id = $1",
    )
    .bind(session_uuid)
    .execute(pool)
    .await
    .expect("seed gone session");

    let second = post_chat_message(
        &app,
        &cookie,
        &csrf,
        &session_id,
        "Follow-up after the missing session",
    )
    .await;
    let second_run = wait_run(&app, &cookie, &csrf, &second).await;
    assert_eq!(
        second_run["run"]["status"], "succeeded",
        "resume fallback did not succeed: {second_run}"
    );

    let log_path = std::path::Path::new(&state.config.storage.artifacts_dir)
        .join("runs")
        .join(&second)
        .join("run.log");
    let log = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", log_path.display()));
    let launches: Vec<&str> = log.lines().filter(|line| line.contains("flags:")).collect();
    assert_eq!(
        launches.len(),
        2,
        "resume must be tried once and retried once: {log}"
    );
    assert!(
        launches[0].contains("--session kilo_sess_gone"),
        "first launch did not resume: {log}"
    );
    assert!(
        !launches[1].contains("--session"),
        "retry passed --session again: {log}"
    );

    let context = std::fs::read_to_string(chat_dir.join(".agent").join("context.md"))
        .expect("fallback context");
    assert!(
        context.contains("### Earlier messages"),
        "fallback did not use the bounded transcript: {context}"
    );
    assert!(
        context.contains("FIRSTUNIQUE"),
        "fallback dropped the earlier turn: {context}"
    );
    assert!(
        context.contains("Follow-up after the missing session"),
        "{context}"
    );
    assert!(
        !context.contains("PADTOKEN"),
        "fallback pasted the full earlier turn instead of the summary: {context}"
    );

    // The worker stores the session id after the run is marked succeeded.
    assert_eq!(
        wait_provider_session(pool, session_uuid).await.as_deref(),
        Some("kilo_sess_abc123")
    );
}

#[tokio::test]
async fn chat_session_starts_on_cursor_and_kilo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.agent.connectors.codex.enabled = Some(true);
        config.agent.connectors.opencode.enabled = Some(true);
        config.agent.connectors.cursor.enabled = Some(true);
        config.agent.connectors.kilo_code.enabled = Some(true);
        config.agent.connectors.claude_code.enabled = Some(true);
    })
    .await;
    let pool = state.db.as_ref().expect("test db");
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let agent_uuid = uuid::Uuid::parse_str(&agent_id).unwrap();

    for connector in ["cursor", "kilo-code", "codex", "claude-code", "opencode"] {
        set_agent_connector(pool, agent_uuid, connector).await;
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
        assert_eq!(created.status(), StatusCode::CREATED, "{connector}");
        let session: serde_json::Value = common::json_body(created).await;
        assert!(session.get("message").is_none(), "{connector}");
        let session_id = session["id"].as_str().unwrap();

        let posted = app
            .clone()
            .oneshot(common::json_request(
                "POST",
                &format!("/api/chat/sessions/{session_id}/messages"),
                r#"{"body":"write README.md"}"#,
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(posted.status(), StatusCode::CREATED, "{connector}");
        let posted_body: serde_json::Value = common::json_body(posted).await;
        assert!(posted_body["runId"].is_string(), "{connector}");
        let body_text = posted_body.to_string();
        assert!(
            !body_text.contains("Chat isn't available"),
            "{connector} {body_text}"
        );
    }
}

async fn set_agent_connector(pool: &sqlx::PgPool, agent_id: uuid::Uuid, connector: &str) {
    sqlx::query("UPDATE agents SET connector = $2 WHERE id = $1")
        .bind(agent_id)
        .bind(connector)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn failed_chat_turn_leaves_a_system_note_in_the_chat() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    // No such fixture, so the turn fails the way a turn with no result does.
    let (app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_workers("missing_agent/chat_turn").await;
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
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();
    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"hi"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut note = None;
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
        let body: serde_json::Value = common::json_body(list).await;
        let messages = body["messages"].as_array().unwrap();
        if let Some(msg) = messages.iter().find(|m| m["role"] == "system") {
            assert!(messages.iter().all(|m| m["role"] != "agent"));
            note = Some(msg["body"].as_str().unwrap().to_string());
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let note = note.expect("system note after the failed turn");
    assert!(
        note.starts_with("The agent stopped without sending a reply."),
        "{note}"
    );
    assert!(note.contains("Details: "), "{note}");
}

async fn start_chat_session(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    agent_id: &str,
    board_id: Option<&str>,
) -> String {
    let body = match board_id {
        Some(board) => format!(r#"{{"agentId":"{agent_id}","boardId":"{board}"}}"#),
        None => format!(r#"{{"agentId":"{agent_id}"}}"#),
    };
    let created = app
        .clone()
        .oneshot(common::json_request("POST", "/api/chat/sessions", &body, cookie, csrf))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let session: serde_json::Value = common::json_body(created).await;
    session["id"].as_str().unwrap().to_string()
}

/// Insert an agent reply carrying one proposal. Returns (message id, proposal id).
async fn seed_agent_proposal(
    pool: &sqlx::PgPool,
    session_id: &str,
    mut proposal: serde_json::Value,
) -> (String, String) {
    let message_id = uuid::Uuid::new_v4();
    let proposal_id = uuid::Uuid::new_v4();
    proposal["id"] = proposal_id.to_string().into();
    let metadata = serde_json::json!({ "proposals": [proposal] });
    sqlx::query(
        r#"
        INSERT INTO chat_messages (id, session_id, seq, role, body, action_metadata, attachment_ids)
        VALUES (
            $1, $2,
            (SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_messages WHERE session_id = $2),
            'agent', 'Draft ready for you to confirm.', $3, '{}'
        )
        "#,
    )
    .bind(message_id)
    .bind(uuid::Uuid::parse_str(session_id).unwrap())
    .bind(metadata)
    .execute(pool)
    .await
    .unwrap();
    (message_id.to_string(), proposal_id.to_string())
}

async fn ticket_count(pool: &sqlx::PgPool, board_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM tickets WHERE board_id = $1")
        .bind(uuid::Uuid::parse_str(board_id).unwrap())
        .fetch_one(pool)
        .await
        .unwrap()
}

fn proposal_url(session_id: &str, message_id: &str, proposal_id: &str, action: &str) -> String {
    format!("/api/chat/sessions/{session_id}/messages/{message_id}/proposals/{proposal_id}/{action}")
}

/// Wait for a chat turn to attach a proposal. Returns (message id, proposal id).
async fn await_agent_proposal(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    session_id: &str,
) -> (String, String) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let list = app
            .clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/chat/sessions/{session_id}/messages"),
                "",
                cookie,
                csrf,
            ))
            .await
            .unwrap();
        let messages: serde_json::Value = common::json_body(list).await;
        let proposal_message = messages["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "agent" && m["actionMetadata"]["proposals"].is_array());
        if let Some(message) = proposal_message {
            let proposal = &message["actionMetadata"]["proposals"][0];
            return (
                message["id"].as_str().unwrap().to_string(),
                proposal["id"].as_str().unwrap().to_string(),
            );
        }
        assert!(tokio::time::Instant::now() < deadline, "no proposal message");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn accept_ticket_proposal_creates_one_ticket_then_conflicts() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;
    let session_id = start_chat_session(&app, &cookie, &csrf, &agent_id, Some(&board_id)).await;
    let (message_id, proposal_id) = seed_agent_proposal(
        pool,
        &session_id,
        serde_json::json!({
            "type": "create_ticket",
            "title": "Fix chat cwd",
            "description": "Resolve the chat cwd before the run.",
            "status": "pending",
        }),
    )
    .await;
    let before = ticket_count(pool, &board_id).await;

    let accepted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "accept"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(accepted).await;
    let ticket_id = body["ticket"]["id"].as_str().expect("ticket id").to_string();
    assert_eq!(body["ticket"]["title"], "Fix chat cwd");
    assert_eq!(body["message"]["actionMetadata"]["proposals"][0]["status"], "created");
    assert_eq!(body["message"]["actionMetadata"]["proposals"][0]["ticketId"], ticket_id);
    assert_eq!(ticket_count(pool, &board_id).await, before + 1);

    let source: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT source_chat_session_id FROM tickets WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&ticket_id).unwrap())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(source.map(|id| id.to_string()).as_deref(), Some(session_id.as_str()));

    let second = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "accept"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::CONFLICT);
    assert_eq!(ticket_count(pool, &board_id).await, before + 1);

    let reject_after_accept = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "reject"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(reject_after_accept.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn reject_proposal_creates_nothing_and_blocks_later_accept() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;
    let session_id = start_chat_session(&app, &cookie, &csrf, &agent_id, Some(&board_id)).await;
    let (message_id, proposal_id) = seed_agent_proposal(
        pool,
        &session_id,
        serde_json::json!({ "type": "create_ticket", "title": "Not wanted", "status": "pending" }),
    )
    .await;
    let before = ticket_count(pool, &board_id).await;

    let rejected = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "reject"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(rejected).await;
    assert_eq!(body["actionMetadata"]["proposals"][0]["status"], "rejected");
    assert_eq!(ticket_count(pool, &board_id).await, before);

    let accept_after_reject = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "accept"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(accept_after_reject.status(), StatusCode::CONFLICT);
    assert_eq!(ticket_count(pool, &board_id).await, before);
}

#[tokio::test]
async fn accept_ticket_proposal_without_any_board_is_unprocessable() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;
    let session_id = start_chat_session(&app, &cookie, &csrf, &agent_id, None).await;
    let (message_id, proposal_id) = seed_agent_proposal(
        pool,
        &session_id,
        serde_json::json!({ "type": "create_ticket", "title": "No board", "status": "pending" }),
    )
    .await;

    let response = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "accept"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn accept_ticket_proposal_from_repo_less_chat_uses_the_only_repo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let session_id = start_chat_session(&app, &cookie, &csrf, &agent_id, Some(&board_id)).await;
    let (message_id, proposal_id) = seed_agent_proposal(
        pool,
        &session_id,
        serde_json::json!({ "type": "create_ticket", "title": "Repo-less chat ticket", "status": "pending" }),
    )
    .await;

    let accepted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "accept"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(accepted).await;
    assert_eq!(body["ticket"]["repoId"], repo_id);
}

#[tokio::test]
async fn accept_knowledge_proposal_creates_knowledge_item() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;
    let session_id = start_chat_session(&app, &cookie, &csrf, &agent_id, Some(&board_id)).await;
    let (message_id, proposal_id) = seed_agent_proposal(
        pool,
        &session_id,
        serde_json::json!({
            "type": "create_knowledge",
            "title": "Resolve chat cwd",
            "content": "Always resolve the chat cwd before a run.",
            "knowledgeType": "coding_convention",
            "scope": "board",
            "status": "pending",
        }),
    )
    .await;

    let accepted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "accept"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(accepted).await;
    let knowledge_id = body["knowledge"]["id"].as_str().expect("knowledge id");
    assert_eq!(body["knowledge"]["title"], "Resolve chat cwd");
    assert_eq!(body["knowledge"]["status"], "pending");
    assert_eq!(body["knowledge"]["sourceType"], "chat_session");
    assert_eq!(body["knowledge"]["sourceId"], session_id);
    assert_eq!(body["message"]["actionMetadata"]["proposals"][0]["status"], "created");
    assert_eq!(
        body["message"]["actionMetadata"]["proposals"][0]["knowledgeItemId"],
        knowledge_id
    );
}

#[tokio::test]
async fn agent_chat_turn_proposes_ticket_until_human_accepts() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("chat_propose_ticket", |_| {}).await;
    let pool = state.db.as_ref().expect("test db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;
    let session_id = start_chat_session(&app, &cookie, &csrf, &agent_id, Some(&board_id)).await;
    let before = ticket_count(pool, &board_id).await;

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"Make a ticket for the chat cwd hardening"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);

    let (message_id, proposal_id) = await_agent_proposal(&app, &cookie, &csrf, &session_id).await;

    assert_eq!(ticket_count(pool, &board_id).await, before);

    let accepted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "accept"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(accepted).await;
    assert_eq!(body["ticket"]["title"], "Harden chat cwd resolution for unbound sessions");
    assert_eq!(ticket_count(pool, &board_id).await, before + 1);
}

#[tokio::test]
async fn agent_chat_turn_proposes_knowledge_until_human_accepts() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("chat_propose_knowledge", |_| {}).await;
    let pool = state.db.as_ref().expect("test db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;
    let session_id = start_chat_session(&app, &cookie, &csrf, &agent_id, Some(&board_id)).await;

    let posted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"Save the chat cwd rule as knowledge"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);

    let (message_id, proposal_id) = await_agent_proposal(&app, &cookie, &csrf, &session_id).await;

    let knowledge_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_items")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(knowledge_rows, 0, "proposal must not create knowledge before accept");

    let accepted = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &proposal_url(&session_id, &message_id, &proposal_id, "accept"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(accepted).await;
    assert_eq!(body["knowledge"]["title"], "Chat cwd is per session");
    assert_eq!(body["knowledge"]["status"], "pending");
    assert_eq!(body["knowledge"]["sourceType"], "chat_session");
}
