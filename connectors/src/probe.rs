//! Local connector probes: binary lookup, auth hints, and the descriptor's probe command.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::ConnectorDescriptor;

/// Bin dirs under `$HOME` that desktop launchers often leave off PATH.
pub const COMMON_BIN_DIRS_HOME: &[&str] =
    &[".local/bin", ".opencode/bin", ".npm-global/bin", ".bun/bin"];
pub const COMMON_BIN_DIRS_ABS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

const OK_LINE_MAX: usize = 200;
const FAILED_MESSAGE_MAX: usize = 500;
const OUTPUT_READ_MAX: u64 = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Grandchildren may keep the pipes open after the child exits.
const PIPE_DRAIN_GRACE: Duration = Duration::from_millis(500);

#[cfg(windows)]
const PATH_SEPARATOR: &str = ";";
#[cfg(not(windows))]
const PATH_SEPARATOR: &str = ":";

pub struct ProbeEnv<'a> {
    pub home: &'a Path,
    /// PATH used for lookup and for the probe child.
    pub path: &'a OsStr,
    /// Config `command`, used instead of the descriptor binary when set.
    pub command_override: Option<&'a str>,
    /// Reads auth env vars; values are only used for presence and redaction.
    pub env_lookup: &'a dyn Fn(&str) -> Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    NotRun,
    Ok { first_line: String },
    Failed { message: String },
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReport {
    pub binary: Option<PathBuf>,
    pub auth_env_set: Vec<&'static str>,
    pub auth_paths_found: Vec<&'static str>,
    pub probe: ProbeOutcome,
    pub probe_proves_auth: bool,
}

impl ProbeReport {
    pub fn auth_ok(&self) -> bool {
        !self.auth_env_set.is_empty()
            || !self.auth_paths_found.is_empty()
            || (self.probe_proves_auth && matches!(self.probe, ProbeOutcome::Ok { .. }))
    }
}

/// Prepends each existing common bin dir not already on `current`, in listed order.
pub fn augment_path(home: &Path, current: &OsStr) -> OsString {
    let existing: Vec<PathBuf> = std::env::split_paths(current).collect();
    let mut added: Vec<PathBuf> = Vec::new();
    let candidates = COMMON_BIN_DIRS_HOME
        .iter()
        .map(|rel| home.join(rel))
        .chain(COMMON_BIN_DIRS_ABS.iter().map(PathBuf::from));
    for dir in candidates {
        if dir.is_dir() && !existing.contains(&dir) && !added.contains(&dir) {
            added.push(dir);
        }
    }
    if added.is_empty() {
        return current.to_os_string();
    }
    let Ok(mut out) = std::env::join_paths(&added) else {
        return current.to_os_string();
    };
    if !current.is_empty() {
        out.push(PATH_SEPARATOR);
        out.push(current);
    }
    out
}

/// Names containing `/` are checked as given; bare names are searched in `path`.
pub fn resolve_binary(name: &str, path: &OsStr) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains('/') || name.contains(std::path::MAIN_SEPARATOR) {
        let p = PathBuf::from(name);
        return is_executable(&p).then_some(p);
    }
    std::env::split_paths(path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// A non-empty file or a non-empty directory.
pub fn auth_path_present(path: &Path) -> bool {
    if path.is_file() {
        return std::fs::metadata(path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
    }
    if path.is_dir() {
        return std::fs::read_dir(path)
            .ok()
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
    }
    false
}

pub fn probe(d: &ConnectorDescriptor, env: &ProbeEnv, timeout: Duration) -> ProbeReport {
    let mut secrets: Vec<String> = Vec::new();
    let mut auth_env_set = Vec::new();
    for &key in d.install.auth_env {
        if let Some(value) = (env.env_lookup)(key).filter(|v| !v.is_empty()) {
            auth_env_set.push(key);
            secrets.push(value);
        }
    }
    let auth_paths_found = d
        .install
        .auth_paths
        .iter()
        .copied()
        .filter(|rel| auth_path_present(&env.home.join(rel)))
        .collect();

    let name = env.command_override.unwrap_or(d.binary);
    let binary = resolve_binary(name, env.path);
    let probe = match &binary {
        Some(bin) if !d.install.probe_args.is_empty() => {
            run_probe_command(bin, d.install.probe_args, env, timeout, &secrets)
        }
        _ => ProbeOutcome::NotRun,
    };

    ProbeReport {
        binary,
        auth_env_set,
        auth_paths_found,
        probe,
        probe_proves_auth: d.install.probe_proves_auth,
    }
}

fn run_probe_command(
    bin: &Path,
    args: &[&str],
    env: &ProbeEnv,
    timeout: Duration,
    secrets: &[String],
) -> ProbeOutcome {
    let spawned = Command::new(bin)
        .args(args)
        .env("PATH", env.path)
        .env("HOME", env.home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            return ProbeOutcome::Failed {
                message: finish(&e.to_string(), secrets, FAILED_MESSAGE_MAX),
            }
        }
    };
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());

    let Some(status) = wait_with_timeout(&mut child, timeout) else {
        let _ = child.kill();
        let _ = child.wait();
        return ProbeOutcome::TimedOut;
    };

    let grace = Instant::now() + PIPE_DRAIN_GRACE;
    let stdout = collect(&stdout, grace);
    let stderr = collect(&stderr, grace);

    if status.success() {
        let text = if stdout.trim().is_empty() {
            &stderr
        } else {
            &stdout
        };
        let redacted = redact(text, secrets);
        let line = redacted
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("");
        ProbeOutcome::Ok {
            first_line: cap(line, OK_LINE_MAX),
        }
    } else {
        let message = [stderr.trim(), stdout.trim()]
            .into_iter()
            .find(|t| !t.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "command failed".to_string());
        ProbeOutcome::Failed {
            message: finish(&message, secrets, FAILED_MESSAGE_MAX),
        }
    }
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {}
            Err(_) => return None,
        }
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        std::thread::sleep(POLL_INTERVAL.min(deadline - now));
    }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    if let Some(pipe) = pipe {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.take(OUTPUT_READ_MAX).read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    }
    rx
}

fn collect(rx: &mpsc::Receiver<Vec<u8>>, deadline: Instant) -> String {
    let wait = deadline.saturating_duration_since(Instant::now());
    rx.recv_timeout(wait)
        .map(|buf| String::from_utf8_lossy(&buf).into_owned())
        .unwrap_or_default()
}

fn redact(text: &str, secrets: &[String]) -> String {
    secrets
        .iter()
        .filter(|s| !s.is_empty())
        .fold(text.to_string(), |acc, s| {
            acc.replace(s.as_str(), "[redacted]")
        })
}

fn finish(text: &str, secrets: &[String], max: usize) -> String {
    cap(&redact(text, secrets), max)
}

fn cap(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{all, get, CLAUDE_CODE, CURSOR, MOCK, OPENCODE};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::RwLock;
    use std::time::Instant;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "coppice-probe-{tag}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Writers hold this exclusively so no parallel test forks while a script's
    /// write fd is open (exec would fail with ETXTBSY).
    static SPAWN_LOCK: RwLock<()> = RwLock::new(());

    fn write_script(path: &Path, body: &str) {
        let _guard = SPAWN_LOCK.write().unwrap_or_else(|e| e.into_inner());
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn run_probe(
        d: &ConnectorDescriptor,
        home: &Path,
        bin: &Path,
        lookup: &dyn Fn(&str) -> Option<String>,
        timeout: Duration,
    ) -> ProbeReport {
        run_probe_with_path(d, home, bin.as_os_str(), None, lookup, timeout)
    }

    fn run_probe_with_path(
        d: &ConnectorDescriptor,
        home: &Path,
        path: &OsStr,
        command_override: Option<&str>,
        lookup: &dyn Fn(&str) -> Option<String>,
        timeout: Duration,
    ) -> ProbeReport {
        let env = ProbeEnv {
            home,
            path,
            command_override,
            env_lookup: lookup,
        };
        let _guard = SPAWN_LOCK.read().unwrap_or_else(|e| e.into_inner());
        probe(d, &env, timeout)
    }

    #[test]
    fn resolve_binary_finds_executable_on_path() {
        let tmp = TempDir::new("resolve");
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        write_script(&b.join("tool"), "exit 0");
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(resolve_binary("tool", &path), Some(b.join("tool")));
        assert_eq!(resolve_binary("missing", &path), None);
    }

    #[test]
    fn resolve_binary_rejects_directory_and_non_executable() {
        let tmp = TempDir::new("reject");
        fs::create_dir_all(tmp.path().join("dirtool")).unwrap();
        fs::write(tmp.path().join("plain"), "data").unwrap();
        let path = tmp.path().as_os_str();
        assert_eq!(resolve_binary("dirtool", path), None);
        #[cfg(unix)]
        assert_eq!(resolve_binary("plain", path), None);
    }

    #[test]
    fn resolve_binary_uses_path_with_slash_as_given() {
        let tmp = TempDir::new("slash");
        let tool = tmp.path().join("tool");
        write_script(&tool, "exit 0");
        let as_given = tool.to_str().unwrap();
        assert_eq!(resolve_binary(as_given, OsStr::new("")), Some(tool.clone()));
        let missing = tmp.path().join("nope");
        assert_eq!(
            resolve_binary(missing.to_str().unwrap(), tmp.path().as_os_str()),
            None
        );
    }

    #[test]
    fn augment_path_adds_existing_dirs_once() {
        let home = TempDir::new("augment");
        fs::create_dir_all(home.path().join(".local/bin")).unwrap();
        let once = augment_path(home.path(), OsStr::new("/usr/bin"));
        let dirs: Vec<PathBuf> = std::env::split_paths(&once).collect();
        assert_eq!(dirs[0], home.path().join(".local/bin"));
        assert!(!dirs.iter().any(|d| d.ends_with(".bun/bin")));
        assert!(dirs.contains(&PathBuf::from("/usr/bin")));
        let twice = augment_path(home.path(), &once);
        assert_eq!(twice, once);
    }

    #[test]
    fn probe_ok_reports_first_line() {
        let tmp = TempDir::new("ok");
        write_script(
            &tmp.path().join("claude"),
            "printf 'claude 2.1.0\\nmore\\n'",
        );
        let d = get(CLAUDE_CODE).unwrap();
        let r = run_probe(d, tmp.path(), tmp.path(), &no_env, Duration::from_secs(10));
        assert_eq!(r.binary, Some(tmp.path().join("claude")));
        assert_eq!(
            r.probe,
            ProbeOutcome::Ok {
                first_line: "claude 2.1.0".into()
            }
        );
    }

    #[test]
    fn probe_failed_reports_stderr_capped() {
        let tmp = TempDir::new("failed");
        write_script(
            &tmp.path().join("claude"),
            "i=0; while [ $i -lt 200 ]; do printf 'xxxxxxxxxx' >&2; i=$((i+1)); done; exit 1",
        );
        let d = get(CLAUDE_CODE).unwrap();
        let r = run_probe(d, tmp.path(), tmp.path(), &no_env, Duration::from_secs(10));
        match r.probe {
            ProbeOutcome::Failed { message } => {
                assert!(message.chars().count() <= 500, "len {}", message.len());
                assert!(message.starts_with("xxxx"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn probe_failed_without_output_says_command_failed() {
        let tmp = TempDir::new("silent");
        write_script(&tmp.path().join("claude"), "exit 3");
        let d = get(CLAUDE_CODE).unwrap();
        let r = run_probe(d, tmp.path(), tmp.path(), &no_env, Duration::from_secs(10));
        assert_eq!(
            r.probe,
            ProbeOutcome::Failed {
                message: "command failed".into()
            }
        );
    }

    #[test]
    fn probe_times_out() {
        let tmp = TempDir::new("timeout");
        write_script(&tmp.path().join("claude"), "exec sleep 30");
        let mut path = tmp.path().as_os_str().to_os_string();
        path.push(":/usr/bin:/bin");
        let d = get(CLAUDE_CODE).unwrap();
        let start = Instant::now();
        let r = run_probe_with_path(
            d,
            tmp.path(),
            &path,
            None,
            &no_env,
            Duration::from_millis(300),
        );
        assert_eq!(r.probe, ProbeOutcome::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn probe_missing_binary_not_run() {
        let tmp = TempDir::new("missing");
        let d = get(CLAUDE_CODE).unwrap();
        let r = run_probe(d, tmp.path(), tmp.path(), &no_env, Duration::from_secs(10));
        assert_eq!(r.binary, None);
        assert_eq!(r.probe, ProbeOutcome::NotRun);
    }

    #[test]
    fn probe_redacts_auth_env_values() {
        let tmp = TempDir::new("redact");
        write_script(&tmp.path().join("claude"), "echo \"key=sk-secret-123456\"");
        let lookup = |k: &str| (k == "ANTHROPIC_API_KEY").then(|| "sk-secret-123456".to_string());
        let d = get(CLAUDE_CODE).unwrap();
        let r = run_probe(d, tmp.path(), tmp.path(), &lookup, Duration::from_secs(10));
        match r.probe {
            ProbeOutcome::Ok { first_line } => {
                assert!(first_line.contains("[redacted]"), "{first_line}");
                assert!(!first_line.contains("sk-secret-123456"));
            }
            other => panic!("expected Ok, got {other:?}"),
        }
        assert_eq!(r.auth_env_set, vec!["ANTHROPIC_API_KEY"]);
    }

    #[test]
    fn auth_detects_env_names_and_paths() {
        let home = TempDir::new("auth");
        let cursor = get(CURSOR).unwrap();
        fs::create_dir_all(home.path().join(".config/cursor")).unwrap();
        fs::write(home.path().join(".config/cursor/auth.json"), "{}").unwrap();
        fs::create_dir_all(home.path().join(".cursor")).unwrap();
        fs::write(home.path().join(".cursor/auth.json"), "").unwrap();
        let r = run_probe(
            cursor,
            home.path(),
            home.path(),
            &no_env,
            Duration::from_secs(10),
        );
        assert_eq!(r.auth_paths_found, vec![".config/cursor/auth.json"]);
        assert!(r.auth_env_set.is_empty());
        assert!(r.probe_proves_auth);
        assert!(r.auth_ok());

        let claude = get(CLAUDE_CODE).unwrap();
        let lookup = |k: &str| (k == "ANTHROPIC_API_KEY").then(|| "x".to_string());
        let r = run_probe(
            claude,
            home.path(),
            home.path(),
            &lookup,
            Duration::from_secs(10),
        );
        assert_eq!(r.auth_env_set, vec!["ANTHROPIC_API_KEY"]);
        assert!(r.auth_paths_found.is_empty());
        assert!(r.auth_ok());

        let empty = |k: &str| (k == "ANTHROPIC_API_KEY").then(String::new);
        let r = run_probe(
            claude,
            home.path(),
            home.path(),
            &empty,
            Duration::from_secs(10),
        );
        assert!(r.auth_env_set.is_empty());
        assert!(!r.auth_ok());
    }

    #[test]
    fn auth_ok_via_probe_only_when_it_proves_auth() {
        let tmp = TempDir::new("proves");
        write_script(&tmp.path().join("opencode"), "echo providers");
        write_script(&tmp.path().join("claude"), "echo 1.0");
        let r = run_probe(
            get(OPENCODE).unwrap(),
            tmp.path(),
            tmp.path(),
            &no_env,
            Duration::from_secs(10),
        );
        assert!(r.auth_ok());
        let r = run_probe(
            get(CLAUDE_CODE).unwrap(),
            tmp.path(),
            tmp.path(),
            &no_env,
            Duration::from_secs(10),
        );
        assert!(!r.auth_ok());
    }

    #[test]
    fn command_override_replaces_descriptor_binary() {
        let tmp = TempDir::new("override");
        let custom = tmp.path().join("my-agent");
        write_script(&custom, "echo custom");
        let r = run_probe_with_path(
            get(CURSOR).unwrap(),
            tmp.path(),
            OsStr::new(""),
            Some(custom.to_str().unwrap()),
            &no_env,
            Duration::from_secs(10),
        );
        assert_eq!(r.binary, Some(custom));
        assert_eq!(
            r.probe,
            ProbeOutcome::Ok {
                first_line: "custom".into()
            }
        );
    }

    #[test]
    fn auth_path_present_matches_cli_behavior() {
        let tmp = TempDir::new("authpath");
        let file = tmp.path().join("f");
        assert!(!auth_path_present(&file));
        fs::write(&file, "").unwrap();
        assert!(!auth_path_present(&file));
        fs::write(&file, "x").unwrap();
        assert!(auth_path_present(&file));
        let dir = tmp.path().join("d");
        fs::create_dir_all(&dir).unwrap();
        assert!(!auth_path_present(&dir));
        fs::write(dir.join("x"), "").unwrap();
        assert!(auth_path_present(&dir));
    }

    #[test]
    fn descriptors_have_probe_args() {
        for d in all().iter().filter(|d| d.id != MOCK) {
            assert!(!d.install.probe_args.is_empty(), "{} probe_args", d.id);
            assert!(
                d.install.docs_url.starts_with("https://"),
                "{} docs_url",
                d.id
            );
        }
        let mock = get(MOCK).unwrap();
        assert!(mock.install.probe_args.is_empty());
        assert_eq!(mock.install.docs_url, "");
        let proves: Vec<&str> = all()
            .iter()
            .filter(|d| d.install.probe_proves_auth)
            .map(|d| d.id)
            .collect();
        assert_eq!(proves, vec![CURSOR, OPENCODE]);
        assert_eq!(get(CURSOR).unwrap().install.probe_args, ["models"]);
        assert_eq!(get(OPENCODE).unwrap().install.probe_args, ["auth", "list"]);
        for id in [CLAUDE_CODE, crate::CODEX, crate::KILO_CODE] {
            assert_eq!(get(id).unwrap().install.probe_args, ["--version"]);
        }
    }
}
