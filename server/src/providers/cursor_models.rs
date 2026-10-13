use crate::providers::cli_catalog;

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

/// Live-list Cursor models through [`crate::host_env::HostEnv`].
///
/// The child is not a TTY. Cursor is Ready from an auth file, not this command.
/// `models` is tried first, then `--list-models`. Both go through
/// [`HostEnv::run_with_timeout`](crate::host_env::HostEnv::run_with_timeout).
pub async fn list_cursor_models(command: &str) -> anyhow::Result<Vec<ModelOption>> {
    list_cursor_models_with(crate::host_env::current().as_ref(), command).await
}

pub async fn list_cursor_models_with(
    host: &crate::host_env::HostEnv,
    command: &str,
) -> anyhow::Result<Vec<ModelOption>> {
    let mut failures: Vec<cli_catalog::ModelListError> = Vec::new();
    for args in MODEL_ARG_SETS {
        match run_model_args(host, command, args).await {
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
        .first()
        .map(|err| err.message.clone())
        .unwrap_or_else(|| {
            format!("`{command} models` listed no models: no output. Full output is in the log.")
        });
    Err(cli_catalog::model_list_err(message, detail))
}

async fn run_model_args(
    host: &crate::host_env::HostEnv,
    command: &str,
    args: &[&str],
) -> Result<Vec<ModelOption>, cli_catalog::ModelListError> {
    let invocation = format!("`{command} {}`", args.join(" "));
    match host
        .run_with_timeout(command, args, crate::host_env::MODEL_LIST_TIMEOUT)
        .await
    {
        crate::host_env::RunOutput::NotFound(missing) => {
            let message = missing.to_string();
            Err(cli_catalog::ModelListError {
                detail: Some(message.clone()),
                message,
            })
        }
        crate::host_env::RunOutput::TimedOut { stderr_line } => {
            let message = format!(
                "`{command}` timed out after {}s",
                crate::host_env::MODEL_LIST_TIMEOUT.as_secs()
            );
            Err(cli_catalog::ModelListError {
                detail: stderr_line.or_else(|| Some(message.clone())),
                message,
            })
        }
        crate::host_env::RunOutput::Exit(exit) => {
            let Some(status) = exit.status else {
                let err = exit
                    .stderr_line
                    .clone()
                    .unwrap_or_else(|| "unknown error".to_string());
                let message = format!("Couldn't start {invocation}: {err}");
                return Err(cli_catalog::ModelListError {
                    detail: Some(err),
                    message,
                });
            };
            let stdout = String::from_utf8_lossy(&exit.stdout);
            let stderr = String::from_utf8_lossy(&exit.stderr);
            let detail = error_detail(&stderr);
            let models = parse_command_output(&stdout, &stderr);
            if !models.is_empty() {
                return Ok(models);
            }
            if !status.success() {
                let message =
                    cli_catalog::models_failure_message(&invocation, status, &stdout, &stderr);
                return Err(cli_catalog::ModelListError { detail, message });
            }
            tracing::warn!(command, %stdout, %stderr, "no cursor models parsed");
            let line = crate::host_env::first_text_line(&stderr)
                .or_else(|| crate::host_env::first_text_line(&stdout))
                .unwrap_or_else(|| "no output".to_string());
            let message =
                format!("{invocation} listed no models: {line}. Full output is in the log.");
            Err(cli_catalog::ModelListError {
                detail: detail.or(Some(line)),
                message,
            })
        }
    }
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
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    fn test_host(path: &OsStr, home: &Path) -> crate::host_env::HostEnv {
        crate::host_env::HostEnv {
            path: path.to_os_string(),
            home: home.to_path_buf(),
            vars: Vec::new(),
            read_at: Instant::now(),
        }
    }

    /// Overlay filesystems can return ETXTBSY for a script that was just executed.
    async fn list_with_retry(
        host: &crate::host_env::HostEnv,
        command: &str,
    ) -> anyhow::Result<Vec<ModelOption>> {
        let mut last = None;
        for _ in 0..4 {
            match list_cursor_models_with(host, command).await {
                Ok(models) => return Ok(models),
                Err(err) if err.to_string().contains("Text file busy") => {
                    last = Some(err);
                    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                }
                Err(err) => return Err(err),
            }
        }
        Err(last.expect("text file busy"))
    }

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
        let host = test_host(dir.path().as_os_str(), home.path());
        let models = list_with_retry(&host, "agent").await.expect("models");
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
        let host = test_host(dir.path().as_os_str(), Path::new("/tmp"));
        let models = list_with_retry(&host, "agent").await.expect("models");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "composer-2.5");
        assert_eq!(models[0].name, "Composer 2.5");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_failure_names_the_models_invocation() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("agent");
        std::fs::write(&bin, []).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let host = test_host(dir.path().as_os_str(), dir.path());
        let err = list_cursor_models_with(&host, "agent")
            .await
            .expect_err("empty executable");
        let text = err.to_string();
        assert!(
            text.starts_with("Couldn't start `agent models`: "),
            "{text}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn authentication_required_line_is_the_error_detail() {
        let dir = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\necho 'could not list models' >&2\necho 'Error: Authentication required' >&2\nexit 1\n";
        write_exe(dir.path(), "agent", script);
        let host = test_host(dir.path().as_os_str(), Path::new("/tmp"));
        let err = list_with_retry(&host, "agent").await.expect_err("auth");
        assert_eq!(
            cli_catalog::model_list_detail(&err).as_deref(),
            Some("Error: Authentication required")
        );
    }

    fn executable(path: &std::path::Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).unwrap();
        }
    }

    #[tokio::test]
    async fn empty_model_list_uses_the_first_line_and_points_at_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let stderr_only = dir.path().join("from-stderr");
        executable(&stderr_only, "echo 'not a table' >&2");
        let host = test_host(OsStr::new("/usr/bin:/bin"), dir.path());
        let err = list_with_retry(&host, &stderr_only.display().to_string())
            .await
            .expect_err("stderr-only output");
        assert_eq!(
            err.to_string(),
            format!(
                "`{} models` listed no models: not a table. Full output is in the log.",
                stderr_only.display()
            )
        );

        let stdout_only = dir.path().join("from-stdout");
        executable(&stdout_only, "echo 'from stdout'");
        let host = test_host(OsStr::new("/usr/bin:/bin"), dir.path());
        let err = list_with_retry(&host, &stdout_only.display().to_string())
            .await
            .expect_err("stdout-only output");
        assert_eq!(
            err.to_string(),
            format!(
                "`{} models` listed no models: from stdout. Full output is in the log.",
                stdout_only.display()
            )
        );
    }
}
