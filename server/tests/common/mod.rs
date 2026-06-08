use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::{db, AppConfig, AppState};
use http_body_util::BodyExt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;
use tower::ServiceExt;

pub static DB_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub async fn db_available() -> bool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://coppice:coppice@localhost:5432/coppice".into());
    db::connect_and_migrate(&database_url).await.is_ok()
}

pub async fn truncate_workspace(pool: &sqlx::PgPool) {
    sqlx::query(
        r#"
        TRUNCATE
            attachments,
            ticket_comments,
            agent_jobs,
            agent_runs,
            tickets,
            repos,
            agents,
            projects,
            sessions,
            users
        RESTART IDENTITY CASCADE
        "#,
    )
    .execute(pool)
    .await
    .expect("truncate workspace tables");
}

#[allow(dead_code)]
pub struct AgentTestEnv {
    pub worktrees: tempfile::TempDir,
}

pub fn create_temp_git_checkout() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&path)
        .output()
        .expect("git init");
    std::fs::write(path.join("README.md"), "# test\n").expect("write readme");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(&path)
        .output()
        .expect("git commit");
    (dir, path)
}

async fn test_state_with_db() -> Arc<AppState> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://coppice:coppice@localhost:5432/coppice".into());
    let pool = db::connect_and_migrate(&database_url)
        .await
        .expect("connect to test database");
    truncate_workspace(&pool).await;

    std::env::set_var(
        "COPPICE_STORAGE__ARTIFACTS_DIR",
        "/tmp/coppice-test-artifacts",
    );
    let config = AppConfig::load_defaults().expect("test config");
    Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        agent_provider: AppState::agent_provider_from_config(&config, None),
        run_streams: Arc::new(coppice_server::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(coppice_server::events::bus::EventBus::new()),
        opencode_serve: None,
        config,
        db: Some(pool),
    })
}

async fn test_state_with_db_and_workers(mock_response: &str) -> (Arc<AppState>, AgentTestEnv) {
    let worktrees = tempfile::tempdir().expect("worktrees tempdir");

    std::env::set_var("WORKTREES_PATH", worktrees.path());
    std::env::set_var("MOCK_AGENT_RESPONSE", mock_response);
    std::env::set_var("AGENT_DEFAULT_PROVIDER", "mock");

    let state = test_state_with_db().await;
    coppice_server::workers::job_worker::spawn_workers(state.clone());

    (state, AgentTestEnv { worktrees })
}

fn bootstrap_password_header() -> (&'static str, &'static str) {
    ("x-bootstrap-password", "changeme")
}

pub async fn spawn_test_server(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve test server");
    });
    addr
}

pub async fn bootstrap_and_login() -> (Router, String, String) {
    let state = test_state_with_db().await;
    let app = coppice_server::app(state);

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/bootstrap")
                .header("content-type", "application/json")
                .header(bootstrap_password_header().0, bootstrap_password_header().1)
                .body(Body::from(
                    r#"{"email":"admin@localhost","password":"changeme"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"email":"admin@localhost","password":"changeme"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);

    let set_cookie = login
        .headers()
        .get(header::SET_COOKIE)
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

    (app, cookie, csrf_token)
}

pub async fn bootstrap_and_login_with_workers(
    mock_response: &str,
) -> (Router, String, String, AgentTestEnv) {
    let (state, env) = test_state_with_db_and_workers(mock_response).await;
    let app = coppice_server::app(state);

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/bootstrap")
                .header("content-type", "application/json")
                .header(bootstrap_password_header().0, bootstrap_password_header().1)
                .body(Body::from(
                    r#"{"email":"admin@localhost","password":"changeme"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"email":"admin@localhost","password":"changeme"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);

    let set_cookie = login
        .headers()
        .get(header::SET_COOKIE)
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

    (app, cookie, csrf_token, env)
}

pub fn json_request(
    method: &str,
    uri: &str,
    body: &str,
    cookie: &str,
    csrf: &str,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::COOKIE, cookie);

    if method != "GET" {
        builder = builder.header("x-csrf-token", csrf);
    }

    builder.body(Body::from(body.to_string())).unwrap()
}

pub async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

pub async fn create_test_project(app: &Router, cookie: &str, csrf: &str) -> String {
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/projects",
            r#"{"name":"Test Project"}"#,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = json_body(res).await;
    body["id"].as_str().unwrap().to_string()
}

pub async fn register_test_repo(
    app: &Router,
    local_path: &str,
    cookie: &str,
    csrf: &str,
) -> String {
    let body = serde_json::json!({
        "name": "test-repo",
        "localPath": local_path,
        "defaultBranch": "main",
    });
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/repos",
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = json_body(res).await;
    body["id"].as_str().unwrap().to_string()
}

pub async fn assign_agent_to_ticket(
    app: &Router,
    ticket_id: &str,
    agent_id: &str,
    cookie: &str,
    csrf: &str,
) {
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/assign"),
            &format!(r#"{{"agentId":"{agent_id}"}}"#),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

pub async fn set_ticket_repo(
    app: &Router,
    ticket_id: &str,
    repo_id: &str,
    cookie: &str,
    csrf: &str,
) {
    let res = app
        .clone()
        .oneshot(json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            &format!(r#"{{"repoId":"{repo_id}"}}"#),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

pub async fn create_test_agent_from_preset(
    app: &Router,
    name: &str,
    cookie: &str,
    csrf: &str,
) -> String {
    let presets_res = app
        .clone()
        .oneshot(json_request("GET", "/api/agent-presets", "", cookie, csrf))
        .await
        .unwrap();
    assert_eq!(presets_res.status(), StatusCode::OK);

    let presets: serde_json::Value = json_body(presets_res).await;
    let preset_id = presets["items"][0]["id"].as_str().unwrap();

    let create_res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/agents",
            &format!(r#"{{"name":"{name}","presetId":"{preset_id}"}}"#),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);

    let agent: serde_json::Value = json_body(create_res).await;
    agent["id"].as_str().unwrap().to_string()
}

pub async fn create_test_ticket(
    app: &Router,
    project_id: &str,
    cookie: &str,
    csrf: &str,
) -> String {
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/projects/{project_id}/tickets"),
            r#"{"title":"Test ticket","description":"details"}"#,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = json_body(res).await;
    body["id"].as_str().unwrap().to_string()
}

pub fn multipart_request(
    uri: &str,
    filename: &str,
    content_type: &str,
    contents: &str,
    cookie: &str,
    csrf: &str,
) -> Request<Body> {
    let boundary = "----coppice-test-boundary";
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: {content_type}\r\n\
         \r\n\
         {contents}\r\n\
         --{boundary}--\r\n"
    );

    Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header(header::COOKIE, cookie)
        .header("x-csrf-token", csrf)
        .body(Body::from(body))
        .unwrap()
}
