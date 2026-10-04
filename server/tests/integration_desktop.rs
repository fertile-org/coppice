#![cfg(all(unix, feature = "embedded-test-db"))]

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

const READY_TIMEOUT: Duration = Duration::from_secs(120);
const EXIT_TIMEOUT: Duration = Duration::from_secs(30);
const INDEX_HTML: &str = "<!doctype html><title>coppice desktop test</title>\n";

/// A running `coppice-server desktop`; killed (and its Postgres stopped) if dropped early.
struct DesktopProcess {
    child: Child,
    url: String,
    pg_bin: PathBuf,
    pg_lib: PathBuf,
    pg_data: PathBuf,
}

impl DesktopProcess {
    async fn spawn(data_dir: &Path, resources: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_coppice-server"))
            .arg("desktop")
            .arg("--data-dir")
            .arg(data_dir)
            .arg("--resources")
            .arg(resources)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn coppice-server desktop");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut process = Self {
            child,
            url: String::new(),
            pg_bin: resources.join("postgres").join("bin"),
            pg_lib: resources.join("postgres").join("lib"),
            pg_data: data_dir.join("pg").join("data"),
        };

        let mut lines = BufReader::new(stdout).lines();
        let url = tokio::time::timeout(READY_TIMEOUT, async {
            while let Some(line) = lines.next_line().await.expect("read stdout") {
                println!("[desktop] {line}");
                if let Some(url) = line.strip_prefix("COPPICE_READY url=") {
                    return url.to_string();
                }
            }
            panic!("coppice-server desktop exited before printing COPPICE_READY");
        })
        .await
        .expect("COPPICE_READY within 120 s");
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                println!("[desktop] {line}");
            }
        });
        process.url = url;
        process
    }

    fn close_stdin(&mut self) {
        drop(self.child.stdin.take());
    }

    fn sigterm(&self) {
        let pid = self.child.id().expect("child running");
        let status = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .expect("run kill");
        assert!(status.success());
    }

    async fn wait_exit(&mut self) -> ExitStatus {
        tokio::time::timeout(EXIT_TIMEOUT, self.child.wait())
            .await
            .expect("exit within 30 s")
            .expect("wait child")
    }
}

impl Drop for DesktopProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        if self.pg_data.join("postmaster.pid").exists() {
            let _ = std::process::Command::new(self.pg_bin.join("pg_ctl"))
                .env("LD_LIBRARY_PATH", &self.pg_lib)
                .arg("-D")
                .arg(&self.pg_data)
                .args(["-m", "immediate", "-w", "stop"])
                .stdin(Stdio::null())
                .status();
        }
    }
}

fn build_resources(root: &Path, pg_install: &Path) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resources = root.join("resources");
    std::fs::create_dir_all(resources.join("web")).unwrap();
    std::fs::create_dir_all(resources.join("fixtures")).unwrap();
    symlink(pg_install, resources.join("postgres")).unwrap();
    std::fs::write(resources.join("web").join("index.html"), INDEX_HTML).unwrap();
    symlink(
        manifest.join("agent_templates"),
        resources.join("agent-templates"),
    )
    .unwrap();
    symlink(
        manifest.join("../fixtures/agent-responses"),
        resources.join("fixtures").join("agent-responses"),
    )
    .unwrap();
    resources
}

async fn assert_serving(url: &str) {
    let client = reqwest::Client::new();
    let health = client
        .get(format!("{url}/health"))
        .send()
        .await
        .expect("GET /health");
    assert_eq!(health.status(), 200);
    let index = client.get(format!("{url}/")).send().await.expect("GET /");
    assert_eq!(index.status(), 200);
    assert_eq!(index.text().await.expect("index body"), INDEX_HTML);
}

#[tokio::test(flavor = "multi_thread")]
async fn desktop_starts_serves_and_stops_postgres_on_shutdown() {
    let pg_install = coppice_server::db::embedded_pg_install_dir()
        .await
        .expect("embedded postgres binaries");
    assert!(pg_install.join("bin").join("initdb").exists());

    let tmp = tempfile::tempdir().expect("tempdir");
    let resources = build_resources(tmp.path(), &pg_install);
    let data_dir = tmp.path().join("Application Support").join("Coppice");
    let pid_file = data_dir.join("pg").join("data").join("postmaster.pid");

    let mut first = DesktopProcess::spawn(&data_dir, &resources).await;
    assert!(first.url.starts_with("http://127.0.0.1:"), "{}", first.url);
    assert_serving(&first.url).await;
    first.close_stdin();
    let status = first.wait_exit().await;
    assert_eq!(status.code(), Some(0), "stdin EOF exit: {status}");
    assert!(!pid_file.exists(), "postgres still running after stdin EOF");
    drop(first);

    let config_file = data_dir.join("config.toml");
    let config_before = std::fs::read(&config_file).expect("config.toml written");
    let master_key_before = std::fs::read(data_dir.join("secrets").join("master_key")).unwrap();

    let mut second = DesktopProcess::spawn(&data_dir, &resources).await;
    assert_serving(&second.url).await;
    assert_eq!(std::fs::read(&config_file).unwrap(), config_before);
    assert_eq!(
        std::fs::read(data_dir.join("secrets").join("master_key")).unwrap(),
        master_key_before
    );
    second.sigterm();
    let status = second.wait_exit().await;
    assert_eq!(status.code(), Some(0), "SIGTERM exit: {status}");
    assert!(!pid_file.exists(), "postgres still running after SIGTERM");
}
