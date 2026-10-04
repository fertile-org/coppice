use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use anyhow::{bail, Context};
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection};
use tokio::process::Command;

use super::layout::{DataLayout, ResourceLayout};

const SUPERUSER: &str = "coppice";
const START_TIMEOUT_SECS: &str = "60";
const FAST_STOP_TIMEOUT_SECS: &str = "5";
const IMMEDIATE_STOP_TIMEOUT_SECS: &str = "3";
const START_LOG_TAIL_LINES: usize = 20;
/// TCP on loopback only; no Unix socket (macOS socket path length limits).
const LOOPBACK_SETTINGS: [(&str, &str); 2] = [
    ("listen_addresses", "127.0.0.1"),
    ("unix_socket_directories", ""),
];

/// The bundled Postgres cluster in `D/pg/data`, run from `R/postgres`.
#[derive(Debug, Clone)]
pub struct DesktopPostgres {
    pg_bin: PathBuf,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pg_lib: PathBuf,
    pg_data: PathBuf,
    pg_log: PathBuf,
}

impl DesktopPostgres {
    pub fn new(resources: &ResourceLayout, layout: &DataLayout) -> Self {
        Self {
            pg_bin: resources.pg_bin.clone(),
            pg_lib: resources.pg_lib.clone(),
            pg_data: layout.pg_data.clone(),
            pg_log: layout.pg_log.clone(),
        }
    }

    pub async fn bundled_major(&self) -> anyhow::Result<u32> {
        let output = self
            .run("postgres", |cmd| {
                cmd.arg("--version");
            })
            .await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_major(&stdout).with_context(|| {
            format!(
                "unrecognised `postgres --version` output: {}",
                stdout.trim()
            )
        })
    }

    /// Major version the cluster was initialised with; `None` before `initdb`.
    pub fn data_major(&self) -> anyhow::Result<Option<u32>> {
        let path = self.pg_data.join("PG_VERSION");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
        };
        contents
            .trim()
            .parse()
            .map(Some)
            .with_context(|| format!("{} is corrupt: {:?}", path.display(), contents.trim()))
    }

    /// Runs `initdb` when the cluster is uninitialised; returns true when it did.
    pub async fn init_if_needed(&self, password: &str) -> anyhow::Result<bool> {
        if self.data_major()?.is_some() {
            return Ok(false);
        }
        std::fs::create_dir_all(&self.pg_data)
            .with_context(|| format!("create {}", self.pg_data.display()))?;
        set_private_dir(&self.pg_data)
            .with_context(|| format!("chmod 0700 {}", self.pg_data.display()))?;

        let mut pwfile = tempfile::NamedTempFile::new().context("create initdb password file")?;
        pwfile
            .write_all(password.as_bytes())
            .and_then(|()| pwfile.flush())
            .context("write initdb password file")?;

        self.run("initdb", |cmd| {
            cmd.arg("-D")
                .arg(&self.pg_data)
                .args(["-U", SUPERUSER])
                .arg(format!("--pwfile={}", pwfile.path().display()))
                .args(["-A", "scram-sha-256", "-E", "UTF8", "--locale=C"]);
            for (key, value) in LOOPBACK_SETTINGS {
                cmd.arg("-c").arg(format!("{key}={value}"));
            }
        })
        .await?;
        Ok(true)
    }

    /// Appends any loopback-only setting that `postgresql.conf` does not
    /// currently end up with, so an edited or half-initialised config can
    /// never expose Postgres beyond `127.0.0.1` or open a Unix socket.
    pub fn ensure_loopback_config(&self) -> anyhow::Result<()> {
        let path = self.pg_data.join("postgresql.conf");
        let conf =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let missing: String = LOOPBACK_SETTINGS
            .iter()
            .filter(|(key, value)| last_setting(&conf, key).as_deref() != Some(*value))
            .map(|(key, value)| format!("{key} = '{value}'\n"))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        let separator = if conf.is_empty() || conf.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .with_context(|| format!("open {}", path.display()))?;
        file.write_all(
            format!(
                "{separator}# Coppice desktop: TCP on loopback only, no Unix socket.\n{missing}"
            )
            .as_bytes(),
        )
        .and_then(|()| file.sync_all())
        .with_context(|| format!("append {}", path.display()))
    }

    /// Removes `postmaster.pid` left by a crashed server; returns true when removed.
    pub fn clear_stale_pid(&self) -> anyhow::Result<bool> {
        let path = self.pg_data.join("postmaster.pid");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
        };
        let pid = contents
            .lines()
            .next()
            .and_then(|line| line.trim().parse::<u32>().ok());
        if let Some(pid) = pid {
            if is_live_postgres(pid)? {
                return Ok(false);
            }
        }
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        Ok(true)
    }

    pub async fn start(&self, port: u16) -> anyhow::Result<()> {
        self.ensure_loopback_config()?;
        let result = self
            .run("pg_ctl", |cmd| {
                cmd.arg("-D")
                    .arg(&self.pg_data)
                    .arg("-l")
                    .arg(&self.pg_log)
                    .arg("-o")
                    .arg(format!("-p {port}"))
                    .args(["-w", "-t", START_TIMEOUT_SECS, "start"]);
            })
            .await;
        if let Err(err) = result {
            let tail = log_tail(&self.pg_log, START_LOG_TAIL_LINES);
            return Err(err.context(format!(
                "postgres failed to start; last lines of {}:\n{tail}",
                self.pg_log.display()
            )));
        }
        Ok(())
    }

    /// Creates database `name` if missing and returns its connection URL.
    pub async fn ensure_database(
        &self,
        port: u16,
        password: &str,
        name: &str,
    ) -> anyhow::Result<String> {
        if !is_valid_db_name(name) {
            bail!("invalid database name {name:?}");
        }
        let mut conn = PgConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .username(SUPERUSER)
            .password(password)
            .database("postgres")
            .connect()
            .await
            .context("connect to bundled postgres")?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(name)
                .fetch_one(&mut conn)
                .await
                .context("check database exists")?;
        if !exists {
            sqlx::query(&format!("CREATE DATABASE \"{name}\""))
                .execute(&mut conn)
                .await
                .with_context(|| format!("create database {name}"))?;
        }
        conn.close().await.ok();
        Ok(format!(
            "postgres://{SUPERUSER}:{}@127.0.0.1:{port}/{name}",
            percent_encode(password)
        ))
    }

    /// Fast shutdown, falling back to immediate; Ok when the server is not running.
    /// Bounded by `FAST_STOP_TIMEOUT_SECS + IMMEDIATE_STOP_TIMEOUT_SECS`.
    pub async fn stop(&self) -> anyhow::Result<()> {
        if !self.is_running()? {
            return Ok(());
        }
        let Err(fast) = self.pg_ctl_stop("fast", FAST_STOP_TIMEOUT_SECS).await else {
            return Ok(());
        };
        tracing::warn!(error = %format!("{fast:#}"), "fast postgres stop failed; trying immediate");
        if !self.is_running()? {
            return Ok(());
        }
        self.pg_ctl_stop("immediate", IMMEDIATE_STOP_TIMEOUT_SECS)
            .await
            .with_context(|| format!("fast stop failed first: {fast:#}"))
    }

    fn is_running(&self) -> anyhow::Result<bool> {
        self.clear_stale_pid()?;
        Ok(self.pg_data.join("postmaster.pid").exists())
    }

    async fn pg_ctl_stop(&self, mode: &str, timeout_secs: &str) -> anyhow::Result<()> {
        self.run("pg_ctl", |cmd| {
            cmd.arg("-D")
                .arg(&self.pg_data)
                .args(["-m", mode, "-w", "-t", timeout_secs, "stop"]);
        })
        .await?;
        Ok(())
    }

    /// Runs a bundled binary to completion; a non-zero exit is an error carrying its output.
    async fn run(&self, bin: &str, configure: impl FnOnce(&mut Command)) -> anyhow::Result<Output> {
        let mut cmd = Command::new(self.pg_bin.join(bin));
        configure(&mut cmd);
        cmd.stdin(Stdio::null());
        #[cfg(target_os = "linux")]
        cmd.env("LD_LIBRARY_PATH", &self.pg_lib);
        let output = cmd
            .output()
            .await
            .with_context(|| format!("run {}", self.pg_bin.join(bin).display()))?;
        if !output.status.success() {
            bail!(failure_message(
                bin,
                &output.status.to_string(),
                &String::from_utf8_lossy(&output.stderr),
                &String::from_utf8_lossy(&output.stdout),
            ));
        }
        Ok(output)
    }
}

fn failure_message(bin: &str, status: &str, stderr: &str, stdout: &str) -> String {
    let mut message = format!("{bin} exited with {status}");
    let streams: Vec<&str> = [stderr.trim(), stdout.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if !streams.is_empty() {
        message.push_str(":\n");
        message.push_str(&streams.join("\n"));
    }
    message
}

/// Last `lines` lines of `path`; empty when unreadable.
fn log_tail(path: &Path, lines: usize) -> String {
    let contents = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = contents.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Effective value of `key` in a `postgresql.conf` body: the last uncommented
/// assignment wins, quotes stripped.
fn last_setting(conf: &str, key: &str) -> Option<String> {
    conf.lines().rev().find_map(|line| parse_setting(line, key))
}

fn parse_setting(line: &str, key: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(key)?;
    if !rest.starts_with(|c: char| c == '=' || c.is_whitespace()) {
        return None;
    }
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
    if let Some(quoted) = rest.strip_prefix('\'') {
        let end = quoted.find('\'')?;
        return Some(quoted[..end].to_string());
    }
    let value = rest.split('#').next().unwrap_or("").trim();
    Some(value.to_string())
}

/// Major version from `postgres --version`, e.g. `postgres (PostgreSQL) 16.4` → 16.
pub fn parse_major(version_output: &str) -> Option<u32> {
    let (_, rest) = version_output.split_once("(PostgreSQL)")?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn set_private_dir(path: &Path) -> io::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

fn is_live_postgres(pid: u32) -> anyhow::Result<bool> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .stdin(Stdio::null())
        .output()
        .context("run ps")?;
    Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).contains("postgres"))
}

fn is_valid_db_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;

    fn postgres_in(dir: &tempfile::TempDir) -> DesktopPostgres {
        let layout = DataLayout::new(&dir.path().join("Application Support").join("Coppice"));
        layout.ensure_dirs().expect("ensure dirs");
        DesktopPostgres::new(&ResourceLayout::new(&dir.path().join("res")), &layout)
    }

    #[test]
    fn parse_major_reads_release_and_prerelease_versions() {
        assert_eq!(parse_major("postgres (PostgreSQL) 16.4"), Some(16));
        assert_eq!(parse_major("postgres (PostgreSQL) 17beta1"), Some(17));
        assert_eq!(parse_major("garbage"), None);
    }

    #[test]
    fn data_major_is_none_until_pg_version_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        assert_eq!(pg.data_major().expect("empty dir"), None);

        std::fs::write(pg.pg_data.join("PG_VERSION"), "16\n").unwrap();
        assert_eq!(pg.data_major().expect("initialised"), Some(16));
    }

    #[test]
    fn clear_stale_pid_removes_dead_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        let pid_file = pg.pg_data.join("postmaster.pid");
        std::fs::write(&pid_file, "999999\n/data\n").unwrap();

        assert!(pg.clear_stale_pid().expect("clear"));
        assert!(!pid_file.exists());
    }

    #[test]
    fn clear_stale_pid_removes_live_non_postgres_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        let pid_file = pg.pg_data.join("postmaster.pid");
        std::fs::write(&pid_file, format!("{}\n/data\n", std::process::id())).unwrap();

        assert!(pg.clear_stale_pid().expect("clear"));
        assert!(!pid_file.exists());
    }

    #[test]
    fn clear_stale_pid_without_file_is_noop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        assert!(!pg.clear_stale_pid().expect("clear"));
    }

    const LOOPBACK_CONF: &str = "listen_addresses = '127.0.0.1'\nunix_socket_directories = ''\n";

    fn conf_after_ensure(dir: &tempfile::TempDir, initial: &str) -> String {
        let pg = postgres_in(dir);
        let conf = pg.pg_data.join("postgresql.conf");
        std::fs::write(&conf, initial).unwrap();
        pg.ensure_loopback_config().expect("ensure loopback");
        std::fs::read_to_string(&conf).unwrap()
    }

    fn assert_loopback_only(conf: &str) {
        assert_eq!(
            last_setting(conf, "listen_addresses").as_deref(),
            Some("127.0.0.1"),
            "{conf}"
        );
        assert_eq!(
            last_setting(conf, "unix_socket_directories").as_deref(),
            Some(""),
            "{conf}"
        );
    }

    #[test]
    fn ensure_loopback_config_appends_missing_settings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conf = conf_after_ensure(&dir, "max_connections = 100\n");
        assert!(conf.starts_with("max_connections = 100\n"));
        assert_loopback_only(&conf);
    }

    #[test]
    fn ensure_loopback_config_leaves_present_settings_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let initial = format!("#listen_addresses = 'localhost'\n{LOOPBACK_CONF}port = 5432 # x\n");
        assert_eq!(conf_after_ensure(&dir, &initial), initial);
    }

    #[test]
    fn ensure_loopback_config_appends_when_only_commented_out() {
        let dir = tempfile::tempdir().expect("tempdir");
        let initial = "#listen_addresses = '127.0.0.1'\n#unix_socket_directories = ''\n";
        let conf = conf_after_ensure(&dir, initial);
        assert!(conf.len() > initial.len());
        assert_loopback_only(&conf);
    }

    #[test]
    fn ensure_loopback_config_overrides_later_conflicting_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let initial = format!("{LOOPBACK_CONF}listen_addresses = '*'\n");
        assert_loopback_only(&conf_after_ensure(&dir, &initial));
    }

    #[tokio::test]
    async fn stop_with_stale_pid_is_ok_without_pg_ctl() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        let pid_file = pg.pg_data.join("postmaster.pid");
        std::fs::write(&pid_file, "999999\n").unwrap();

        pg.stop().await.expect("stale pid means not running");
        assert!(!pid_file.exists());
    }

    /// A live process named `postgres` recorded in `postmaster.pid`, plus a fake
    /// `pg_ctl` that logs its arguments, fails `fast` stops when `fail_fast`,
    /// and otherwise removes the pid file like a real stop.
    struct FakeRunningPostgres {
        pg: DesktopPostgres,
        log: PathBuf,
        process: std::process::Child,
    }

    impl FakeRunningPostgres {
        fn new(dir: &tempfile::TempDir, fail_fast: bool) -> Self {
            let pg = postgres_in(dir);
            std::fs::create_dir_all(&pg.pg_bin).unwrap();
            // A script (not a renamed binary) so `ps -o comm=` reports `postgres`;
            // no `exec`, which would rename the process to `sleep`.
            let fake_postgres = dir.path().join("postgres");
            std::fs::write(&fake_postgres, "#!/bin/sh\nwhile :; do sleep 1; done\n").unwrap();
            std::fs::set_permissions(&fake_postgres, std::fs::Permissions::from_mode(0o755))
                .unwrap();
            let process = std::process::Command::new(&fake_postgres)
                .process_group(0)
                .spawn()
                .unwrap();
            for _ in 0..50 {
                if is_live_postgres(process.id()).unwrap() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let pid_file = pg.pg_data.join("postmaster.pid");
            std::fs::write(&pid_file, format!("{}\n", process.id())).unwrap();

            let log = dir.path().join("pg_ctl.log");
            let fast_exit = if fail_fast { "exit 1" } else { "" };
            let script = format!(
                "#!/bin/sh\necho \"$*\" >> '{log}'\ncase \"$*\" in *fast*) {fast_exit};; esac\nrm -f '{pid}'\n",
                log = log.display(),
                pid = pid_file.display(),
            );
            let pg_ctl = pg.pg_bin.join("pg_ctl");
            std::fs::write(&pg_ctl, script).unwrap();
            std::fs::set_permissions(&pg_ctl, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self { pg, log, process }
        }

        fn assert_alive(&mut self) {
            assert!(
                self.process.try_wait().unwrap().is_none(),
                "fake postgres exited"
            );
            assert!(
                is_live_postgres(self.process.id()).unwrap(),
                "fake postgres not seen as postgres"
            );
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(&self.log)
                .unwrap_or_default()
                .lines()
                .map(|line| line.split(" -m ").nth(1).unwrap_or(line).to_string())
                .collect()
        }
    }

    impl Drop for FakeRunningPostgres {
        fn drop(&mut self) {
            // The whole group, so the script's `sleep` child goes too.
            let _ = std::process::Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", self.process.id())])
                .status();
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }

    #[tokio::test]
    async fn stop_uses_bounded_fast_shutdown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut fake = FakeRunningPostgres::new(&dir, false);
        fake.assert_alive();

        fake.pg.stop().await.expect("fast stop");
        fake.assert_alive();
        assert_eq!(fake.calls(), ["fast -w -t 5 stop"]);
    }

    #[tokio::test]
    async fn stop_falls_back_to_immediate_when_fast_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut fake = FakeRunningPostgres::new(&dir, true);
        fake.assert_alive();

        fake.pg.stop().await.expect("immediate stop");
        fake.assert_alive();
        assert_eq!(
            fake.calls(),
            ["fast -w -t 5 stop", "immediate -w -t 3 stop"]
        );
    }

    #[test]
    fn last_setting_ignores_comments_and_similar_keys() {
        let conf = "listen_addresses='a' # c\nlisten_addresses_x = 'b'\n# listen_addresses = 'c'\n";
        assert_eq!(last_setting(conf, "listen_addresses").as_deref(), Some("a"));
        assert_eq!(last_setting("port = 5 # x\n", "port").as_deref(), Some("5"));
        assert_eq!(last_setting("", "port"), None);
    }

    #[test]
    fn failure_message_omits_empty_streams() {
        assert_eq!(
            failure_message("initdb", "1", "  ", ""),
            "initdb exited with 1"
        );
        assert_eq!(
            failure_message("initdb", "1", "bad\n", " out "),
            "initdb exited with 1:\nbad\nout"
        );
        assert_eq!(
            failure_message("initdb", "1", "", "out"),
            "initdb exited with 1:\nout"
        );
    }

    #[test]
    fn log_tail_keeps_last_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("postgres.log");
        let lines: Vec<String> = (1..=30).map(|i| format!("line {i}")).collect();
        std::fs::write(&log, lines.join("\n")).unwrap();

        let tail = log_tail(&log, 20);
        assert!(tail.starts_with("line 11\n"), "{tail}");
        assert!(tail.ends_with("line 30"), "{tail}");
        assert_eq!(log_tail(&dir.path().join("missing.log"), 20), "");
    }

    #[test]
    fn db_names_must_be_plain_lowercase_identifiers() {
        assert!(is_valid_db_name("coppice"));
        assert!(is_valid_db_name("_test_1"));
        for bad in ["", "1db", "Coppice", "a-b", "a\"b", "a b"] {
            assert!(!is_valid_db_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn percent_encode_escapes_reserved_bytes() {
        assert_eq!(percent_encode("abc123-._~"), "abc123-._~");
        assert_eq!(percent_encode("p@ss:w/d%"), "p%40ss%3Aw%2Fd%25");
    }

    #[test]
    fn set_private_dir_makes_data_dir_owner_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        std::fs::set_permissions(&pg.pg_data, std::fs::Permissions::from_mode(0o755)).unwrap();

        set_private_dir(&pg.pg_data).expect("chmod");
        let mode = std::fs::metadata(&pg.pg_data).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
