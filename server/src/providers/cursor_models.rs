use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;

use crate::providers::cli_catalog::{self, MODEL_LIST_TIMEOUT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

/// Argument sets for the Cursor agent CLI. Both print the same human list.
const MODEL_ARG_SETS: &[&[&str]] = &[&["models"], &["--list-models"]];

/// Parse `agent models` / `agent --list-models` text.
///
/// The terminal format is a header, a blank line, then `id - Display Name`
/// lines. Markers such as `(current, default)` are not part of the name.
/// A line that is only an id is kept, with that id as the name. Internal
/// spaces in a display name are preserved.
pub fn parse_cursor_models_stdout(stdout: &str) -> Vec<ModelOption> {
    let cleaned = strip_ansi(stdout);
    let mut models = Vec::new();
    for line in cleaned.lines() {
        let line = line.trim();
        if line.is_empty() || is_header(line) {
            continue;
        }
        let line = strip_status_markers(line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (id, name) = match split_id_name(line) {
            Some((id, name)) => (id.trim(), name.trim()),
            None if !line.contains(' ') => (line, line),
            None => continue,
        };
        if id.is_empty() || name.is_empty() || id.contains(' ') {
            continue;
        }
        models.push(ModelOption {
            id: id.to_string(),
            name: name.to_string(),
        });
    }
    dedup_keep_first(&mut models);
    models
}

fn is_header(line: &str) -> bool {
    line.starts_with("Available") || line.starts_with("Error")
}

fn split_id_name(line: &str) -> Option<(&str, &str)> {
    for sep in [" - ", " – ", " — "] {
        if let Some(parts) = line.split_once(sep) {
            return Some(parts);
        }
    }
    None
}

/// Drop `(current)` / `(default)` markers, including `(current, default)`.
/// Other parentheticals, such as `(NO ZDR)`, stay.
fn strip_status_markers(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('(') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(')') else {
            out.push_str(rest);
            return out;
        };
        let inner = after[..end].trim();
        let parts: Vec<&str> = inner
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        let marker = !parts.is_empty()
            && parts
                .iter()
                .all(|part| *part == "current" || *part == "default");
        if marker {
            out.push_str(&rest[..start]);
            if out.ends_with(' ') {
                out.pop();
            }
            rest = &after[end + 1..];
        } else {
            let keep = start + 1 + end + 1;
            out.push_str(&rest[..keep]);
            rest = &rest[keep..];
        }
    }
    out.push_str(rest);
    out
}

/// Strip CSI / OSC ANSI sequences so colorized CLI output still parses.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                for ch in chars.by_ref() {
                    if ch == '\u{7}' {
                        break;
                    }
                    if ch == '\u{1b}' {
                        // ST is ESC \
                        if matches!(chars.peek(), Some('\\')) {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

fn dedup_keep_first(models: &mut Vec<ModelOption>) {
    let mut seen = std::collections::HashSet::new();
    models.retain(|model| seen.insert(model.id.clone()));
}

fn parse_command_output(stdout: &str, stderr: &str) -> Vec<ModelOption> {
    let from_stdout = parse_cursor_models_stdout(stdout);
    if !from_stdout.is_empty() {
        return from_stdout;
    }
    let from_stderr = parse_cursor_models_stdout(stderr);
    if !from_stderr.is_empty() {
        return from_stderr;
    }
    if stdout.trim().is_empty() || stderr.trim().is_empty() {
        return Vec::new();
    }
    parse_cursor_models_stdout(&format!("{stdout}\n{stderr}"))
}

/// Same lookup the Ready probe uses: an absolute path when `command` is on
/// `path`, otherwise the name unchanged.
pub fn resolve_model_program(command: &str, path: &OsStr) -> PathBuf {
    coppice_connectors::probe::resolve_binary(command, path)
        .unwrap_or_else(|| PathBuf::from(command))
}

/// Live-list Cursor models.
///
/// The child is not a TTY (stdout and stderr are pipes, stdin is null), which
/// is the same shape as the connector probe. The desktop Ready check does not
/// run this command: Cursor is Ready when an auth file exists. This spawn uses
/// the probe's resolved absolute path and the same `PATH` and `HOME`, then
/// waits up to [`MODEL_LIST_TIMEOUT`].
pub async fn list_cursor_models(command: &str) -> anyhow::Result<Vec<ModelOption>> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let home = std::env::var_os("HOME").unwrap_or_default();
    list_cursor_models_in(command, &path, &home).await
}

pub async fn list_cursor_models_in(
    command: &str,
    path: &OsStr,
    home: &OsStr,
) -> anyhow::Result<Vec<ModelOption>> {
    let program = resolve_model_program(command, path);
    let mut failures: Vec<cli_catalog::ModelListError> = Vec::new();
    for args in MODEL_ARG_SETS {
        match run_model_args(&program, args, path, home).await {
            Ok(models) => return Ok(models),
            Err(err) => failures.push(err),
        }
    }
    let detail = failures
        .iter()
        .find_map(|err| {
            err.detail
                .as_ref()
                .filter(|line| line.contains("Authentication required"))
                .cloned()
        })
        .or_else(|| failures.iter().find_map(|err| err.detail.clone()));
    let message = failures
        .last()
        .map(|err| err.message.clone())
        .unwrap_or_else(|| format!("no cursor models parsed from `{}`", program.display()));
    Err(cli_catalog::model_list_err(message, detail))
}

async fn run_model_args(
    program: &std::path::Path,
    args: &[&str],
    path: &OsStr,
    home: &OsStr,
) -> Result<Vec<ModelOption>, cli_catalog::ModelListError> {
    let shown = format!("`{} {}`", program.display(), args.join(" "));
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .env("PATH", path)
        .env("HOME", home)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = match tokio::time::timeout(MODEL_LIST_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            let message = format!("failed to spawn {shown}: {err}");
            tracing::warn!(%message, "cursor model list failed");
            return Err(cli_catalog::ModelListError {
                detail: Some(err.to_string()),
                message,
            });
        }
        Err(_) => {
            let message = format!("{shown} timed out after {}s", MODEL_LIST_TIMEOUT.as_secs());
            tracing::warn!(%message, "cursor model list timed out");
            return Err(cli_catalog::ModelListError {
                detail: Some(message.clone()),
                message,
            });
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_line = error_detail(&stderr);
    if !output.status.success() {
        tracing::warn!(
            status = %output.status,
            stderr = %truncate_for_err(&stderr),
            stdout = %truncate_for_err(&stdout),
            command = %shown,
            "cursor model list command failed"
        );
    }
    let models = parse_command_output(&stdout, &stderr);
    if !models.is_empty() {
        return Ok(models);
    }
    let message = if output.status.success() {
        format!(
            "no cursor models parsed from {shown} (stdout={} stderr={})",
            truncate_for_err(&stdout),
            truncate_for_err(&stderr)
        )
    } else {
        format!(
            "{shown} exited {} (stdout={} stderr={})",
            output.status,
            truncate_for_err(&stdout),
            truncate_for_err(&stderr)
        )
    };
    tracing::warn!(%message, "cursor model list parsed nothing");
    Err(cli_catalog::ModelListError {
        detail: stderr_line,
        message,
    })
}

/// First stderr line, or the line that contains `Authentication required`
/// when that is not the first line. The form matches that substring.
fn error_detail(text: &str) -> Option<String> {
    let cleaned = strip_ansi(text);
    let lines: Vec<&str> = cleaned
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let line = lines
        .iter()
        .copied()
        .find(|line| line.contains("Authentication required"))
        .or_else(|| lines.first().copied())?;
    Some(truncate_for_err(line))
}

fn truncate_for_err(text: &str) -> String {
    const MAX: usize = 500;
    let trimmed = text.trim();
    if trimmed.len() <= MAX {
        trimmed.to_string()
    } else {
        format!("{}…", &trimmed[..MAX])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    /// Captured from a signed-in `agent models` in a terminal. Some display
    /// names contain two spaces (`Grok 4.7  Low`).
    const USER_LINUX_AGENT_MODELS: &str = "\
Available models

auto - Auto (current, default)
gpt-5.3-codex-low - Codex 5.3 Low
composer-2.5 - Composer 2.5
claude-opus-5-thinking-high - Claude Opus 5 1M Thinking
claude-fable-5-thinking-high - Claude Fable 5 1M Thinking (NO ZDR)
grok-4.7-low - Grok 4.7  Low
cursor-grok-4.6-low-fast - Grok 4.6 Low Fast
";

    #[test]
    fn parses_terminal_fixture_and_strips_current_default_markers() {
        let models = parse_cursor_models_stdout(USER_LINUX_AGENT_MODELS);
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec![
                "auto",
                "gpt-5.3-codex-low",
                "composer-2.5",
                "claude-opus-5-thinking-high",
                "claude-fable-5-thinking-high",
                "grok-4.7-low",
                "cursor-grok-4.6-low-fast",
            ]
        );
        assert_eq!(models[0].name, "Auto");
        assert_eq!(models[4].name, "Claude Fable 5 1M Thinking (NO ZDR)");
        assert_eq!(models[5].name, "Grok 4.7  Low");
    }

    #[test]
    fn parses_agent_models_lines() {
        let stdout = "\
auto - Auto (current, default)
composer-2.5 - Composer 2.5
gpt-5.5-high - GPT-5.5 1M High
";
        let models = parse_cursor_models_stdout(stdout);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].name, "Auto");
        assert_eq!(models[1].id, "composer-2.5");
        assert_eq!(models[2].id, "gpt-5.5-high");
    }

    #[test]
    fn skips_headers_and_blank_lines() {
        let stdout = "Available models\n\nauto - Auto\n";
        let models = parse_cursor_models_stdout(stdout);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].name, "Auto");
    }

    #[test]
    fn parses_ansi_colorized_lines() {
        let stdout = "\u{1b}[1mauto\u{1b}[0m - \u{1b}[32mAuto (default)\u{1b}[0m\n";
        let models = parse_cursor_models_stdout(stdout);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].name, "Auto");
    }

    #[test]
    fn strips_current_and_default_markers() {
        for name in [
            "(current)",
            "(default)",
            "(default, current)",
            "(current, default)",
        ] {
            let stdout = format!("auto - Auto {name}\n");
            let models = parse_cursor_models_stdout(&stdout);
            assert_eq!(models[0].name, "Auto", "{name}");
        }
    }

    #[test]
    fn parses_bare_ids() {
        let stdout = "auto\ncomposer-2.5\n";
        let models = parse_cursor_models_stdout(stdout);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].name, "auto");
        assert_eq!(models[1].id, "composer-2.5");
    }

    #[test]
    fn bare_id_with_marker_keeps_auto() {
        let models = parse_cursor_models_stdout("auto (current, default)\n");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].name, "auto");
    }

    #[cfg(unix)]
    fn write_exe(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_uses_resolved_path_and_probe_env_and_parses_piped_output() {
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let record = dir.path().join("env.txt");
        // Shell builtins only: PATH is just this directory, so `cat` is absent.
        let mut script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$0\" > '{}'\nprintf '%s\\n' \"$PATH\" >> '{}'\nprintf '%s\\n' \"$HOME\" >> '{}'\nwhile IFS= read -r line || [ -n \"$line\" ]; do\n  printf '%s\\n' \"$line\"\ndone <<'EOF'\n",
            record.display(),
            record.display(),
            record.display(),
        );
        script.push_str(USER_LINUX_AGENT_MODELS);
        script.push_str("EOF\n");
        let bin = write_exe(dir.path(), "agent", &script);
        let models =
            list_cursor_models_in("agent", dir.path().as_os_str(), home.path().as_os_str())
                .await
                .expect("models");
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].name, "Auto");
        let recorded = std::fs::read_to_string(&record).unwrap();
        let mut lines = recorded.lines();
        assert_eq!(lines.next(), Some(bin.to_str().unwrap()));
        assert_eq!(lines.next(), Some(dir.path().to_str().unwrap()));
        assert_eq!(lines.next(), Some(home.path().to_str().unwrap()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn falls_back_to_list_models_flag_and_stderr_table() {
        let dir = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\nif [ \"$1\" = \"--list-models\" ]; then\n  echo 'not a table' >&2\n  echo 'composer-2.5 - Composer 2.5' >&2\n  exit 0\nfi\nexit 1\n";
        write_exe(dir.path(), "agent", script);
        let models = list_cursor_models_in("agent", dir.path().as_os_str(), OsStr::new("/tmp"))
            .await
            .expect("models");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "composer-2.5");
        assert_eq!(models[0].name, "Composer 2.5");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn authentication_required_line_is_the_error_detail() {
        let dir = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\necho 'could not list models' >&2\necho 'Error: Authentication required' >&2\nexit 1\n";
        write_exe(dir.path(), "agent", script);
        let err = list_cursor_models_in("agent", dir.path().as_os_str(), OsStr::new("/tmp"))
            .await
            .expect_err("auth");
        assert_eq!(
            cli_catalog::model_list_detail(&err).as_deref(),
            Some("Error: Authentication required")
        );
    }
}
