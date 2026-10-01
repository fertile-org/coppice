//! Cursor and kilo-code adapters driven end to end against the `fake-cli` bin.

use coppice_config::{CursorProviderConfig, KiloCodeProviderConfig};
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::providers::cursor::CursorProvider;
use coppice_server::providers::kilo_code::KiloCodeProvider;
use coppice_server::providers::{AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::watch;

const FAKE_CLI: &str = env!("CARGO_BIN_EXE_fake-cli");

fn fixture(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures");
    std::fs::read_to_string(root.join(path)).expect("read fixture")
}

/// A worktree with `.agent/context.md` and the fake CLI's env file in its root
/// (the adapters spawn the CLI with the worktree as cwd).
fn worktree(env: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let agent = tmp.path().join(".agent");
    std::fs::create_dir_all(&agent).expect("mkdir .agent");
    std::fs::write(agent.join("context.md"), "context").expect("write context");
    let vars: serde_json::Map<String, serde_json::Value> = env
        .iter()
        .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
        .collect();
    std::fs::write(
        tmp.path().join(".fake-cli-env.json"),
        serde_json::Value::Object(vars).to_string(),
    )
    .expect("write env file");
    tmp
}

fn run_input(worktree: &Path) -> AgentRunInput {
    AgentRunInput {
        agent_id: "agent-1".into(),
        agent_key: "backend_engineer".into(),
        agent_role: "Backend Engineer".into(),
        job_type: "work_on_ticket".into(),
        ticket_id: None,
        ticket_status: None,
        context_profile: ContextProfile::Full,
        context_path: worktree
            .join(".agent")
            .join("context.md")
            .display()
            .to_string(),
        run_id: Some("run-1".into()),
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
    }
}

fn cursor(run_timeout_secs: u64) -> CursorProvider {
    CursorProvider::new(CursorProviderConfig {
        command: FAKE_CLI.to_string(),
        run_timeout_secs,
        ..CursorProviderConfig::default()
    })
}

fn kilo(run_timeout_secs: u64) -> KiloCodeProvider {
    KiloCodeProvider::new(KiloCodeProviderConfig {
        command: FAKE_CLI.to_string(),
        run_timeout_secs,
        ..KiloCodeProviderConfig::default()
    })
}

fn invalid_fixture(err: ProviderError) -> String {
    match err {
        ProviderError::InvalidFixture(msg) => msg,
        other => panic!("expected InvalidFixture, got {other:?}"),
    }
}

#[tokio::test]
async fn cursor_accepts_result_despite_nonzero_exit() {
    let lines = fixture("cursor/done.jsonl");
    let wt = worktree(&[("FAKE_CLI_LINES", &lines), ("FAKE_CLI_EXIT", "1")]);
    let (tx, rx) = watch::channel(String::new());
    let mut input = run_input(wt.path());
    input.session_created_tx = Some(tx);

    let result = cursor(30).run(input).await.expect("cursor run");

    match result {
        AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Implemented the feature."),
        other => panic!("expected done, got {other:?}"),
    }
    assert_eq!(*rx.borrow(), "sess_cursor_abc");
}

#[tokio::test]
async fn cursor_result_error_message() {
    let wt = worktree(&[
        (
            "FAKE_CLI_LINES",
            r#"{"type":"result","subtype":"error","is_error":true,"result":"model refused"}"#,
        ),
        ("FAKE_CLI_STDERR", "warn one\n   \nwarn two"),
    ]);

    let err = cursor(30)
        .run(run_input(wt.path()))
        .await
        .expect_err("result error");

    assert_eq!(
        invalid_fixture(err),
        format!("`{FAKE_CLI}` result error: model refused; stderr: warn one | warn two")
    );
}

#[tokio::test]
async fn cursor_nonzero_exit_without_result_message() {
    let wt = worktree(&[
        ("FAKE_CLI_STDERR", "fatal: no auth"),
        ("FAKE_CLI_EXIT", "3"),
    ]);
    let cwd = wt.path().canonicalize().unwrap();

    let err = cursor(30)
        .run(run_input(wt.path()))
        .await
        .expect_err("non-zero exit");

    assert_eq!(
        invalid_fixture(err),
        format!(
            "`{FAKE_CLI}` exited with exit status: 3 (cwd {}); stderr: fatal: no auth",
            cwd.display()
        )
    );
}

#[tokio::test]
async fn cursor_missing_result_message() {
    let wt = worktree(&[("FAKE_CLI_STDERR", "hint")]);

    let err = cursor(30)
        .run(run_input(wt.path()))
        .await
        .expect_err("no result");

    match err {
        ProviderError::MissingResult(msg) => assert_eq!(
            msg,
            format!("no result contract found in `{FAKE_CLI}` output; stderr: hint")
        ),
        other => panic!("expected MissingResult, got {other:?}"),
    }
}

#[tokio::test]
async fn cursor_spawn_failure_message() {
    let wt = worktree(&[]);
    let cwd = wt.path().canonicalize().unwrap();
    let missing = wt.path().join("no-such-cli").display().to_string();
    let provider = CursorProvider::new(CursorProviderConfig {
        command: missing.clone(),
        ..CursorProviderConfig::default()
    });

    let err = provider
        .run(run_input(wt.path()))
        .await
        .expect_err("spawn failure");

    match err {
        ProviderError::Io(e) => assert!(
            e.to_string().starts_with(&format!(
                "failed to spawn `{missing}` (cwd {}): ",
                cwd.display()
            )),
            "{e}"
        ),
        other => panic!("expected Io, got {other:?}"),
    }
}

#[tokio::test]
async fn cursor_timeout_message_has_stderr_suffix() {
    let wt = worktree(&[
        ("FAKE_CLI_STDERR", "still thinking\n\nalmost there"),
        ("FAKE_CLI_SLEEP_MS", "10000"),
    ]);

    let err = cursor(1)
        .run(run_input(wt.path()))
        .await
        .expect_err("timeout");

    assert_eq!(
        invalid_fixture(err),
        format!("`{FAKE_CLI}` timed out after 1s; stderr: still thinking | almost there")
    );
}

#[tokio::test]
async fn cursor_cancel_returns_cancelled() {
    let wt = worktree(&[("FAKE_CLI_SLEEP_MS", "10000")]);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let mut input = run_input(wt.path());
    input.cancel_rx = Some(cancel_rx);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = cancel_tx.send(true);
    });

    let started = Instant::now();
    let err = cursor(30).run(input).await.expect_err("cancelled");

    assert!(matches!(err, ProviderError::Cancelled), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn kilo_stops_on_session_idle() {
    // A later contract would win if the stream were read past `session.idle`.
    let blocked = fixture("kilo-code/blocked.jsonl");
    let blocked_contract = blocked.lines().nth(2).expect("blocked contract line");
    let lines = format!("{}{blocked_contract}\n", fixture("kilo-code/done.jsonl"));
    let wt = worktree(&[("FAKE_CLI_LINES", &lines)]);
    let (tx, rx) = watch::channel(String::new());
    let mut input = run_input(wt.path());
    input.session_created_tx = Some(tx);

    let result = kilo(30).run(input).await.expect("kilo run");

    match result {
        AgentRunResult::Done { summary, .. } => {
            assert_eq!(summary, "Kilo feature implementation complete.")
        }
        other => panic!("expected done, got {other:?}"),
    }
    assert_eq!(*rx.borrow(), "kilo_sess_abc123");
}

#[tokio::test]
async fn kilo_timeout_message() {
    let wt = worktree(&[
        ("FAKE_CLI_STDERR", "ignored"),
        ("FAKE_CLI_SLEEP_MS", "10000"),
    ]);

    let err = kilo(1)
        .run(run_input(wt.path()))
        .await
        .expect_err("timeout");

    assert_eq!(invalid_fixture(err), "kilo-code run timed out after 1s");
}

#[tokio::test]
async fn kilo_nonzero_exit_message() {
    let lines = fixture("kilo-code/done.jsonl");
    let wt = worktree(&[("FAKE_CLI_LINES", &lines), ("FAKE_CLI_EXIT", "2")]);

    let err = kilo(30)
        .run(run_input(wt.path()))
        .await
        .expect_err("non-zero exit");

    assert_eq!(
        invalid_fixture(err),
        "kilo-code exited with status exit status: 2"
    );
}

#[tokio::test]
async fn kilo_missing_result_message() {
    let wt = worktree(&[]);

    let err = kilo(30)
        .run(run_input(wt.path()))
        .await
        .expect_err("no result");

    match err {
        ProviderError::MissingResult(msg) => {
            assert_eq!(msg, "no result contract found in kilo-code output")
        }
        other => panic!("expected MissingResult, got {other:?}"),
    }
}

#[tokio::test]
async fn kilo_cancel_returns_cancelled() {
    let wt = worktree(&[("FAKE_CLI_SLEEP_MS", "10000")]);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let mut input = run_input(wt.path());
    input.cancel_rx = Some(cancel_rx);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = cancel_tx.send(true);
    });

    let started = Instant::now();
    let err = kilo(30).run(input).await.expect_err("cancelled");

    assert!(matches!(err, ProviderError::Cancelled), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
}
