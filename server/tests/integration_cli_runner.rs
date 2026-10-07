#![cfg(feature = "embedded-test-db")]

use coppice_server::process_tree::process_alive;
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
        run_id: None,
        artifacts_dir: None,
        result_before_exit: false,
        result_exit_grace: None,
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
    assert!(wait_dead(pid as u32).await, "child {pid} still running");
}

#[tokio::test]
async fn run_cli_cancel_kills_background_child() {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut handler = Recorder { seen: seen.clone() };
    let task = tokio::spawn(async move {
        run_cli(
            fake_cli(
                &[
                    ("FAKE_CLI_BACKGROUND_SLEEP_SECS", "20"),
                    ("FAKE_CLI_SLEEP_MS", "30000"),
                ],
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

    let background = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(pid) = seen
                .lock()
                .unwrap()
                .iter()
                .find_map(|value| value["background_pid"].as_u64())
            {
                return pid as u32;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fake-cli printed its background pid");
    assert!(process_alive(background));

    cancel_tx.send(true).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("cancel returns within 5s")
        .expect("join");
    assert!(matches!(result, Err(CliError::Cancelled)), "{result:?}");
    assert!(
        wait_dead(background).await,
        "background {background} still running after cancel"
    );
}

#[tokio::test]
async fn run_cli_completion_kills_background_child_and_logs_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut invocation = fake_cli(
        &[("FAKE_CLI_BACKGROUND_SLEEP_SECS", "20")],
        Duration::from_secs(10),
    );
    invocation.run_id = Some("run-complete".into());
    invocation.artifacts_dir = Some(dir.path().display().to_string());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut handler = Recorder { seen: seen.clone() };

    let exit = run_cli(invocation, &mut handler, io())
        .await
        .expect("run_cli");
    assert!(exit.status.success());

    let background = seen
        .lock()
        .unwrap()
        .iter()
        .find_map(|value| value["background_pid"].as_u64())
        .expect("background pid") as u32;
    assert!(
        wait_dead(background).await,
        "background {background} still running after the run ended"
    );
    let log = std::fs::read_to_string(
        dir.path()
            .join("runs")
            .join("run-complete")
            .join("process-stops.log"),
    )
    .expect("task log");
    assert!(log.contains("\"runId\":\"run-complete\""), "{log}");
    assert!(
        log.contains("\"outcome\":\"terminated\"") || log.contains("\"outcome\":\"killed\""),
        "{log}"
    );
    assert!(log.contains("\"reason\":\"run ended\""), "{log}");
}

#[tokio::test]
async fn run_cli_reaps_lingering_process_after_result() {
    let mut invocation = fake_cli(
        &[
            ("FAKE_CLI_PRINT_PID", "1"),
            ("FAKE_CLI_LINES", r#"{"type":"result","result":"ok"}"#),
            ("FAKE_CLI_SLEEP_MS", "30000"),
        ],
        Duration::from_secs(30),
    );
    invocation.result_before_exit = true;
    invocation.result_exit_grace = Some(Duration::from_millis(200));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut handler = Recorder { seen: seen.clone() };
    let started = std::time::Instant::now();
    let exit = run_cli(invocation, &mut handler, io())
        .await
        .expect("run_cli");
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "lingering process held the run open"
    );
    assert!(exit.had_terminal_result);
    assert!(exit.stopped_after_result);
    let pid = seen.lock().unwrap()[0]["pid"].as_u64().expect("pid") as u32;
    assert!(wait_dead(pid).await, "child {pid} still running");
}

#[tokio::test]
async fn run_cli_cancel_during_result_grace() {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let mut invocation = fake_cli(
        &[
            ("FAKE_CLI_LINES", r#"{"type":"result"}"#),
            ("FAKE_CLI_SLEEP_MS", "30000"),
        ],
        Duration::from_secs(30),
    );
    invocation.result_before_exit = true;
    invocation.result_exit_grace = Some(Duration::from_secs(10));
    let mut handler = Recorder::default();
    let task = tokio::spawn(async move {
        run_cli(
            invocation,
            &mut handler,
            RunIo {
                cancel_rx: Some(cancel_rx),
                ..io()
            },
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel_tx.send(true).unwrap();
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("cancel during grace returns")
        .expect("join");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(matches!(result, Err(CliError::Cancelled)), "{result:?}");
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
async fn run_cli_timeout_returns_even_if_grandchild_holds_stderr() {
    let mut handler = Recorder::default();
    let started = std::time::Instant::now();
    let result = run_cli(
        fake_cli(
            &[
                ("FAKE_CLI_GRANDCHILD_SLEEP_MS", "30000"),
                ("FAKE_CLI_STDERR", "boom"),
                ("FAKE_CLI_SLEEP_MS", "30000"),
            ],
            Duration::from_millis(300),
        ),
        &mut handler,
        io(),
    )
    .await;
    let elapsed = started.elapsed();

    let grandchild = handler
        .seen
        .lock()
        .unwrap()
        .iter()
        .find_map(|v| v["grandchild_pid"].as_u64());

    assert!(grandchild.is_some(), "fake-cli reported its grandchild");
    let grandchild = grandchild.expect("grandchild") as u32;
    assert!(
        wait_dead(grandchild).await,
        "grandchild {grandchild} still running after timeout"
    );
    assert!(
        elapsed < Duration::from_millis(300) + Duration::from_secs(3),
        "timeout took {elapsed:?}"
    );
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

async fn wait_dead(pid: u32) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if !process_alive(pid) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}
