//! A short process PATH plus a login shell that adds tools must still find CLIs.
#![cfg(feature = "embedded-test-db")]

mod common;

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use coppice_config::CursorProviderConfig;
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::host_env;
use coppice_server::mcp::proxy::{McpTransport, StdioTransport};
use coppice_server::plugins::placeholders::ResolvedTransport;
use coppice_server::providers::cursor::CursorProvider;
use coppice_server::providers::{AgentProvider, AgentRunInput, AgentRunResult};
use tower::ServiceExt;

static ENV_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

fn script(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

fn zsh_path() -> &'static str {
    if Path::new("/bin/zsh").is_file() {
        "/bin/zsh"
    } else if Path::new("/usr/bin/zsh").is_file() {
        "/usr/bin/zsh"
    } else {
        panic!("zsh is required for the login-shell PATH test");
    }
}

struct EnvGuard {
    path: Option<OsString>,
    home: Option<OsString>,
    shell: Option<OsString>,
}

impl EnvGuard {
    fn apply(path: &str, home: &Path) -> Self {
        let saved = Self {
            path: std::env::var_os("PATH"),
            home: std::env::var_os("HOME"),
            shell: std::env::var_os("SHELL"),
        };
        std::env::set_var("PATH", path);
        std::env::set_var("HOME", home);
        std::env::set_var("SHELL", zsh_path());
        host_env::invalidate();
        saved
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        restore("PATH", self.path.take());
        restore("HOME", self.home.take());
        restore("SHELL", self.shell.take());
        host_env::invalidate();
    }
}

fn restore(key: &str, value: Option<OsString>) {
    match value {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
    }
}

#[tokio::test]
async fn short_path_login_shell_finds_clis_and_refresh_sees_a_new_dir() {
    let _guard = ENV_LOCK.lock().await;
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools/bin");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(
        home.path().join(".zshrc"),
        "export PATH=\"$HOME/tools/bin:$PATH\"\n",
    )
    .unwrap();

    let result_line = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"status\":\"done\",\"summary\":\"Found via the login shell.\",\"changedFiles\":[],\"testsRun\":[],\"assignTo\":\"backend_engineer\",\"mentionAgents\":[],\"blockers\":[]}"}"#;
    script(
        &tools.join("agent"),
        &format!(
            r#"case "$1" in
  models) echo 'composer-2.5 - Composer 2.5'; exit 0 ;;
  --version) echo 'agent 1.2.3'; exit 0 ;;
  *) printf '%s\n' '{result_line}'; exit 0 ;;
esac"#
        ),
    );
    script(
        &tools.join("claude"),
        r#"case "$1" in
  --version) echo 'claude 2.1.0'; exit 0 ;;
  *) echo '{"loggedIn":true}'; exit 0 ;;
esac"#,
    );
    let fake_mcp = env!("CARGO_BIN_EXE_fake-mcp");
    script(
        &tools.join("npx"),
        &format!("touch \"$HOME/npx-stamp\"\nexec '{fake_mcp}' \"$@\""),
    );

    let _env = EnvGuard::apply("/usr/bin:/bin", home.path());

    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.agent.connectors.cursor.enabled = true;
        config.agent.connectors.claude_code.enabled = true;
    })
    .await;

    let cursor = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/tools/connectors/cursor/check",
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(cursor.status(), axum::http::StatusCode::OK);
    let cursor_json = common::json_body(cursor).await;
    assert_eq!(cursor_json["readiness"], "ready", "{cursor_json}");
    let cursor_path = cursor_json["cli"]["path"].as_str().unwrap_or("");
    assert!(
        cursor_path.contains("tools/bin/agent"),
        "{cursor_path}"
    );

    let claude = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/tools/connectors/claude-code/check",
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(claude.status(), axum::http::StatusCode::OK);
    let claude_json = common::json_body(claude).await;
    assert_eq!(claude_json["readiness"], "ready", "{claude_json}");

    let models = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/connectors/cursor/model-providers/cursor/models")
                .header(axum::http::header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(models.status(), axum::http::StatusCode::OK);
    let models_json = common::json_body(models).await;
    let ids: Vec<&str> = models_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert!(ids.contains(&"composer-2.5"), "{models_json}");

    let work = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join(".agent")).unwrap();
    std::fs::write(work.path().join(".agent/context.md"), "context").unwrap();
    let provider = CursorProvider::new(CursorProviderConfig {
        command: "agent".into(),
        enabled: true,
        run_timeout_secs: 30,
        ..CursorProviderConfig::default()
    });
    let result = provider
        .run(AgentRunInput {
            agent_id: "agent-1".into(),
            agent_key: "backend_engineer".into(),
            agent_role: "Backend Engineer".into(),
            job_type: "work_on_ticket".into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::Full,
            context_path: work
                .path()
                .join(".agent/context.md")
                .display()
                .to_string(),
            run_id: Some("run-host-env".into()),
            chat_session_id: None,
            artifacts_dir: None,
            stream: None,
            cancel_rx: None,
            model_provider: None,
            model: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: false,
            mcp: None,
        })
        .await
        .expect("cursor run");
    match result {
        AgentRunResult::Done { summary, .. } => {
            assert_eq!(summary, "Found via the login shell.");
        }
        other => panic!("expected done, got {other:?}"),
    }

    let cwd = tempfile::tempdir().unwrap();
    let conn = StdioTransport
        .connect(
            &ResolvedTransport::Stdio {
                command: "npx".into(),
                args: Vec::new(),
                env: Default::default(),
                secrets: Vec::new(),
            },
            cwd.path(),
        )
        .await
        .expect("mcp spawn");
    assert!(home.path().join("npx-stamp").is_file(), "npx was not spawned");
    conn.close().await;

    let extra = home.path().join("extra/bin");
    script(&extra.join("kilo"), "echo 'kilo 1.0.0'\nexit 0");
    std::fs::write(
        home.path().join(".zshrc"),
        "export PATH=\"$HOME/extra/bin:$HOME/tools/bin:$PATH\"\n",
    )
    .unwrap();
    let kilo = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/tools/connectors/kilo-code/check",
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(kilo.status(), axum::http::StatusCode::OK);
    let kilo_json = common::json_body(kilo).await;
    assert_eq!(kilo_json["readiness"], "found", "{kilo_json}");
    assert!(kilo_json["cli"]["found"].as_bool().unwrap_or(false));
    let kilo_path = kilo_json["cli"]["path"].as_str().unwrap_or("");
    assert!(kilo_path.contains("extra/bin/kilo"), "{kilo_path}");
}

#[test]
fn a_shell_that_sleeps_falls_back_before_the_hang_finishes() {
    let _guard = ENV_LOCK.blocking_lock();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("tools/bin")).unwrap();
    std::fs::write(home.path().join(".zshrc"), "sleep 30\n").unwrap();
    let _env = EnvGuard::apply("/usr/bin:/bin", home.path());
    let started = Instant::now();
    let captured = host_env::current();
    assert!(started.elapsed() < Duration::from_secs(15));
    let path = captured.path.to_string_lossy();
    assert!(
        !path.contains("tools/bin"),
        "slow shell must not contribute PATH: {path}"
    );
    assert!(path.contains("/usr/bin"), "{path}");
}
