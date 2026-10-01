use coppice_server::providers::cli_runner::{
    run_cli, run_cli_with_stdin, CliError, CliInvocation, LineHandler, LineStep, RunIo,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

fn fake_cli(env: &[(&str, &str)], timeout: Duration) -> CliInvocation {
    CliInvocation {
        program: env!("CARGO_BIN_EXE_fake-cli").to_string(),
        args: Vec::new(),
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        cwd: std::env::temp_dir(),
        timeout,
    }
}

fn io() -> RunIo {
    RunIo {
        cancel_rx: None,
        session_created_tx: None,
        stderr_target: "fake_cli.stderr",
    }
}

/// Records every JSON line; stops on `{"type":"result"}`; reports `session_id`.
#[derive(Default)]
struct Recorder {
    seen: Arc<Mutex<Vec<Value>>>,
}

impl LineHandler for Recorder {
    fn on_json(&mut self, value: &Value) -> LineStep {
        self.seen.lock().unwrap().push(value.clone());
        LineStep {
            session_id: value
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            stop: value.get("type").and_then(Value::as_str) == Some("result"),
        }
    }
}

#[tokio::test]
async fn run_cli_reads_json_lines_until_stop() {
    let lines =
        "not json\n{\"type\":\"a\"}\nalso not json\n{\"type\":\"result\"}\n{\"type\":\"after\"}";
    let mut handler = Recorder::default();
    let exit = run_cli(
        fake_cli(&[("FAKE_CLI_LINES", lines)], Duration::from_secs(10)),
        &mut handler,
        io(),
    )
    .await
    .expect("run_cli");

    assert!(exit.status.success());
    let seen = handler.seen.lock().unwrap();
    let types: Vec<_> = seen.iter().map(|v| v["type"].as_str().unwrap()).collect();
    assert_eq!(types, vec!["a", "result"]);
}

#[tokio::test]
async fn run_cli_forwards_first_session_id_once() {
    let lines = "{\"session_id\":\"\"}\n{\"session_id\":\"first\"}\n{\"session_id\":\"second\"}";
    let (tx, rx) = watch::channel(String::new());
    let mut handler = Recorder::default();
    run_cli(
        fake_cli(&[("FAKE_CLI_LINES", lines)], Duration::from_secs(10)),
        &mut handler,
        RunIo {
            session_created_tx: Some(tx),
            ..io()
        },
    )
    .await
    .expect("run_cli");

    // A watch channel keeps the last value sent, so "first" means "second" never was.
    assert_eq!(*rx.borrow(), "first");
}

#[tokio::test]
async fn run_cli_cancel_kills_child() {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut handler = Recorder { seen: seen.clone() };
    let task = tokio::spawn(async move {
        run_cli(
            fake_cli(
                &[("FAKE_CLI_PRINT_PID", "1"), ("FAKE_CLI_SLEEP_MS", "30000")],
                Duration::from_secs(60),
            ),
            &mut handler,
            RunIo {
                cancel_rx: Some(cancel_rx),
                ..io()
            },
        )
        .await
    });

    let pid = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(pid) = seen.lock().unwrap().first().and_then(|v| v["pid"].as_u64()) {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fake-cli printed its pid");

    cancel_tx.send(true).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("cancel returns within 2s")
        .expect("join");
    assert!(matches!(result, Err(CliError::Cancelled)), "{result:?}");

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "child {pid} still running"
    );
}

#[tokio::test]
async fn run_cli_timeout_kills_child_and_returns_tail() {
    let mut handler = Recorder::default();
    let started = std::time::Instant::now();
    let result = run_cli(
        fake_cli(
            &[("FAKE_CLI_STDERR", "boom"), ("FAKE_CLI_SLEEP_MS", "30000")],
            Duration::from_millis(300),
        ),
        &mut handler,
        io(),
    )
    .await;

    assert!(started.elapsed() < Duration::from_secs(5));
    match result {
        Err(CliError::TimedOut { stderr_tail }) => assert_eq!(stderr_tail, vec!["boom"]),
        other => panic!("expected TimedOut, got {other:?}"),
    }
}

#[tokio::test]
async fn run_cli_returns_status_and_tail() {
    let stderr: String = (0..50).map(|i| format!("line {i}\n")).collect();
    let mut handler = Recorder::default();
    let exit = run_cli(
        fake_cli(
            &[("FAKE_CLI_STDERR", &stderr), ("FAKE_CLI_EXIT", "3")],
            Duration::from_secs(10),
        ),
        &mut handler,
        io(),
    )
    .await
    .expect("run_cli");

    assert_eq!(exit.status.code(), Some(3));
    assert_eq!(exit.stderr_tail.len(), 40);
    assert_eq!(exit.stderr_tail[0], "line 0");
    assert_eq!(exit.stderr_tail[39], "line 39");
}

#[tokio::test]
async fn run_cli_spawn_failure() {
    let mut inv = fake_cli(&[], Duration::from_secs(10));
    inv.program = "/nonexistent/coppice-fake-cli-missing".to_string();
    let mut handler = Recorder::default();
    let result = run_cli(inv, &mut handler, io()).await;
    assert!(matches!(result, Err(CliError::Spawn(_))), "{result:?}");
}

#[tokio::test]
async fn run_cli_with_stdin_writes_and_closes_stdin() {
    let mut handler = Recorder::default();
    run_cli_with_stdin(
        fake_cli(&[("FAKE_CLI_ECHO_STDIN", "1")], Duration::from_secs(10)),
        Some("the prompt".to_string()),
        &mut handler,
        io(),
    )
    .await
    .expect("run_cli_with_stdin");

    let seen = handler.seen.lock().unwrap();
    assert_eq!(seen[0]["stdin"], "the prompt");
}
