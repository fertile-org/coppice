use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::services::connector_probe_service::ConnectorProbes;
use coppice_server::{db, AppConfig, AppState};
use http_body_util::BodyExt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tower::ServiceExt;

pub static DB_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Whether the shared embedded (or external escape-hatch) test database is reachable.
pub async fn db_available() -> bool {
    let pool = match db::shared_test_pool().await {
        Ok(pool) => pool,
        Err(_) => return false,
    };
    sqlx::query("SELECT 1").execute(&pool).await.is_ok()
}

async fn prepare_test_pool() -> sqlx::PgPool {
    let pool = db::shared_test_pool()
        .await
        .expect("embedded test database (see docs/testing.md)");
    truncate_workspace(&pool).await;
    pool
}

/// Auth integration tests only need sessions/users cleared.
pub async fn prepare_test_pool_for_auth() -> sqlx::PgPool {
    let pool = db::shared_test_pool()
        .await
        .expect("embedded test database (see docs/testing.md)");
    sqlx::query("TRUNCATE sessions, users RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .expect("truncate auth tables");
    pool
}

pub async fn truncate_workspace(pool: &sqlx::PgPool) {
    db::truncate_test_workspace(pool)
        .await
        .expect("truncate workspace tables");
}

#[allow(dead_code)]
pub struct AgentTestEnv {
    pub worktrees: tempfile::TempDir,
}

/// Repo-local identity for checkouts whose commits are created or replayed by
/// the server (`git rebase`, `git commit`). Git 2.55 refuses an empty committer
/// name, and CI runners have none. Env vars on the test process do not reach
/// the server's git subprocess; this config does, including from worktrees.
pub fn configure_test_git_identity(dir: &Path) {
    for (key, value) in [("user.name", "test"), ("user.email", "test@localhost")] {
        let output = Command::new("git")
            .args(["config", key, value])
            .current_dir(dir)
            .output()
            .unwrap_or_else(|_| panic!("git config {key}"));
        assert!(
            output.status.success(),
            "git config {key} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

pub fn setup_worktree_with_commit(
    git_dir: &Path,
    worktrees_root: &Path,
    repo_name: &str,
    ticket_id: &str,
) -> PathBuf {
    use coppice_server::services::worktree_service::compute_paths;
    let ticket_uuid: uuid::Uuid = ticket_id.parse().expect("ticket uuid");
    let paths = compute_paths(worktrees_root, repo_name, ticket_uuid);
    std::fs::create_dir_all(&paths.worktree_dir).expect("worktree dir");
    let worktree_output = Command::new("git")
        .args([
            "worktree",
            "add",
            "-B",
            &paths.branch_name,
            paths.worktree_dir.to_str().unwrap(),
            "main",
        ])
        .current_dir(git_dir)
        .output()
        .expect("worktree add");
    assert!(
        worktree_output.status.success(),
        "worktree add failed: {}",
        String::from_utf8_lossy(&worktree_output.stderr)
    );
    std::fs::write(paths.worktree_dir.join("feature.txt"), "new feature\n").expect("write");
    Command::new("git")
        .args(["add", "feature.txt"])
        .current_dir(&paths.worktree_dir)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "feature"])
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(&paths.worktree_dir)
        .output()
        .expect("git commit");
    paths.worktree_dir
}

pub fn create_temp_git_checkout() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&path)
        .output()
        .expect("git init");
    configure_test_git_identity(&path);
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

/// Points the default plugin dir at a fresh per-test directory and runs the
/// same plugin startup as `main.rs`, so tests never touch the repo's `./data`.
async fn init_test_plugins(
    config: &mut AppConfig,
    pool: &sqlx::PgPool,
) -> Arc<coppice_server::plugins::skills::SkillCatalog> {
    static BASE: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let base = BASE.get_or_init(|| tempfile::tempdir().expect("plugins tempdir"));
    let dir = base.path().join(uuid::Uuid::new_v4().to_string());
    config.plugins.dir = dir.to_string_lossy().into_owned();
    let skills = AppState::test_skills();
    AppState::init_plugins(pool, config, &skills)
        .await
        .expect("init test plugins");
    skills
}

/// Secret store plus the production tool registry (with the plugin MCP source).
pub struct GatewayParts {
    pub secret_store: coppice_server::crypto::SecretStore,
    pub tools: Arc<coppice_server::mcp::registry::ToolRegistry>,
    pub plugin_mcp: Arc<coppice_server::mcp::proxy::McpServerPool>,
}

impl GatewayParts {
    pub fn new(config: &AppConfig, pool: &sqlx::PgPool) -> Self {
        let secret_store =
            coppice_server::crypto::SecretStore::from_master_key(&config.secrets.master_key);
        let plugin_mcp = AppState::plugin_mcp_from_config(config);
        let tools = AppState::build_tool_registry(
            Some(pool),
            &secret_store,
            plugin_mcp.clone(),
            AppState::list_timeout_from_config(config),
        );
        Self {
            secret_store,
            tools,
            plugin_mcp,
        }
    }
}

async fn test_state_with_db() -> Arc<AppState> {
    test_state_with_db_config(|_| {}).await
}

async fn test_state_with_db_config<F>(configure: F) -> Arc<AppState>
where
    F: FnOnce(&mut AppConfig),
{
    std::env::remove_var("MOCK_AGENT_RESPONSE");
    let pool = prepare_test_pool().await;

    std::env::set_var(
        "COPPICE_STORAGE__ARTIFACTS_DIR",
        "/tmp/coppice-test-artifacts",
    );
    let mut config = AppConfig::load_defaults().expect("test config");
    configure(&mut config);
    let skills = init_test_plugins(&mut config, &pool).await;
    let opencode_runs = AppState::test_opencode_runs();
    let gateway = GatewayParts::new(&config, &pool);
    Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        connectors: AppState::connectors_runtime(&config, opencode_runs.clone()),
        agent_health: Arc::new(coppice_server::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(coppice_server::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(coppice_server::events::bus::EventBus::new()),
        opencode_runs,
        agent_templates: coppice_server::AppState::load_agent_templates(),
        secret_store: gateway.secret_store,
        skills,
        tools: gateway.tools,
        plugin_mcp: gateway.plugin_mcp,
        connector_probes: Arc::new(ConnectorProbes::new()),
        config,
        db: Some(pool),
    })
}

async fn test_state_with_db_and_workers(mock_response: &str) -> (Arc<AppState>, AgentTestEnv) {
    test_state_with_db_and_workers_config(mock_response, |_| {}).await
}

async fn test_state_with_db_and_workers_config<F>(
    mock_response: &str,
    configure: F,
) -> (Arc<AppState>, AgentTestEnv)
where
    F: FnOnce(&mut AppConfig),
{
    let worktrees = tempfile::tempdir().expect("worktrees tempdir");

    std::env::set_var("WORKTREES_PATH", worktrees.path());
    std::env::set_var("MOCK_AGENT_RESPONSE", mock_response);
    std::env::set_var("AGENT_DEFAULT_PROVIDER", "mock");
    std::env::remove_var("WORKFLOW_AUTO_START_RUNS");

    let pool = prepare_test_pool().await;

    std::env::set_var(
        "COPPICE_STORAGE__ARTIFACTS_DIR",
        "/tmp/coppice-test-artifacts",
    );
    let mut config = AppConfig::load_defaults().expect("test config");
    configure(&mut config);
    config.agent.worker_count = 1;
    let skills = init_test_plugins(&mut config, &pool).await;

    let opencode_runs = AppState::test_opencode_runs();
    let gateway = GatewayParts::new(&config, &pool);
    let state = Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        connectors: AppState::connectors_runtime(&config, opencode_runs.clone()),
        agent_health: Arc::new(coppice_server::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(coppice_server::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(coppice_server::events::bus::EventBus::new()),
        opencode_runs,
        agent_templates: coppice_server::AppState::load_agent_templates(),
        secret_store: gateway.secret_store,
        skills,
        tools: gateway.tools,
        plugin_mcp: gateway.plugin_mcp,
        connector_probes: Arc::new(ConnectorProbes::new()),
        config,
        db: Some(pool),
    });
    coppice_server::workers::job_worker::spawn_workers(state.clone());

    (state, AgentTestEnv { worktrees })
}

async fn test_state_with_db_and_auto_start_workers<F>(
    worker_count: u32,
    configure: F,
) -> (Arc<AppState>, AgentTestEnv)
where
    F: FnOnce(&mut AppConfig),
{
    let worktrees = tempfile::tempdir().expect("worktrees tempdir");

    std::env::set_var("WORKTREES_PATH", worktrees.path());
    std::env::remove_var("MOCK_AGENT_RESPONSE");
    std::env::set_var("AGENT_DEFAULT_PROVIDER", "mock");
    std::env::set_var("WORKFLOW_AUTO_START_RUNS", "true");

    let pool = prepare_test_pool().await;

    std::env::set_var(
        "COPPICE_STORAGE__ARTIFACTS_DIR",
        "/tmp/coppice-test-artifacts",
    );
    let mut config = AppConfig::load_defaults().expect("test config");
    config.workflow.auto_start_runs = true;
    configure(&mut config);
    config.agent.worker_count = worker_count;
    let skills = init_test_plugins(&mut config, &pool).await;

    let opencode_runs = AppState::test_opencode_runs();
    let gateway = GatewayParts::new(&config, &pool);
    let state = Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        connectors: AppState::connectors_runtime(&config, opencode_runs.clone()),
        agent_health: Arc::new(coppice_server::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(coppice_server::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(coppice_server::events::bus::EventBus::new()),
        opencode_runs,
        agent_templates: coppice_server::AppState::load_agent_templates(),
        secret_store: gateway.secret_store,
        skills,
        tools: gateway.tools,
        plugin_mcp: gateway.plugin_mcp,
        connector_probes: Arc::new(ConnectorProbes::new()),
        config,
        db: Some(pool),
    });
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

pub async fn bootstrap_and_login_with_state() -> (Arc<AppState>, Router, String, String) {
    bootstrap_and_login_with_state_config(|_| {}).await
}

pub async fn bootstrap_and_login_with_state_config<F>(
    configure: F,
) -> (Arc<AppState>, Router, String, String)
where
    F: FnOnce(&mut AppConfig),
{
    let state = test_state_with_db_config(configure).await;
    let app = coppice_server::app(state.clone());

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

    (state, app, cookie, csrf_token)
}

pub async fn bootstrap_and_login_with_auto_start_workers(
) -> (Arc<AppState>, Router, String, String, AgentTestEnv) {
    bootstrap_and_login_with_auto_start_worker_count(1).await
}

pub async fn bootstrap_and_login_with_auto_start_worker_count(
    worker_count: u32,
) -> (Arc<AppState>, Router, String, String, AgentTestEnv) {
    bootstrap_and_login_with_auto_start_worker_config(worker_count, |_| {}).await
}

pub async fn bootstrap_and_login_with_auto_start_worker_config<F>(
    worker_count: u32,
    configure: F,
) -> (Arc<AppState>, Router, String, String, AgentTestEnv)
where
    F: FnOnce(&mut AppConfig),
{
    let (state, env) = test_state_with_db_and_auto_start_workers(worker_count, configure).await;
    let app = coppice_server::app(state.clone());

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

    (state, app, cookie, csrf_token, env)
}

pub async fn bootstrap_and_login_with_state_and_workers<F>(
    mock_response: &str,
    configure: F,
) -> (Arc<AppState>, Router, String, String, AgentTestEnv)
where
    F: FnOnce(&mut AppConfig),
{
    let (state, env) = test_state_with_db_and_workers_config(mock_response, configure).await;
    let app = coppice_server::app(state.clone());

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

    (state, app, cookie, csrf_token, env)
}

/// Workers plus a real listener: the gateway URL in the config points at the
/// spawned server, so mock `toolCalls` reach `/mcp` over HTTP.
pub async fn bootstrap_and_login_with_gateway(
    mock_response: &str,
) -> (Arc<AppState>, Router, String, String, AgentTestEnv) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway listener");
    let addr = listener.local_addr().expect("gateway addr");
    let (state, app, cookie, csrf, env) =
        bootstrap_and_login_with_state_and_workers(mock_response, |config| {
            config.mcp.base_url = Some(format!("http://{addr}/mcp"));
        })
        .await;
    let served = app.clone();
    tokio::spawn(async move {
        axum::serve(listener, served).await.expect("serve gateway");
    });
    (state, app, cookie, csrf, env)
}

pub async fn bootstrap_and_login_with_workers(
    mock_response: &str,
) -> (Router, String, String, AgentTestEnv) {
    let (_state, app, cookie, csrf, env) =
        bootstrap_and_login_with_state_and_workers(mock_response, |_| {}).await;
    (app, cookie, csrf, env)
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

pub async fn create_test_board(app: &Router, cookie: &str, csrf: &str) -> String {
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/boards",
            r#"{"name":"Test Board"}"#,
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

pub async fn create_agent_with_preset_key(
    app: &Router,
    preset_key: &str,
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
    let preset_id = presets["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"].as_str() == Some(preset_key))
        .and_then(|item| item["id"].as_str())
        .unwrap_or_else(|| panic!("preset {preset_key} not found"));

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
    assert_eq!(agent["connector"].as_str().unwrap(), "mock");
    agent["id"].as_str().unwrap().to_string()
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

pub async fn get_ticket(
    app: &Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    json_body(res).await
}

pub async fn poll_ticket_until(
    app: &Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
    label: &str,
    timeout: Duration,
    predicate: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = Instant::now() + timeout;
    loop {
        let ticket = get_ticket(app, ticket_id, cookie, csrf).await;
        if predicate(&ticket) {
            return ticket;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for ticket condition: {label}; last status={}",
                ticket["status"].as_str().unwrap_or("?")
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn poll_runs_until_count(
    app: &Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
    label: &str,
    timeout: Duration,
    predicate: impl Fn(&[serde_json::Value]) -> bool,
) -> Vec<serde_json::Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let res = app
            .clone()
            .oneshot(json_request(
                "GET",
                &format!("/api/tickets/{ticket_id}/runs"),
                "",
                cookie,
                csrf,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body: serde_json::Value = json_body(res).await;
        let runs = body["runs"].as_array().cloned().unwrap_or_default();
        if predicate(&runs) {
            return runs;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for runs condition: {label}");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn create_test_ticket(app: &Router, board_id: &str, cookie: &str, csrf: &str) -> String {
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/boards/{board_id}/tickets"),
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

/// Mints a gateway token for an existing run, scoped from the run's own agent/ticket/board.
pub async fn mint_test_token(
    state: &Arc<AppState>,
    run_id: uuid::Uuid,
    profile: coppice_server::domain::context_profile::ContextProfile,
) -> String {
    mint_test_token_with_plugins(state, run_id, profile, vec![]).await
}

/// Like `mint_test_token`, with the given plugin snapshot.
pub async fn mint_test_token_with_plugins(
    state: &Arc<AppState>,
    run_id: uuid::Uuid,
    profile: coppice_server::domain::context_profile::ContextProfile,
    plugin_ids: Vec<uuid::Uuid>,
) -> String {
    use coppice_server::mcp::token::{NewRunToolScope, TokenService};

    let pool = state.db.as_ref().expect("db pool");
    let (agent_id, ticket_id, job_type, board_id): (uuid::Uuid, uuid::Uuid, String, uuid::Uuid) =
        sqlx::query_as(
            "SELECT r.agent_id, r.ticket_id, r.job_type, t.board_id \
             FROM agent_runs r JOIN tickets t ON t.id = r.ticket_id WHERE r.id = $1",
        )
        .bind(run_id)
        .fetch_one(pool)
        .await
        .expect("run row");
    TokenService::new(pool)
        .mint(
            &NewRunToolScope {
                run_id,
                agent_id,
                ticket_id: Some(ticket_id),
                chat_session_id: None,
                board_id: Some(board_id),
                profile,
                job_type,
                compaction_ticket_ids: vec![],
                plugin_ids,
            },
            Duration::from_secs(60),
        )
        .await
        .expect("mint token")
}
