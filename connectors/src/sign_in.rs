//! Cheap sign-in checks. They must not open a browser, prompt, or call a model.
//!
//! A connector with no reliable check never becomes "not signed in": it stays
//! [`Readiness::Found`] until something actually verifies a login.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::probe::{auth_path_present, ProbeEnv};
use crate::{ConnectorDescriptor, CLAUDE_CODE, CODEX, CURSOR, KILO_CODE, OPENCODE};

const OUTPUT_CAP: u64 = 8 * 1024;
const POLL: Duration = Duration::from_millis(50);

/// What the agent form and Connectors row show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    /// The connector binary is not on PATH.
    NotOnPath,
    /// Binary found and sign-in was verified.
    Ready,
    /// Binary found, and a reliable check says there is no sign-in.
    FoundNotSignedIn,
    /// Binary found, but sign-in could not be verified. Do not claim the user
    /// is signed out.
    Found,
}

pub fn readiness_from(binary_found: bool, signed_in: Option<bool>) -> Readiness {
    if !binary_found {
        return Readiness::NotOnPath;
    }
    match signed_in {
        Some(true) => Readiness::Ready,
        Some(false) => Readiness::FoundNotSignedIn,
        None => Readiness::Found,
    }
}

/// `Some(true)` verified, `Some(false)` reliably signed out, `None` unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandObservation<'a> {
    pub timed_out: bool,
    pub spawn_failed: bool,
    pub exit_ok: bool,
    pub stdout: &'a str,
    pub stderr: &'a str,
}

/// Map a connector id and local observations to evidence, without running anything.
pub fn evidence_from(
    id: &str,
    env_hit: bool,
    file_hit: bool,
    command: Option<CommandObservation<'_>>,
) -> Option<bool> {
    if id == KILO_CODE || id == crate::MOCK {
        return None;
    }
    if env_hit || file_hit {
        return Some(true);
    }
    if id == CURSOR {
        // Auth lives in those json files. Missing both means not signed in.
        return Some(false);
    }
    let command = command?;
    if command.timed_out || command.spawn_failed {
        return None;
    }
    if id == CLAUDE_CODE {
        return interpret_claude(command);
    }
    if id == CODEX {
        return interpret_codex(command);
    }
    if id == OPENCODE {
        return interpret_opencode(command);
    }
    None
}

fn interpret_claude(command: CommandObservation<'_>) -> Option<bool> {
    let text = format!("{}\n{}", command.stdout, command.stderr);
    let compact = text.replace([' ', '\n', '\t'], "").to_ascii_lowercase();
    if compact.contains("\"loggedin\":true") {
        return Some(true);
    }
    if compact.contains("\"loggedin\":false") {
        return Some(false);
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("unknown command") || lower.contains("unrecognized") {
        return None;
    }
    if lower.contains("not logged in") || lower.contains("not signed in") {
        return Some(false);
    }
    if lower.contains("logged in") {
        return Some(true);
    }
    None
}

fn interpret_codex(command: CommandObservation<'_>) -> Option<bool> {
    let text = format!("{}\n{}", command.stdout, command.stderr).to_ascii_lowercase();
    if text.contains("unknown command") || text.contains("unrecognized") {
        return None;
    }
    Some(command.exit_ok)
}

fn interpret_opencode(command: CommandObservation<'_>) -> Option<bool> {
    let text = format!("{}\n{}", command.stdout, command.stderr);
    let lower = text.to_ascii_lowercase();
    if lower.contains("unknown command") || lower.contains("unrecognized") {
        return None;
    }
    if auth_list_has_provider(&text) {
        return Some(true);
    }
    if command.exit_ok {
        return Some(false);
    }
    None
}

fn auth_list_has_provider(stdout: &str) -> bool {
    stdout.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        if line.chars().all(|c| {
            c.is_whitespace()
                || matches!(
                    c,
                    '┌' | '┐'
                        | '└'
                        | '┘'
                        | '│'
                        | '─'
                        | '┬'
                        | '┴'
                        | '┼'
                        | '╭'
                        | '╮'
                        | '╯'
                        | '╰'
                        | '├'
                        | '┤'
                        | '+'
                        | '-'
                        | '|'
                        | '='
                )
        }) {
            return false;
        }
        let header = line.to_ascii_lowercase();
        if matches!(
            header.as_str(),
            "provider" | "credentials" | "type" | "status" | "provider status"
        ) {
            return false;
        }
        line.chars().any(|c| c.is_ascii_alphanumeric())
    })
}

struct Spec {
    env: &'static [&'static str],
    files: &'static [&'static str],
    command: Option<&'static [&'static str]>,
}

fn spec_for(id: &str) -> Spec {
    if id == CURSOR {
        Spec {
            env: &[],
            files: &[".config/cursor/auth.json", ".cursor/auth.json"],
            command: None,
        }
    } else if id == CLAUDE_CODE {
        Spec {
            env: &["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"],
            files: &[".claude/.credentials.json"],
            command: Some(&["auth", "status"]),
        }
    } else if id == CODEX {
        Spec {
            env: &["OPENAI_API_KEY"],
            files: &[".codex/auth.json"],
            command: Some(&["login", "status"]),
        }
    } else if id == OPENCODE {
        Spec {
            env: &[],
            files: &[".local/share/opencode/auth.json"],
            command: Some(&["auth", "list"]),
        }
    } else {
        Spec {
            env: &[],
            files: &[],
            command: None,
        }
    }
}

/// Resolve sign-in for a connector whose binary was already looked up.
/// `binary == None` is [`Readiness::NotOnPath`] and runs nothing.
pub fn assess(
    descriptor: &ConnectorDescriptor,
    env: &ProbeEnv<'_>,
    binary: Option<&Path>,
    timeout: Duration,
) -> Readiness {
    let Some(binary) = binary else {
        return Readiness::NotOnPath;
    };
    let spec = spec_for(descriptor.id);
    let env_hit = spec
        .env
        .iter()
        .any(|key| (env.env_lookup)(key).is_some_and(|value| !value.is_empty()));
    let file_hit = spec
        .files
        .iter()
        .any(|rel| auth_path_present(&env.home.join(rel)));
    if evidence_from(descriptor.id, env_hit, file_hit, None) == Some(true) {
        return Readiness::Ready;
    }
    // Cursor has no command: missing files is the negative result.
    if spec.command.is_none() {
        return readiness_from(true, evidence_from(descriptor.id, env_hit, file_hit, None));
    }
    let Some(args) = spec.command else {
        return Readiness::Found;
    };
    let observed = run_command(binary, args, env, timeout);
    readiness_from(
        true,
        evidence_from(
            descriptor.id,
            env_hit,
            file_hit,
            Some(CommandObservation {
                timed_out: observed.timed_out,
                spawn_failed: observed.spawn_failed,
                exit_ok: observed.exit_ok,
                stdout: &observed.stdout,
                stderr: &observed.stderr,
            }),
        ),
    )
}

struct Ran {
    timed_out: bool,
    spawn_failed: bool,
    exit_ok: bool,
    stdout: String,
    stderr: String,
}

fn run_command(bin: &Path, args: &[&str], env: &ProbeEnv<'_>, timeout: Duration) -> Ran {
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
        Err(_) => {
            return Ran {
                timed_out: false,
                spawn_failed: true,
                exit_ok: false,
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let finished = wait_with_timeout(&mut child, timeout);
    let Some(status) = finished else {
        let _ = child.kill();
        let _ = child.wait();
        return Ran {
            timed_out: true,
            spawn_failed: false,
            exit_ok: false,
            stdout: String::new(),
            stderr: String::new(),
        };
    };
    let grace = Instant::now() + Duration::from_millis(200);
    Ran {
        timed_out: false,
        spawn_failed: false,
        exit_ok: status.success(),
        stdout: collect(&stdout, grace),
        stderr: collect(&stderr, grace),
    }
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Option<std::process::ExitStatus> {
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
        std::thread::sleep(POLL.min(deadline - now));
    }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(mut pipe) = pipe {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.by_ref().take(OUTPUT_CAP).read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    }
    rx
}

fn collect(rx: &std::sync::mpsc::Receiver<Vec<u8>>, deadline: Instant) -> String {
    let wait = deadline.saturating_duration_since(Instant::now());
    rx.recv_timeout(wait)
        .map(|buf| String::from_utf8_lossy(&buf).into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::ProbeEnv;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    fn obs<'a>(exit_ok: bool, stdout: &'a str, stderr: &'a str) -> CommandObservation<'a> {
        CommandObservation {
            timed_out: false,
            spawn_failed: false,
            exit_ok,
            stdout,
            stderr,
        }
    }

    #[test]
    fn readiness_mapping() {
        assert_eq!(readiness_from(false, Some(true)), Readiness::NotOnPath);
        assert_eq!(readiness_from(false, None), Readiness::NotOnPath);
        assert_eq!(readiness_from(true, Some(true)), Readiness::Ready);
        assert_eq!(
            readiness_from(true, Some(false)),
            Readiness::FoundNotSignedIn
        );
        assert_eq!(readiness_from(true, None), Readiness::Found);
    }

    #[test]
    fn claude_json_and_phrases() {
        assert_eq!(
            evidence_from(
                CLAUDE_CODE,
                false,
                false,
                Some(obs(true, r#"{"loggedIn": true}"#, ""))
            ),
            Some(true)
        );
        assert_eq!(
            evidence_from(
                CLAUDE_CODE,
                false,
                false,
                Some(obs(true, r#"{"loggedIn":false}"#, ""))
            ),
            Some(false)
        );
        assert_eq!(
            evidence_from(
                CLAUDE_CODE,
                false,
                false,
                Some(obs(true, "", "Not logged in"))
            ),
            Some(false)
        );
        assert_eq!(
            evidence_from(
                CLAUDE_CODE,
                false,
                false,
                Some(obs(false, "unknown command", ""))
            ),
            None
        );
        assert_eq!(
            evidence_from(
                CLAUDE_CODE,
                false,
                false,
                Some(CommandObservation {
                    timed_out: true,
                    spawn_failed: false,
                    exit_ok: false,
                    stdout: "",
                    stderr: "",
                })
            ),
            None
        );
        assert_eq!(evidence_from(CLAUDE_CODE, true, false, None), Some(true));
    }

    #[test]
    fn codex_exit_status_and_unknown_subcommand() {
        assert_eq!(
            evidence_from(CODEX, false, false, Some(obs(true, "Logged in", ""))),
            Some(true)
        );
        assert_eq!(
            evidence_from(CODEX, false, false, Some(obs(false, "", "Not logged in"))),
            Some(false)
        );
        assert_eq!(
            evidence_from(
                CODEX,
                false,
                false,
                Some(obs(false, "", "unknown command: login"))
            ),
            None
        );
        assert_eq!(evidence_from(CODEX, false, true, None), Some(true));
    }

    #[test]
    fn cursor_files_are_the_whole_check() {
        assert_eq!(evidence_from(CURSOR, false, true, None), Some(true));
        assert_eq!(evidence_from(CURSOR, false, false, None), Some(false));
    }

    #[test]
    fn opencode_empty_list_is_signed_out_and_a_provider_is_ready() {
        assert_eq!(
            evidence_from(OPENCODE, false, false, Some(obs(true, "", ""))),
            Some(false)
        );
        assert_eq!(
            evidence_from(
                OPENCODE,
                false,
                false,
                Some(obs(true, "┌──────────┐\n│ anthropic │\n└──────────┘\n", ""))
            ),
            Some(true)
        );
        assert_eq!(
            evidence_from(OPENCODE, false, false, Some(obs(true, "Provider\n", ""))),
            Some(false)
        );
        assert_eq!(
            evidence_from(
                OPENCODE,
                false,
                false,
                Some(obs(false, "unknown command", ""))
            ),
            None
        );
    }

    #[test]
    fn kilo_never_claims_signed_out() {
        assert_eq!(
            evidence_from(KILO_CODE, true, true, Some(obs(false, "", ""))),
            None
        );
        assert_eq!(evidence_from(KILO_CODE, false, false, None), None);
    }

    fn env<'a>(home: &'a Path, lookup: &'a dyn Fn(&str) -> Option<String>) -> ProbeEnv<'a> {
        ProbeEnv {
            home,
            path: OsStr::new(""),
            command_override: None,
            env_lookup: lookup,
        }
    }

    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("cli");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[test]
    fn cursor_missing_file_is_not_signed_in_and_does_not_run_a_command() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        let marker = bin_dir.path().join("ran");
        let bin = script(
            bin_dir.path(),
            &format!("touch '{}'\nexit 0", marker.display()),
        );
        let none = |_: &str| None;
        let readiness = assess(
            crate::get(CURSOR).unwrap(),
            &env(home.path(), &none),
            Some(&bin),
            Duration::from_secs(2),
        );
        assert_eq!(readiness, Readiness::FoundNotSignedIn);
        assert!(!marker.exists(), "cursor sign-in must not spawn the CLI");
    }

    #[test]
    fn cursor_auth_json_is_ready() {
        let home = tempfile::tempdir().unwrap();
        let auth = home.path().join(".config/cursor/auth.json");
        std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
        std::fs::write(&auth, "{ \"accessToken\": \"x\" }\n").unwrap();
        let none = |_: &str| None;
        let readiness = assess(
            crate::get(CURSOR).unwrap(),
            &env(home.path(), &none),
            Some(Path::new("/usr/bin/true")),
            Duration::from_millis(200),
        );
        assert_eq!(readiness, Readiness::Ready);
    }

    #[test]
    fn claude_api_key_is_ready_without_running_the_cli() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        let marker = bin_dir.path().join("ran");
        let bin = script(
            bin_dir.path(),
            &format!("touch '{}'\nexit 1", marker.display()),
        );
        let lookup = |key: &str| (key == "ANTHROPIC_API_KEY").then(|| "secret".to_string());
        let readiness = assess(
            crate::get(CLAUDE_CODE).unwrap(),
            &env(home.path(), &lookup),
            Some(&bin),
            Duration::from_secs(2),
        );
        assert_eq!(readiness, Readiness::Ready);
        assert!(!marker.exists());
    }

    #[test]
    fn codex_not_logged_in_script() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        let bin = script(bin_dir.path(), "echo 'Not logged in' >&2\nexit 1");
        let none = |_: &str| None;
        let readiness = assess(
            crate::get(CODEX).unwrap(),
            &env(home.path(), &none),
            Some(&bin),
            Duration::from_secs(2),
        );
        assert_eq!(readiness, Readiness::FoundNotSignedIn);
    }

    #[test]
    fn hung_auth_command_does_not_claim_signed_out() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        let bin = script(bin_dir.path(), "/bin/sleep 30");
        let none = |_: &str| None;
        let readiness = assess(
            crate::get(CODEX).unwrap(),
            &env(home.path(), &none),
            Some(&bin),
            Duration::from_millis(200),
        );
        assert_eq!(readiness, Readiness::Found);
    }

    #[test]
    fn kilo_binary_is_found_without_a_sign_in_claim() {
        let home = tempfile::tempdir().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        let marker = bin_dir.path().join("ran");
        let bin = script(
            bin_dir.path(),
            &format!("touch '{}'\nexit 1", marker.display()),
        );
        let none = |_: &str| None;
        let readiness = assess(
            crate::get(KILO_CODE).unwrap(),
            &env(home.path(), &none),
            Some(&bin),
            Duration::from_secs(2),
        );
        assert_eq!(readiness, Readiness::Found);
        assert!(!marker.exists(), "kilo has no sign-in command");
    }

    #[test]
    fn missing_binary_is_not_on_path() {
        let home = tempfile::tempdir().unwrap();
        let none = |_: &str| None;
        let readiness = assess(
            crate::get(CLAUDE_CODE).unwrap(),
            &env(home.path(), &none),
            None,
            Duration::from_millis(50),
        );
        assert_eq!(readiness, Readiness::NotOnPath);
    }
}
