#![cfg(all(unix, feature = "embedded-test-db"))]

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

const READY_TIMEOUT: Duration = Duration::from_secs(120);
const EXIT_TIMEOUT: Duration = Duration::from_secs(30);
/// Electron SIGKILLs the server this long after SIGTERM.
const SIGTERM_GRACE: Duration = Duration::from_secs(15);
const INDEX_HTML: &str = "<!doctype html><title>coppice desktop test</title>\n";

/// A running `coppice-server desktop`; killed (and its Postgres stopped) if dropped early.
struct DesktopProcess {
    child: Child,
    /// Held outside `child`: `Child::wait` closes the child's stdin, which
    /// would trigger the stdin-EOF shutdown instead of the one under test.
    stdin: Option<ChildStdin>,
    rest_of_stdout: Option<tokio::task::JoinHandle<Vec<String>>>,
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
        let stdin = child.stdin.take();
        let mut process = Self {
            child,
            stdin,
            rest_of_stdout: None,
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
        process.rest_of_stdout = Some(tokio::spawn(async move {
            let mut rest = Vec::new();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("[desktop] {line}");
                rest.push(line);
            }
            rest
        }));
        process.url = url;
        process
    }

    /// Stdout after the ready line; call once the child has exited.
    async fn stdout_after_ready(&mut self) -> String {
        let task = self.rest_of_stdout.take().expect("stdout reader");
        tokio::time::timeout(EXIT_TIMEOUT, task)
            .await
            .expect("stdout closes after exit")
            .expect("stdout reader")
            .join("\n")
    }

    fn close_stdin(&mut self) {
        drop(self.stdin.take());
    }

    fn sigterm(&self) {
        let pid = self.child.id().expect("child running");
        let status = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .expect("run kill");
        assert!(status.success());
    }

    async fn wait_exit(&mut self, timeout: Duration) -> ExitStatus {
        tokio::time::timeout(timeout, self.child.wait())
            .await
            .unwrap_or_else(|_| panic!("exit within {timeout:?}"))
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
    symlink(pg_install, resources.join("postgres")).unwrap();
    std::fs::write(resources.join("web").join("index.html"), INDEX_HTML).unwrap();
    symlink(
        manifest.join("agent_templates"),
        resources.join("agent-templates"),
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

/// A login request whose body never finishes arriving, so graceful shutdown
/// cannot drain it; the returned stream keeps it open.
async fn hold_request_open(url: &str) -> tokio::net::TcpStream {
    let addr = url.strip_prefix("http://").expect("http url");
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let head = format!(
        "POST /api/auth/login HTTP/1.1\r\nHost: {addr}\r\n\
         Content-Type: application/json\r\nContent-Length: 1024\r\n\r\n{{\"email\":"
    );
    stream.write_all(head.as_bytes()).await.expect("send head");
    stream.flush().await.expect("flush");
    tokio::time::sleep(Duration::from_millis(300)).await;
    stream
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
    let status = first.wait_exit(EXIT_TIMEOUT).await;
    assert_eq!(status.code(), Some(0), "stdin EOF exit: {status}");
    assert!(!pid_file.exists(), "postgres still running after stdin EOF");
    let log = first.stdout_after_ready().await;
    assert!(log.contains("stdin closed"), "{log}");
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
    let _in_flight = hold_request_open(&second.url).await;
    second.sigterm();
    let status = second.wait_exit(SIGTERM_GRACE).await;
    assert_eq!(status.code(), Some(0), "SIGTERM exit: {status}");
    assert!(!pid_file.exists(), "postgres still running after SIGTERM");
    let log = second.stdout_after_ready().await;
    assert!(log.contains("SIGTERM received"), "{log}");
    assert!(log.contains("did not drain in time"), "{log}");
}
