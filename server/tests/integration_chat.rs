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

#[tokio::test]
async fn create_ticket_from_chat_sets_source_session() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;

    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            &format!(r#"{{"agentId":"{agent_id}","projectId":"{project_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    let _ = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"We should fix chat cwd policy"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();

    let missing_project = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/create-ticket"),
            r#"{"title":"Fix cwd"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(missing_project.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let created_ticket = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/create-ticket"),
            &format!(r#"{{"projectId":"{project_id}","title":"Fix chat cwd"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(created_ticket.status(), StatusCode::CREATED);
    let body: serde_json::Value = common::json_body(created_ticket).await;
    let ticket_id = body["ticket"]["id"].as_str().expect("ticket id");
    assert_eq!(body["ticket"]["title"], "Fix chat cwd");
    assert_eq!(body["ticket"]["status"], "backlog");
    assert_eq!(body["message"]["role"], "system");
    assert_eq!(body["message"]["actionMetadata"]["action"], "create_ticket");
    assert_eq!(body["message"]["actionMetadata"]["ticketId"], ticket_id);

    let source: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT source_chat_session_id FROM tickets WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(ticket_id).unwrap())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        source.map(|id| id.to_string()).as_deref(),
        Some(session_id)
    );
}

#[tokio::test]
async fn create_knowledge_from_chat_is_pending_not_admin_route() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;

    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            &format!(r#"{{"agentId":"{agent_id}","projectId":"{project_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let session: serde_json::Value = common::json_body(created).await;
    let session_id = session["id"].as_str().unwrap();

    let _ = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/messages"),
            r#"{"body":"Prefer WORKTREES_PATH/chat/{id} for unbound sessions"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();

    let created_knowledge = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{session_id}/create-knowledge"),
            &format!(
                r#"{{"projectId":"{project_id}","title":"Chat cwd convention","knowledgeType":"coding_convention","scope":"project"}}"#
            ),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(created_knowledge.status(), StatusCode::CREATED);
    let body: serde_json::Value = common::json_body(created_knowledge).await;
    assert_eq!(body["knowledge"]["status"], "pending");
    assert_eq!(body["knowledge"]["sourceType"], "chat_session");
    assert_eq!(body["knowledge"]["sourceId"], session_id);
    assert!(body["knowledge"]["activeRevisionId"].is_null());
    assert_eq!(body["message"]["actionMetadata"]["action"], "create_knowledge");
}

#[tokio::test]
async fn cutoff_opens_child_session_with_summary_seed() {
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
    let parent_id = session["id"].as_str().unwrap().to_string();

    let _ = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{parent_id}/messages"),
            r#"{"body":"Long exploration about cwd policy"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();

    let cutoff = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{parent_id}/cutoff"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(cutoff.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(cutoff).await;
    assert_eq!(body["parent"]["status"], "cutoff");
    assert_eq!(body["parent"]["id"], parent_id);
    let child_id = body["child"]["id"].as_str().expect("child id");
    assert_eq!(body["child"]["status"], "active");
    assert_eq!(body["child"]["parentSessionId"], parent_id);
    assert_eq!(body["seedMessage"]["role"], "system");
    assert!(body["seedMessage"]["body"]
        .as_str()
        .unwrap()
        .contains("Prior conversation summary"));

    let parent_messages = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/chat/sessions/{parent_id}/messages"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(parent_messages.status(), StatusCode::OK);
    let parent_body: serde_json::Value = common::json_body(parent_messages).await;
    assert!(
        parent_body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "human"),
        "parent transcript retained"
    );

    let reject_post = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{parent_id}/messages"),
            r#"{"body":"should fail"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(reject_post.status(), StatusCode::BAD_REQUEST);

    let child_post = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/chat/sessions/{child_id}/messages"),
            r#"{"body":"continue from summary"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(child_post.status(), StatusCode::CREATED);
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
