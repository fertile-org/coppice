//! The user's login environment, shared by every process Coppice spawns.
//!
//! Desktop launchers often start the server with a short `PATH`. This module
//! reads `$SHELL -ilc` once (marker, `env -0`, marker), merges the usual bin
//! directories, and keeps the snapshot for about a minute. Refresh models,
//! connector checks, and run retry call [`invalidate`] so a newly installed
//! CLI is visible without restarting.
//!
//! Model lists use [`MODEL_LIST_TIMEOUT`] and [`resolve_model_program`], the
//! same names the agent model picker resolves a CLI with.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use coppice_connectors::probe::{augment_path, resolve_binary};

/// Login-shell capture. A slow `~/.zshrc` must fall back inside this bound.
pub const SHELL_TIMEOUT: Duration = Duration::from_secs(8);
/// How long a captured environment is reused.
pub const CACHE_TTL: Duration = Duration::from_secs(60);
/// Model lists hit the network. Cursor's `agent models` often takes several
/// seconds; a shorter bound fails the picker while a Ready check can still pass.
pub const MODEL_LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// Backstop for `git` after `GIT_TERMINAL_PROMPT=0`. A credential prompt must
/// not wait forever.
pub const GIT_TIMEOUT: Duration = Duration::from_secs(120);

const MARKER: &str = "__COPPICE_ENV__";

struct Slot {
    generation: u64,
    env: Option<Arc<HostEnv>>,
    /// One login-shell capture at a time. Parallel callers wait for that snapshot.
    capturing: bool,
}

static SLOT: Mutex<Slot> = Mutex::new(Slot {
    generation: 0,
    env: None,
    capturing: false,
});

/// PATH, HOME, and the rest of the login environment at [`HostEnv::read_at`].
#[derive(Debug, Clone)]
pub struct HostEnv {
    pub path: OsString,
    pub home: PathBuf,
    pub vars: Vec<(OsString, OsString)>,
    pub read_at: Instant,
}

/// `name` is not an executable on the directories that were searched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound {
    pub name: String,
    pub dirs: Vec<PathBuf>,
}

impl std::fmt::Display for NotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let dirs = if self.dirs.is_empty() {
            "(no directories)".to_string()
        } else {
            self.dirs
                .iter()
                .map(|dir| dir.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        write!(f, "`{}` wasn't found. Looked in: {dirs}", self.name)
    }
}

impl std::error::Error for NotFound {}

/// One finished command. `stderr_line` is the first non-empty stderr line.
#[derive(Debug)]
pub struct Exit {
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stderr_line: Option<String>,
}

impl Exit {
    pub fn success(&self) -> bool {
        self.status.is_some_and(|status| status.success())
    }
}

/// Result of [`HostEnv::run_with_timeout`].
#[derive(Debug)]
pub enum RunOutput {
    NotFound(NotFound),
    TimedOut { stderr_line: Option<String> },
    Exit(Exit),
}

/// Cached snapshot, refreshing when it is older than [`CACHE_TTL`].
///
/// Only one caller captures the login shell. The rest wait and reuse that snapshot,
/// so a burst of spawns does not start a burst of login shells.
pub fn current() -> Arc<HostEnv> {
    loop {
        let mut slot = lock();
        if let Some(hit) = slot
            .env
            .as_ref()
            .filter(|env| env.read_at.elapsed() < CACHE_TTL)
        {
            return Arc::clone(hit);
        }
        if slot.capturing {
            drop(slot);
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        slot.capturing = true;
        let generation = slot.generation;
        drop(slot);

        let built = Arc::new(capture_from_process());

        let mut slot = lock();
        slot.capturing = false;
        if slot.generation == generation {
            slot.env = Some(Arc::clone(&built));
        }
        return built;
    }
}

/// Drops the cached snapshot. The next [`current`] reads the login shell again.
pub fn invalidate() {
    let mut slot = lock();
    slot.generation = slot.generation.wrapping_add(1);
    slot.env = None;
}

/// Absolute path when `command` is on `path`, otherwise the name unchanged.
///
/// The agent model picker calls this the same way `resolve_model_program` did:
/// pass the connector's configured command and the host PATH.
pub fn resolve_model_program(command: &str, path: &OsStr) -> PathBuf {
    resolve_in(path, command).unwrap_or_else(|_| PathBuf::from(command))
}

/// First non-empty line of CLI output, trimmed to about 200 characters.
pub fn first_text_line(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line.replace('\0', "");
    if line.is_empty() {
        return None;
    }
    const MAX: usize = 200;
    if line.chars().count() <= MAX {
        Some(line)
    } else {
        Some(format!("{}…", line.chars().take(MAX).collect::<String>()))
    }
}

/// First non-empty line of a stderr buffer, capped so a UI row stays short.
pub fn first_stderr_line(stderr: &[u8]) -> Option<String> {
    first_text_line(&String::from_utf8_lossy(stderr))
}

impl HostEnv {
    pub fn var(&self, key: &str) -> Option<&OsStr> {
        self.vars
            .iter()
            .find(|(name, _)| name.as_os_str() == key)
            .map(|(_, value)| value.as_os_str())
    }

    pub fn var_string(&self, key: &str) -> Option<String> {
        self.var(key)
            .map(|value| value.to_string_lossy().into_owned())
            .filter(|value| !value.is_empty())
    }

    /// Absolute path, or [`NotFound`] listing every directory that was searched.
    pub fn resolve(&self, name: &str) -> Result<PathBuf, NotFound> {
        resolve_in(&self.path, name)
    }

    /// `std::process::Command` for `name` with an absolute program and explicit
    /// `PATH` and `HOME`.
    pub fn command<S: AsRef<OsStr>>(
        &self,
        name: &str,
        args: &[S],
    ) -> Result<std::process::Command, NotFound> {
        let program = self.resolve(name)?;
        let mut cmd = std::process::Command::new(program);
        cmd.args(args.iter().map(AsRef::as_ref));
        apply_host(&mut cmd, self);
        Ok(cmd)
    }

    /// Tokio command with the same program, `PATH`, and `HOME` as [`command`](Self::command).
    pub fn tokio_command<S: AsRef<OsStr>>(
        &self,
        name: &str,
        args: &[S],
    ) -> Result<tokio::process::Command, NotFound> {
        let program = self.resolve(name)?;
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args.iter().map(AsRef::as_ref));
        apply_host_tokio(&mut cmd, self);
        Ok(cmd)
    }

    /// Run `name` and stop it when `timeout` elapses.
    pub async fn run_with_timeout<S>(&self, name: &str, args: &[S], timeout: Duration) -> RunOutput
    where
        S: AsRef<OsStr>,
    {
        let mut cmd = match self.tokio_command(name, args) {
            Ok(cmd) => cmd,
            Err(missing) => return RunOutput::NotFound(missing),
        };
        cmd.stdin(Stdio::null());
        finish(cmd, timeout).await
    }
}

fn resolve_in(path: &OsStr, name: &str) -> Result<PathBuf, NotFound> {
    if let Some(found) = resolve_binary(name, path) {
        return Ok(found);
    }
    let dirs = if name.is_empty() {
        Vec::new()
    } else if name.contains('/') || name.contains(std::path::MAIN_SEPARATOR) {
        vec![PathBuf::from(name)]
    } else {
        std::env::split_paths(path).collect()
    };
    Err(NotFound {
        name: name.to_string(),
        dirs,
    })
}

fn apply_host(cmd: &mut std::process::Command, env: &HostEnv) {
    cmd.env("PATH", &env.path);
    if !env.home.as_os_str().is_empty() {
        cmd.env("HOME", &env.home);
    }
}

fn apply_host_tokio(cmd: &mut tokio::process::Command, env: &HostEnv) {
    cmd.env("PATH", &env.path);
    if !env.home.as_os_str().is_empty() {
        cmd.env("HOME", &env.home);
    }
}

fn lock() -> std::sync::MutexGuard<'static, Slot> {
    SLOT.lock().unwrap_or_else(|err| err.into_inner())
}

fn capture_from_process() -> HostEnv {
    let process_path = std::env::var_os("PATH").unwrap_or_default();
    let process_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let captured = shell_program().and_then(|shell| capture_shell(&shell, SHELL_TIMEOUT));
    assemble(captured, &process_home, &process_path)
}

fn shell_program() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        None
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("SHELL")
            .filter(|shell| !shell.is_empty())
            .map(PathBuf::from)
    }
}

fn assemble(
    captured: Option<Vec<(OsString, OsString)>>,
    process_home: &Path,
    process_path: &OsStr,
) -> HostEnv {
    let (mut vars, base_path, home) = match captured {
        Some(vars) => {
            let path = var_of(&vars, "PATH")
                .map(OsString::from)
                .unwrap_or_else(|| process_path.to_os_string());
            let home = var_of(&vars, "HOME")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| process_home.to_path_buf());
            (vars, path, home)
        }
        None => (
            std::env::vars_os().collect(),
            process_path.to_os_string(),
            process_home.to_path_buf(),
        ),
    };
    let path = augment_path(&home, &base_path);
    upsert_var(&mut vars, "PATH", &path);
    if !home.as_os_str().is_empty() {
        upsert_var(&mut vars, "HOME", home.as_os_str());
    }
    HostEnv {
        path,
        home,
        vars,
        read_at: Instant::now(),
    }
}

fn var_of<'a>(vars: &'a [(OsString, OsString)], key: &str) -> Option<&'a OsStr> {
    vars.iter()
        .find(|(name, _)| name.as_os_str() == key)
        .map(|(_, value)| value.as_os_str())
}

fn upsert_var(vars: &mut Vec<(OsString, OsString)>, key: &str, value: &OsStr) {
    if let Some((_, existing)) = vars.iter_mut().find(|(name, _)| name.as_os_str() == key) {
        *existing = value.to_os_string();
    } else {
        vars.push((OsString::from(key), value.to_os_string()));
    }
}

/// `$SHELL -ilc` prints a marker, then `env -0`, then the marker.
/// Timeout or a missing shell returns `None` so the caller uses the fallback.
fn capture_shell(shell: &Path, timeout: Duration) -> Option<Vec<(OsString, OsString)>> {
    let script = format!("printf '%s' '{MARKER}'; env -0; printf '%s' '{MARKER}'");
    let mut cmd = std::process::Command::new(shell);
    cmd.arg("-ilc").arg(script);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // A new session, not only a process group. Interactive zsh puts `sleep` in
    // its own group, and that child keeps the stdout pipe open after the shell dies.
    detach_session(&mut cmd);
    let mut child = cmd.spawn().ok()?;
    // Read while the shell runs. A full pipe otherwise blocks `env -0` until
    // the timeout, which looks like a hung login shell.
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut stdout) = stdout {
            let _ = stdout.read_to_end(&mut bytes);
        }
        bytes
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= timeout => {
                let pid = child.id();
                kill_session(pid);
                let _ = child.kill();
                let _ = wait_briefly(&mut child, Duration::from_millis(500));
                let _ = join_briefly(reader, Duration::from_millis(500));
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                kill_session(child.id());
                let _ = child.kill();
                let _ = wait_briefly(&mut child, Duration::from_millis(500));
                let _ = join_briefly(reader, Duration::from_millis(500));
                return None;
            }
        }
    };
    let bytes = reader.join().unwrap_or_default();
    if !status.is_some_and(|status| status.success()) {
        return None;
    }
    parse_env_blob(&bytes)
}

fn detach_session(cmd: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `setsid` is async-signal-safe and touches no other memory.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
    #[cfg(not(unix))]
    {
        let _ = cmd;
    }
}

fn kill_process_group(pid: u32) -> Result<(), i32> {
    #[cfg(unix)]
    {
        // The login shell's `sleep` is a child. Kill the group, not only the shell.
        let rc = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(rc))
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Ok(())
    }
}

/// Kill `sid` and every process still in that session.
///
/// Interactive shells move children into a new process group. Those children
/// still share the session created by [`detach_session`].
fn kill_session(sid: u32) {
    let _ = kill_process_group(sid);
    #[cfg(unix)]
    {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if pid == sid || session_of(pid) == Some(sid) {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
    }
}

fn session_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    // Fields after comm: state, ppid, pgrp, session.
    stat[end + 1..]
        .split_whitespace()
        .nth(3)
        .and_then(|session| session.parse().ok())
}

fn join_briefly<T: Send + 'static>(
    handle: std::thread::JoinHandle<T>,
    grace: Duration,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(handle.join());
    });
    rx.recv_timeout(grace).ok().and_then(Result::ok)
}

/// After a timeout signal, do not block on a shell that ignored it.
fn wait_briefly(child: &mut std::process::Child, grace: Duration) -> Result<ExitStatus, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() >= grace => {
                return Err(format!("still running after {grace:?}"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(err) => return Err(err.to_string()),
        }
    }
}

fn parse_env_blob(bytes: &[u8]) -> Option<Vec<(OsString, OsString)>> {
    let marker = MARKER.as_bytes();
    let start = find_bytes(bytes, marker)?;
    let from = start + marker.len();
    let end = find_bytes(&bytes[from..], marker)? + from;
    let body = &bytes[from..end];
    let mut vars = Vec::new();
    for entry in body.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq) = entry.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        if eq == 0 {
            continue;
        }
        let key = bytes_to_os(&entry[..eq]);
        let value = bytes_to_os(&entry[eq + 1..]);
        vars.push((key, value));
    }
    if vars.is_empty() {
        None
    } else {
        Some(vars)
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn bytes_to_os(bytes: &[u8]) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(bytes.to_vec())
    }
    #[cfg(not(unix))]
    {
        OsString::from(String::from_utf8_lossy(bytes).as_ref())
    }
}

/// Wait for `cmd` and keep stderr if the process times out.
pub async fn finish(mut cmd: tokio::process::Command, timeout: Duration) -> RunOutput {
    cmd.kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            return RunOutput::Exit(Exit {
                status: None,
                stdout: Vec::new(),
                stderr: err.to_string().into_bytes(),
                stderr_line: Some(err.to_string()),
            });
        }
    };
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stderr_buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let collected = Arc::clone(&stderr_buf);
    let drain = tokio::spawn(async move {
        let Some(mut pipe) = stderr_pipe else {
            return;
        };
        let mut tmp = [0u8; 1024];
        let mut all = Vec::new();
        loop {
            match tokio::io::AsyncReadExt::read(&mut pipe, &mut tmp).await {
                Ok(0) => break,
                Ok(n) => {
                    all.extend_from_slice(&tmp[..n]);
                    if all.len() >= 8 * 1024 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        if let Ok(mut guard) = collected.lock() {
            *guard = all;
        }
    });

    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => {
            let mut stdout = Vec::new();
            if let Some(mut pipe) = stdout_pipe {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    tokio::io::AsyncReadExt::read_to_end(&mut pipe, &mut stdout),
                )
                .await;
            }
            let _ = tokio::time::timeout(Duration::from_millis(500), drain).await;
            let stderr = stderr_buf
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            let stderr_line = first_stderr_line(&stderr);
            RunOutput::Exit(Exit {
                status: Some(status),
                stdout,
                stderr,
                stderr_line,
            })
        }
        Ok(Err(err)) => RunOutput::Exit(Exit {
            status: None,
            stdout: Vec::new(),
            stderr: err.to_string().into_bytes(),
            stderr_line: Some(err.to_string()),
        }),
        Err(_) => {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_millis(500), child.wait()).await;
            let _ = tokio::time::timeout(Duration::from_millis(200), drain).await;
            let stderr = stderr_buf
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            RunOutput::TimedOut {
                stderr_line: first_stderr_line(&stderr),
            }
        }
    }
}

/// Synchronous git-style wait used by callers that are not on the async runtime.
pub fn wait_blocking(
    mut cmd: std::process::Command,
    timeout: Duration,
) -> Result<std::process::Output, std::io::Error> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    detach_session(&mut cmd);
    let mut child = cmd.spawn()?;
    let started = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_end(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_end(&mut stderr);
                }
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            None if started.elapsed() >= timeout => {
                kill_session(child.id());
                let _ = child.kill();
                let _ = wait_briefly(&mut child, Duration::from_millis(500));
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("command timed out after {}s", timeout.as_secs()),
                ));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(path: &Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn parses_env_between_markers_and_ignores_a_banner() {
        let raw = b"welcome\n__COPPICE_ENV__PATH=/bin\0HOME=/tmp\0FOO=a=b\0__COPPICE_ENV__trailing";
        let vars = parse_env_blob(raw).unwrap();
        assert_eq!(var_of(&vars, "PATH").unwrap(), "/bin");
        assert_eq!(var_of(&vars, "HOME").unwrap(), "/tmp");
        assert_eq!(var_of(&vars, "FOO").unwrap(), "a=b");
    }

    #[test]
    fn missing_markers_are_not_an_environment() {
        assert!(parse_env_blob(b"no markers here").is_none());
    }

    #[test]
    fn resolve_reports_the_directories_it_searched() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        script(&bin.join("tool"), "exit 0");
        let path = OsString::from(bin.as_os_str());
        let env = HostEnv {
            path: path.clone(),
            home: dir.path().to_path_buf(),
            vars: Vec::new(),
            read_at: Instant::now(),
        };
        assert_eq!(env.resolve("tool").unwrap(), bin.join("tool"));
        let missing = env.resolve("absent").unwrap_err();
        assert_eq!(missing.name, "absent");
        assert_eq!(missing.dirs, vec![bin.clone()]);
        assert_eq!(
            missing.to_string(),
            format!("`absent` wasn't found. Looked in: {}", bin.display())
        );
        assert_eq!(
            resolve_model_program("absent", &path),
            PathBuf::from("absent")
        );
        assert_eq!(resolve_model_program("tool", &path), bin.join("tool"));
    }

    #[test]
    fn no_shell_uses_the_fallback_only() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".cargo/bin")).unwrap();
        let env = assemble(None, home.path(), OsStr::new("/usr/bin:/bin"));
        let dirs: Vec<PathBuf> = std::env::split_paths(&env.path).collect();
        assert!(dirs.iter().any(|dir| dir.ends_with(".cargo/bin")));
        assert!(dirs
            .iter()
            .any(|dir| dir.ends_with("usr/bin") || dir.as_os_str() == "/usr/bin"));
    }

    #[test]
    fn shell_capture_merges_fallback_dirs_into_the_printed_path() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".cargo/bin")).unwrap();
        let shell = home.path().join("shell");
        let home_literal = home.path().display().to_string().replace('\'', "'\\''");
        script(
            &shell,
            &format!(
                r#"if [ "$1" = "-ilc" ]; then
  export PATH="/from/shell:${{PATH}}"
  export HOME='{home_literal}'
  eval "$2"
fi"#
            ),
        );
        let captured = capture_shell(&shell, Duration::from_secs(5));
        let vars = captured.expect("shell env");
        assert_eq!(
            var_of(&vars, "PATH")
                .unwrap()
                .to_string_lossy()
                .split(':')
                .next(),
            Some("/from/shell")
        );
        let env = assemble(Some(vars), home.path(), OsStr::new("/usr/bin"));
        let dirs: Vec<PathBuf> = std::env::split_paths(&env.path).collect();
        let shell_at = dirs.iter().position(|dir| dir.as_os_str() == "/from/shell");
        let cargo_at = dirs.iter().position(|dir| dir.ends_with(".cargo/bin"));
        assert!(shell_at.is_some(), "shell PATH entry missing: {dirs:?}");
        assert!(
            cargo_at.is_some_and(|cargo| cargo < shell_at.unwrap()),
            "fallback dirs are merged ahead of the shell PATH: {dirs:?}"
        );
    }

    #[test]
    fn a_shell_that_sleeps_hits_the_timeout_and_returns_none() {
        let home = tempfile::tempdir().unwrap();
        let shell = home.path().join("slow");
        script(&shell, "sleep 30\n");
        let started = Instant::now();
        let captured = capture_shell(&shell, Duration::from_millis(200));
        assert!(captured.is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
