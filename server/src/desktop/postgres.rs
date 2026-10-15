use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;

use anyhow::{bail, Context};
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection};
use tokio::process::Command;

use super::layout::{DataLayout, ResourceLayout};

const SUPERUSER: &str = "coppice";
const START_TIMEOUT_SECS: &str = "60";
pub(super) const FAST_STOP_TIMEOUT_SECS: &str = "5";
pub(super) const IMMEDIATE_STOP_TIMEOUT_SECS: &str = "3";
const START_LOG_TAIL_LINES: usize = 20;
/// TCP on loopback only; no Unix socket (macOS socket path length limits).
const LOOPBACK_SETTINGS: [(&str, &str); 2] = [
    ("listen_addresses", "127.0.0.1"),
    ("unix_socket_directories", ""),
];
/// Collector file names. The logs sweep treats a file newer than this age as
/// the one Postgres may still have open (`postgres_files::OPEN_WINDOW`).
const LOG_FILENAME: &str = "postgresql-%Y-%m-%d_%H%M%S.log";
const LOG_ROTATION_AGE: &str = "1d";
const LOG_ROTATION_SIZE: &str = "10MB";
/// How long to wait for `pg_ctl start` to notice SIGINT and exit.
pub(super) const START_INTERRUPT_WAIT: Duration = Duration::from_secs(2);
/// How long to wait for `postmaster.pid` if shutdown arrives before it exists.
pub(super) const START_PID_POLL: Duration = Duration::from_secs(1);

/// Result of [`DesktopPostgres::start`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartStatus {
    Started,
    Cancelled,
}

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

    /// Removes `postmaster.pid` when it does not name a live process whose
    /// command line names this data directory. Returns true when removed.
    /// An unreadable command line is left in place.
    pub fn clear_stale_pid(&self) -> anyhow::Result<bool> {
        if !matches!(self.classify_postmaster()?, PostmasterPid::Stale) {
            return Ok(false);
        }
        let path = self.pg_data.join("postmaster.pid");
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err).with_context(|| format!("remove {}", path.display())),
        }
    }

    /// Stops a postmaster still running on this data directory.
    /// Returns whether that postmaster was stopped.
    ///
    /// The pid comes from `postmaster.pid` in this data directory. It is
    /// stopped only when that process is alive and its command line can be
    /// read and names this data directory. A reused pid is not signalled.
    /// An unreadable command line is left running: the pid may belong to an
    /// unrelated process. Processes are not matched by name.
    pub(super) async fn stop_leftover(&self) -> anyhow::Result<bool> {
        match self.classify_postmaster()? {
            PostmasterPid::Ours => {
                self.stop_running().await?;
                Ok(true)
            }
            PostmasterPid::Unreadable { pid, error } => {
                tracing::warn!(
                    error = %error,
                    pid,
                    "could not read the postmaster command line; leaving it running"
                );
                Ok(false)
            }
            PostmasterPid::Stale => {
                self.clear_stale_pid()?;
                Ok(false)
            }
            PostmasterPid::Missing => Ok(false),
        }
    }

    pub async fn start(
        &self,
        port: u16,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<StartStatus> {
        self.ensure_loopback_config()?;
        self.ensure_logging_config()?;
        if *cancel.borrow() {
            return Ok(StartStatus::Cancelled);
        }
        let log_dir = self.log_directory();
        let mut child = self
            .command("pg_ctl", |cmd| {
                cmd.arg("-D")
                    .arg(&self.pg_data)
                    .arg("-l")
                    .arg(&self.pg_log)
                    .arg("-o")
                    .arg(server_options(port, &log_dir))
                    .args(["-w", "-t", START_TIMEOUT_SECS, "start"]);
            })
            .spawn()
            .with_context(|| format!("run {}", self.pg_bin.join("pg_ctl").display()))?;
        let status = tokio::select! {
            biased;
            // Drop the watch guard before any later await. Holding it across
            // `interrupt_pg_ctl` makes this future `!Send`.
            () = async {
                let _ = cancel.wait_for(|requested| *requested).await;
            } => {
                interrupt_pg_ctl(&mut child).await;
                self.ensure_stopped_after_cancel().await?;
                return Ok(StartStatus::Cancelled);
            }
            status = child.wait() => status,
        };
        let status = status.context("wait for pg_ctl start")?;
        if !status.success() {
            let tail = startup_log_excerpt(&self.pg_log);
            bail!(
                "pg_ctl exited with {status}; last lines of {}:\n{tail}",
                self.pg_log.display()
            );
        }
        if *cancel.borrow() {
            self.ensure_stopped_after_cancel().await?;
            return Ok(StartStatus::Cancelled);
        }
        Ok(StartStatus::Started)
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
        self.stop_leftover().await?;
        Ok(())
    }

    async fn stop_running(&self) -> anyhow::Result<()> {
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

    /// Appends collector settings so Postgres rotates logs into the logs
    /// folder. Command-line options in [`server_options`] still win.
    pub fn ensure_logging_config(&self) -> anyhow::Result<()> {
        let log_dir = self.log_directory();
        let path = self.pg_data.join("postgresql.conf");
        let conf =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let missing: String = logging_settings(&log_dir)
            .iter()
            .filter(|setting| {
                last_setting(&conf, setting.key).as_deref() != Some(setting.value.as_str())
            })
            .map(|setting| format!("{} = {}\n", setting.key, setting.rendered))
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
                "{separator}# Coppice desktop: rotate database logs into the logs folder.\n{missing}"
            )
            .as_bytes(),
        )
        .and_then(|()| file.sync_all())
        .with_context(|| format!("append {}", path.display()))
    }

    fn log_directory(&self) -> PathBuf {
        let dir = self.pg_log.parent().unwrap_or_else(|| Path::new("."));
        std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
    }

    /// What `postmaster.pid` in this data directory refers to.
    /// Identity is the command line, never the process name.
    fn classify_postmaster(&self) -> anyhow::Result<PostmasterPid> {
        let path = self.pg_data.join("postmaster.pid");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(PostmasterPid::Missing);
            }
            Err(err) => {
                return Err(err).with_context(|| format!("read {}", path.display()));
            }
        };
        let Some(pid) = contents
            .lines()
            .next()
            .and_then(|line| line.trim().parse::<u32>().ok())
        else {
            return Ok(PostmasterPid::Stale);
        };
        match pid_is_alive(pid) {
            Ok(true) => {}
            Ok(false) => return Ok(PostmasterPid::Stale),
            Err(error) => return Ok(PostmasterPid::Unreadable { pid, error }),
        }
        match process_args(pid) {
            Ok(args) if args_contain_data_dir(&args, &self.pg_data) => Ok(PostmasterPid::Ours),
            Ok(_) => Ok(PostmasterPid::Stale),
            Err(error) => Ok(PostmasterPid::Unreadable { pid, error }),
        }
    }

    async fn ensure_stopped_after_cancel(&self) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + START_PID_POLL;
        loop {
            if self.is_running()? {
                tracing::info!("shutdown during postgres start; stopping the database");
                return self.stop().await;
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
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

    fn command(&self, bin: &str, configure: impl FnOnce(&mut Command)) -> Command {
        let mut cmd = Command::new(self.pg_bin.join(bin));
        configure(&mut cmd);
        cmd.stdin(Stdio::null());
        #[cfg(target_os = "linux")]
        cmd.env("LD_LIBRARY_PATH", &self.pg_lib);
        cmd
    }

    /// Runs a bundled binary to completion; a non-zero exit is an error carrying its output.
    async fn run(&self, bin: &str, configure: impl FnOnce(&mut Command)) -> anyhow::Result<Output> {
        let output = self
            .command(bin, configure)
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

/// `pg_ctl -o` value. Command-line settings beat every config file (includes,
/// `postgresql.auto.conf`), so loopback and log rotation hold whatever the
/// files say. pg_ctl passes this through `/bin/sh`, where an unquoted `name=`
/// is an empty value.
fn server_options(port: u16, log_directory: &Path) -> String {
    let mut options = format!("-p {port}");
    for (key, value) in LOOPBACK_SETTINGS {
        options.push_str(&format!(" -c {key}={value}"));
    }
    for setting in logging_settings(log_directory) {
        let rendered = if setting.shell_quote {
            shell_quote(&setting.value)
        } else {
            setting.value.clone()
        };
        options.push_str(&format!(" -c {}={rendered}", setting.key));
    }
    options
}

struct LoggingSetting {
    key: &'static str,
    /// Value as [`last_setting`] returns it.
    value: String,
    /// Text written after `=` in `postgresql.conf`.
    rendered: String,
    shell_quote: bool,
}

fn logging_settings(log_directory: &Path) -> Vec<LoggingSetting> {
    let dir = log_directory.to_string_lossy().into_owned();
    vec![
        LoggingSetting {
            key: "logging_collector",
            value: "on".to_string(),
            rendered: "on".to_string(),
            shell_quote: false,
        },
        LoggingSetting {
            key: "log_directory",
            value: dir.clone(),
            rendered: conf_quote(&dir),
            shell_quote: true,
        },
        LoggingSetting {
            key: "log_filename",
            value: LOG_FILENAME.to_string(),
            rendered: conf_quote(LOG_FILENAME),
            shell_quote: true,
        },
        LoggingSetting {
            key: "log_rotation_age",
            value: LOG_ROTATION_AGE.to_string(),
            rendered: LOG_ROTATION_AGE.to_string(),
            shell_quote: false,
        },
        LoggingSetting {
            key: "log_rotation_size",
            value: LOG_ROTATION_SIZE.to_string(),
            rendered: LOG_ROTATION_SIZE.to_string(),
            shell_quote: false,
        },
        LoggingSetting {
            key: "log_truncate_on_rotation",
            value: "off".to_string(),
            rendered: "off".to_string(),
            shell_quote: false,
        },
    ]
}

/// Single quotes for `/bin/sh`.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Single quotes for `postgresql.conf` (a quote inside is doubled).
fn conf_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn interrupt_pg_ctl(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(pid as i32, libc::SIGINT);
        }
    }
    if tokio::time::timeout(START_INTERRUPT_WAIT, child.wait())
        .await
        .is_err()
    {
        let _ = child.start_kill();
        let _ = child.wait().await;
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

fn process_args(pid: u32) -> anyhow::Result<String> {
    let output = ps_command()
        .args(["-ww", "-p", &pid.to_string(), "-o", "args="])
        .stdin(Stdio::null())
        .output()
        .context("run ps")?;
    if !output.status.success() {
        bail!("ps exited with {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn args_contain_data_dir(args: &str, data: &Path) -> bool {
    args.contains(&*data.to_string_lossy())
}

/// Last lines of `pg_ctl -l` and of the newest rotated collector file.
fn startup_log_excerpt(pg_log: &Path) -> String {
    let mut parts = Vec::new();
    let legacy = log_tail(pg_log, START_LOG_TAIL_LINES);
    if !legacy.is_empty() {
        parts.push(legacy);
    }
    if let Some(dir) = pg_log.parent() {
        if let Ok(files) = crate::logs::postgres_files::list(dir) {
            if let Some(newest) = files
                .iter()
                .filter(|file| file.opened.is_some())
                .max_by_key(|file| file.opened)
            {
                let tail = log_tail(&newest.path, START_LOG_TAIL_LINES);
                if !tail.is_empty() {
                    parts.push(tail);
                }
            }
        }
    }
    parts.join("\n")
}

/// Last `lines` lines of `path`; empty when unreadable.
fn log_tail(path: &Path, lines: usize) -> String {
    let contents = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = contents.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Value inside a `postgresql.conf` single-quoted string. `''` is one quote.
fn unquote_conf(quoted: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = quoted.chars();
    while let Some(ch) = chars.next() {
        if ch == '\'' {
            if chars.as_str().starts_with('\'') {
                chars.next();
                out.push('\'');
            } else {
                return Some(out);
            }
        } else {
            out.push(ch);
        }
    }
    None
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
        return unquote_conf(quoted);
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

enum PostmasterPid {
    Missing,
    /// No live process, or a live one whose command line does not name this data dir.
    Stale,
    /// Alive, but `ps` could not show the command line. Do not signal it.
    Unreadable {
        pid: u32,
        error: anyhow::Error,
    },
    /// Alive and the command line names this data dir.
    Ours,
}

fn pid_is_alive(pid: u32) -> anyhow::Result<bool> {
    let output = ps_command()
        .args(["-p", &pid.to_string()])
        .stdin(Stdio::null())
        .output()
        .context("run ps")?;
    Ok(output.status.success())
}

fn ps_command() -> std::process::Command {
    #[cfg(test)]
    {
        if let Some(path) = PS_BIN.with(|cell| cell.borrow().clone()) {
            return std::process::Command::new(path);
        }
    }
    std::process::Command::new("ps")
}

#[cfg(test)]
std::thread_local! {
    static PS_BIN: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
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
    use tracing_subscriber::layer::SubscriberExt;

    struct WarnCapture(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
    struct WarnVisit(String);

    impl tracing::field::Visit for WarnVisit {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "message" {
                self.0 = value.to_string();
            }
        }

        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" && self.0.is_empty() {
                self.0 = format!("{value:?}");
            }
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for WarnCapture {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if *event.metadata().level() > tracing::Level::WARN {
                return;
            }
            let mut visit = WarnVisit(String::new());
            event.record(&mut visit);
            if !visit.0.is_empty() {
                self.0.lock().expect("warnings").push(visit.0);
            }
        }
    }

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

    /// A live process whose command line names this data directory, plus a fake
    /// `pg_ctl` that logs its arguments, fails `fast` stops when `fail_fast`,
    /// and otherwise removes the pid file like a real stop. The process is not
    /// named `postgres`.
    struct FakeRunningPostgres {
        pg: DesktopPostgres,
        log: PathBuf,
        process: std::process::Child,
    }

    impl FakeRunningPostgres {
        fn new(dir: &tempfile::TempDir, fail_fast: bool) -> Self {
            let pg = postgres_in(dir);
            std::fs::create_dir_all(&pg.pg_bin).unwrap();
            // Not named `postgres`: identity is the `-D <data dir>` argument.
            // No `exec`, which would drop that argument from the command line.
            let sleeper = dir.path().join("sleeper");
            std::fs::write(&sleeper, "#!/bin/sh\nwhile :; do sleep 1; done\n").unwrap();
            std::fs::set_permissions(&sleeper, std::fs::Permissions::from_mode(0o755)).unwrap();
            let process = std::process::Command::new(&sleeper)
                .arg("-D")
                .arg(&pg.pg_data)
                .process_group(0)
                .spawn()
                .unwrap();
            for _ in 0..50 {
                if pid_is_alive(process.id()).unwrap() {
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
                pid_is_alive(self.process.id()).unwrap(),
                "sleeper is not running"
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

    #[tokio::test]
    async fn stop_does_not_signal_a_reused_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        std::fs::create_dir_all(&pg.pg_bin).unwrap();
        let fake_postgres = dir.path().join("postgres");
        std::fs::write(&fake_postgres, "#!/bin/sh\nwhile :; do sleep 1; done\n").unwrap();
        std::fs::set_permissions(&fake_postgres, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut process = std::process::Command::new(&fake_postgres)
            .process_group(0)
            .spawn()
            .unwrap();
        for _ in 0..50 {
            if pid_is_alive(process.id()).unwrap() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let pid_file = pg.pg_data.join("postmaster.pid");
        std::fs::write(&pid_file, format!("{}\n", process.id())).unwrap();
        let log = dir.path().join("pg_ctl.log");
        let script = format!("#!/bin/sh\necho \"$*\" >> '{}'\nexit 1\n", log.display());
        let pg_ctl = pg.pg_bin.join("pg_ctl");
        std::fs::write(&pg_ctl, script).unwrap();
        std::fs::set_permissions(&pg_ctl, std::fs::Permissions::from_mode(0o755)).unwrap();

        pg.stop().await.expect("reused pid is not our server");

        assert!(
            process.try_wait().unwrap().is_none(),
            "reused pid was signalled"
        );
        assert!(!pid_file.exists(), "stale pid file should be removed");
        assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), "");
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", process.id())])
            .status();
        let _ = process.wait();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stop_leaves_an_unreadable_command_line_running() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        std::fs::create_dir_all(&pg.pg_bin).unwrap();
        let mut process = std::process::Command::new("sleep")
            .arg("300")
            .process_group(0)
            .spawn()
            .unwrap();
        let pid_file = pg.pg_data.join("postmaster.pid");
        std::fs::write(&pid_file, format!("{}\n", process.id())).unwrap();
        let log = dir.path().join("pg_ctl.log");
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nkill -KILL {} 2>/dev/null || true\nexit 1\n",
            log.display(),
            process.id(),
        );
        let pg_ctl = pg.pg_bin.join("pg_ctl");
        std::fs::write(&pg_ctl, script).unwrap();
        std::fs::set_permissions(&pg_ctl, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ps = dir.path().join("ps");
        std::fs::write(
            &ps,
            "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"args=\" ]; then\n    echo denied >&2\n    exit 1\n  fi\ndone\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&ps, std::fs::Permissions::from_mode(0o755)).unwrap();

        PS_BIN.with(|cell| *cell.borrow_mut() = Some(ps));
        struct ClearPs;
        impl Drop for ClearPs {
            fn drop(&mut self) {
                PS_BIN.with(|cell| *cell.borrow_mut() = None);
            }
        }
        let _clear_ps = ClearPs;

        let warnings = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(WarnCapture(warnings.clone()));
        let _tracing = tracing::subscriber::set_default(subscriber);

        pg.stop()
            .await
            .expect("unreadable command line is left running");

        assert!(
            process.try_wait().unwrap().is_none(),
            "unreadable pid was signalled"
        );
        assert!(
            pid_file.exists(),
            "pid file should stay when the command line cannot be read"
        );
        assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), "");
        let recorded = warnings.lock().expect("warnings").clone();
        assert!(
            recorded.iter().any(|line| {
                line == "could not read the postmaster command line; leaving it running"
            }),
            "{recorded:?}"
        );
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", process.id())])
            .status();
        let _ = process.wait();
    }

    #[tokio::test]
    async fn shutdown_during_start_stops_postgres() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        std::fs::write(pg.pg_data.join("postgresql.conf"), "\n").unwrap();
        std::fs::create_dir_all(&pg.pg_bin).unwrap();
        let fake_postgres = dir.path().join("postgres");
        std::fs::write(&fake_postgres, "#!/bin/sh\nwhile :; do sleep 1; done\n").unwrap();
        std::fs::set_permissions(&fake_postgres, std::fs::Permissions::from_mode(0o755)).unwrap();
        let log = dir.path().join("pg_ctl.log");
        let pid_file = pg.pg_data.join("postmaster.pid");
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> {log}\ncase \"$*\" in\n  *start*)\n    if command -v setsid >/dev/null 2>&1; then\n      setsid {postgres} -D {data} >/dev/null 2>&1 &\n    else\n      {postgres} -D {data} >/dev/null 2>&1 &\n    fi\n    echo $! > {pid}\n    while :; do sleep 1; done\n    ;;\n  *stop*)\n    if [ -f {pid} ]; then\n      target=$(head -n 1 {pid})\n      kill -KILL -- \"-$target\" 2>/dev/null || kill -KILL \"$target\" 2>/dev/null || true\n      rm -f {pid}\n    fi\n    ;;\nesac\n",
            log = shell_quote(&log.to_string_lossy()),
            postgres = shell_quote(&fake_postgres.to_string_lossy()),
            data = shell_quote(&pg.pg_data.to_string_lossy()),
            pid = shell_quote(&pid_file.to_string_lossy()),
        );
        let pg_ctl = pg.pg_bin.join("pg_ctl");
        std::fs::write(&pg_ctl, &script).unwrap();
        std::fs::set_permissions(&pg_ctl, std::fs::Permissions::from_mode(0o755)).unwrap();

        let (tx, rx) = tokio::sync::watch::channel(false);
        let started = tokio::spawn({
            let pg = pg.clone();
            async move { pg.start(5432, rx).await }
        });
        let sleeper = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Ok(text) = std::fs::read_to_string(&pid_file) {
                    if let Some(pid) = text
                        .lines()
                        .next()
                        .and_then(|line| line.trim().parse::<u32>().ok())
                    {
                        if pid_is_alive(pid).unwrap_or(false)
                            && process_args(pid)
                                .ok()
                                .is_some_and(|args| args_contain_data_dir(&args, &pg.pg_data))
                        {
                            return pid;
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("sleeper postgres did not appear");
        tx.send(true).expect("cancel");
        let status = tokio::time::timeout(std::time::Duration::from_secs(8), started)
            .await
            .expect("start returned")
            .expect("join")
            .expect("start");
        assert_eq!(status, StartStatus::Cancelled);
        assert!(!pid_file.exists(), "postmaster.pid still present");
        assert!(
            !pid_is_alive(sleeper).unwrap(),
            "postgres kept running after shutdown during start"
        );
        let calls = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(
            calls.lines().any(|line| line.contains("stop")),
            "pg_ctl stop was not run:\n{calls}"
        );
    }

    #[test]
    fn server_options_force_loopback_and_log_rotation_on_every_start() {
        let dir = Path::new("/Library/Application Support/Coppice/logs");
        let options = server_options(5433, dir);
        assert!(
            options
                .starts_with("-p 5433 -c listen_addresses=127.0.0.1 -c unix_socket_directories="),
            "{options}"
        );
        assert!(options.contains(" -c logging_collector=on"), "{options}");
        assert!(
            options.contains(&format!(
                " -c log_directory={}",
                shell_quote(dir.to_str().unwrap())
            )),
            "{options}"
        );
        assert!(
            options.contains(&format!(" -c log_filename={}", shell_quote(LOG_FILENAME))),
            "{options}"
        );
        assert!(options.contains(" -c log_rotation_age=1d"), "{options}");
        assert!(options.contains(" -c log_rotation_size=10MB"), "{options}");
        assert!(
            options.contains(" -c log_truncate_on_rotation=off"),
            "{options}"
        );
        let quoted = server_options(1, Path::new("/tmp/Coppice's logs"));
        assert!(quoted.contains("'/tmp/Coppice'\\''s logs'"), "{quoted}");
    }

    #[test]
    fn ensure_logging_config_appends_once_and_overrides_a_later_off() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        let conf = pg.pg_data.join("postgresql.conf");
        std::fs::write(&conf, "logging_collector = off\n").unwrap();

        pg.ensure_logging_config().expect("append");
        let once = std::fs::read_to_string(&conf).unwrap();
        assert!(
            once.contains("# Coppice desktop: rotate database logs into the logs folder."),
            "{once}"
        );
        assert_eq!(
            last_setting(&once, "logging_collector").as_deref(),
            Some("on")
        );
        assert_eq!(
            last_setting(&once, "log_directory").as_deref(),
            Some(pg.log_directory().to_str().unwrap())
        );
        assert_eq!(
            last_setting(&once, "log_filename").as_deref(),
            Some(LOG_FILENAME)
        );
        assert_eq!(
            last_setting(&once, "log_rotation_age").as_deref(),
            Some("1d")
        );
        assert_eq!(
            last_setting(&once, "log_rotation_size").as_deref(),
            Some("10MB")
        );
        assert_eq!(
            last_setting(&once, "log_truncate_on_rotation").as_deref(),
            Some("off")
        );

        pg.ensure_logging_config().expect("second");
        assert_eq!(std::fs::read_to_string(&conf).unwrap(), once);

        let quoted_logs = dir.path().join("Coppice's logs");
        std::fs::create_dir_all(&quoted_logs).unwrap();
        let layout = DataLayout::with_logs_dir(&dir.path().join("quoted-data"), quoted_logs);
        layout.ensure_dirs().expect("quoted dirs");
        let quoted = DesktopPostgres::new(&ResourceLayout::new(&dir.path().join("res")), &layout);
        let quoted_conf = quoted.pg_data.join("postgresql.conf");
        std::fs::write(&quoted_conf, "").unwrap();
        quoted.ensure_logging_config().expect("quoted append");
        quoted.ensure_logging_config().expect("quoted second");
        let quoted_text = std::fs::read_to_string(&quoted_conf).unwrap();
        assert_eq!(
            quoted_text.matches("log_directory").count(),
            1,
            "{quoted_text}"
        );
        assert_eq!(
            last_setting(&quoted_text, "log_directory").as_deref(),
            Some(quoted.log_directory().to_str().unwrap())
        );
    }

    #[test]
    fn last_setting_ignores_comments_and_similar_keys() {
        let conf = "listen_addresses='a' # c\nlisten_addresses_x = 'b'\n# listen_addresses = 'c'\n";
        assert_eq!(last_setting(conf, "listen_addresses").as_deref(), Some("a"));
        assert_eq!(last_setting("port = 5 # x\n", "port").as_deref(), Some("5"));
        assert_eq!(last_setting("", "port"), None);
        assert_eq!(
            last_setting("log_directory = '/tmp/Coppice''s logs'\n", "log_directory").as_deref(),
            Some("/tmp/Coppice's logs")
        );
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
