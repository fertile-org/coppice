#![cfg(feature = "embedded-test-db")]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use coppice_server::sessions::opencode_run_server::OpenCodeRunServers;

const FAKE_OPENCODE: &str = env!("CARGO_BIN_EXE_fake-opencode");

fn servers() -> Arc<OpenCodeRunServers> {
    OpenCodeRunServers::new(FAKE_OPENCODE.into(), "127.0.0.1".into())
}

async fn doc_ok(base_url: &str) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    matches!(
        client.get(format!("{base_url}/doc")).send().await,
        Ok(resp) if resp.status().is_success()
    )
}

async fn stops_answering_within(base_url: &str, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    while tokio::time::Instant::now() < deadline {
        if !doc_ok(base_url).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

fn config_file(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("opencode.json");
    std::fs::write(&path, "{}").unwrap();
    path
}

#[tokio::test]
async fn run_server_spawns_with_per_run_config_and_token_env() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_file(dir.path());
    let record = dir.path().join("record.json");
    let servers = servers();

    let lease = servers
        .start(
            "run-1",
            &config,
            vec![
                ("FAKE_OPENCODE_RECORD", record.display().to_string()),
                ("COPPICE_MCP_TOKEN", "secret".into()),
            ],
            None,
        )
        .await
        .expect("start");

    let recorded: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&record).expect("record")).unwrap();
    assert_eq!(recorded["opencodeConfig"], config.display().to_string());
    assert_eq!(recorded["hasToken"], true);
    assert_eq!(servers.base_url("run-1").as_deref(), Some(lease.base_url()));

    lease.stop().await;
}

#[tokio::test]
async fn two_leases_get_distinct_ports() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_file(dir.path());
    let servers = servers();

    let (a, b) = tokio::join!(
        servers.start("run-a", &config, vec![], None),
        servers.start("run-b", &config, vec![], None),
    );
    let (a, b) = (a.expect("start a"), b.expect("start b"));

    assert_ne!(a.base_url(), b.base_url());
    assert!(doc_ok(a.base_url()).await);
    assert!(doc_ok(b.base_url()).await);

    a.stop().await;
    b.stop().await;
}

#[tokio::test]
async fn lease_drop_kills_process_and_forgets_base_url() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_file(dir.path());
    let servers = servers();

    let lease = servers
        .start("run-1", &config, vec![], None)
        .await
        .expect("start");
    let base_url = lease.base_url().to_string();
    assert!(doc_ok(&base_url).await);

    drop(lease);

    assert_eq!(servers.base_url("run-1"), None);
    assert!(stops_answering_within(&base_url, Duration::from_secs(5)).await);
}

#[tokio::test]
async fn shutdown_all_kills_every_server() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_file(dir.path());
    let servers = servers();

    let a = servers
        .start("run-a", &config, vec![], None)
        .await
        .expect("start a");
    let b = servers
        .start("run-b", &config, vec![], None)
        .await
        .expect("start b");
    let (url_a, url_b) = (a.base_url().to_string(), b.base_url().to_string());

    servers.shutdown_all().await;

    assert!(stops_answering_within(&url_a, Duration::from_secs(5)).await);
    assert!(stops_answering_within(&url_b, Duration::from_secs(5)).await);
    drop((a, b));
}

#[tokio::test]
async fn start_with_missing_binary_names_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_file(dir.path());
    let servers = OpenCodeRunServers::new("does-not-exist-opencode".into(), "127.0.0.1".into());

    let err = servers
        .start("run-1", &config, vec![], None)
        .await
        .err()
        .expect("missing binary must fail");
    let message = err.to_string();
    assert!(message.contains("does-not-exist-opencode"), "{message}");
    assert!(message.contains("wasn't found"), "{message}");
    assert!(
        message.contains("version-specific flags weren't applied"),
        "{message}"
    );
}

#[test]
fn opencode_stdout_and_stderr_are_stored_with_the_run_id() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_file(dir.path());
    let logs = tempfile::tempdir().unwrap();
    let servers = servers();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    coppice_server::logs::with_cli_logs(logs.path(), || {
        runtime.block_on(async {
            let lease = servers
                .start(
                    "run-oc",
                    &config,
                    vec![
                        ("FAKE_OPENCODE_STDOUT", "oc-stdout-line".into()),
                        ("FAKE_OPENCODE_STDERR", "oc-stderr-line".into()),
                    ],
                    Some("ticket-oc"),
                )
                .await
                .expect("start");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            loop {
                let lines = jsonl_lines(logs.path());
                if lines
                    .iter()
                    .any(|line| line["fields"]["message"] == "oc-stdout-line")
                    && lines
                        .iter()
                        .any(|line| line["fields"]["message"] == "oc-stderr-line")
                {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "missing cli lines: {lines:?}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            lease.stop().await;
        });
    })
    .expect("logs");

    let lines = jsonl_lines(logs.path());
    for message in ["oc-stdout-line", "oc-stderr-line"] {
        let line = lines
            .iter()
            .find(|line| line["fields"]["message"] == message)
            .unwrap_or_else(|| panic!("missing {message} in {lines:?}"));
        let stream = if message.contains("stdout") {
            "stdout"
        } else {
            "stderr"
        };
        assert_eq!(line["fields"]["source"], "cli");
        assert_eq!(line["fields"]["stream"], stream);
        assert_eq!(line["fields"]["run_id"], "run-oc");
        assert_eq!(line["fields"]["ticket_id"], "ticket-oc");
    }
}

fn jsonl_lines(dir: &Path) -> Vec<serde_json::Value> {
    let mut lines = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return lines;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !(name.starts_with("coppice.") && name.ends_with(".jsonl")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            if let Ok(value) = serde_json::from_str(line) {
                lines.push(value);
            }
        }
    }
    lines
}
