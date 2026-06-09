mod common;

use axum::http::{header, StatusCode};
use coppice_server::services::artifact_service::{ArtifactService, RunArtifactPaths};
use futures_util::StreamExt;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
};
use tower::ServiceExt;
use uuid::Uuid;

async fn connect_ws(url: &str, cookie: &str) -> tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
> {
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        header::COOKIE,
        HeaderValue::from_str(cookie).unwrap(),
    );
    connect_async(request).await.expect("ws connect").0
}

async fn create_opencode_agent(app: &axum::Router, cookie: &str, csrf: &str) -> String {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/agents",
            r#"{"name":"OpenCode Bot","role":"Developer","systemPrompt":"You are a developer","connector":"opencode"}"#,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = common::json_body(res).await;
    body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn live_ws_replays_snapshot_when_no_registry() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    coppice_server::workers::job_worker::spawn_workers(state.clone());
    let addr = common::spawn_test_server(app.clone()).await;

    let agent_id = create_opencode_agent(&app, &cookie, &csrf).await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;

    let run_id = Uuid::new_v4();
    let session_id = "test-session-abc";
    let worktree_path = local_path.display().to_string();
    let pool = state.db.as_ref().unwrap();

    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
            worktree_path, session_id, started_at
        )
        VALUES ($1, $2, $3, 'work_on_ticket', 'running', 'permissive-default', $4, $5, now())
        "#,
    )
    .bind(run_id)
    .bind(Uuid::parse_str(&ticket_id).unwrap())
    .bind(Uuid::parse_str(&agent_id).unwrap())
    .bind(&worktree_path)
    .bind(session_id)
    .execute(pool)
    .await
    .expect("insert running opencode run");

    let snapshot = serde_json::json!({
        "sessionId": session_id,
        "messages": [{"id": "msg-1", "role": "assistant", "sessionID": session_id}],
        "parts": {"msg-1": [{"type": "text", "text": "hello"}]}
    });
    let paths = RunArtifactPaths::new("/tmp/coppice-test-artifacts", &run_id.to_string());
    ArtifactService::write_session_snapshot(&paths, &snapshot).expect("write snapshot");

    assert!(
        state.run_streams.get(run_id).is_none(),
        "registry should be empty for recovery test"
    );

    let ws_url = format!("ws://{addr}/ws/agent-runs/{run_id}/live");
    let ws = connect_ws(&ws_url, &cookie).await;
    let (_, mut read) = ws.split();

    let first = read
        .next()
        .await
        .expect("first ws message")
        .expect("ws ok");
    let first_text = match first {
        Message::Text(text) => text,
        other => panic!("expected text message, got {other:?}"),
    };
    let first_json: serde_json::Value = serde_json::from_str(&first_text).unwrap();
    assert_eq!(first_json["type"], "snapshot");
    assert_eq!(first_json["sessionId"], session_id);
    assert_eq!(first_json["messages"][0]["id"], "msg-1");

    let end_msg = read
        .next()
        .await
        .expect("end ws message")
        .expect("ws ok");
    let end_text = match end_msg {
        Message::Text(text) => text,
        other => panic!("expected text message, got {other:?}"),
    };
    let end_json: serde_json::Value = serde_json::from_str(&end_text).unwrap();
    assert_eq!(end_json["type"], "end");
    assert_eq!(end_json["status"], "running");
    assert_eq!(end_json["recoverable"], false);
    assert_eq!(
        end_json["reason"].as_str().unwrap(),
        "opencode serve not available"
    );
}
