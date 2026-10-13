//! Discover model providers by asking a connector CLI to list models.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Bound for `opencode models` / `kilo models` provider discovery. A hung CLI must not stall the form.
pub const CLI_LIST_TIMEOUT: Duration = Duration::from_secs(3);

pub use crate::host_env::MODEL_LIST_TIMEOUT;

const CLI_LIST_CACHE: Duration = Duration::from_secs(30);

/// Config override when non-empty; otherwise `builtin`.
pub fn effective_model_providers(configured: &[String], builtin: &[&str]) -> Vec<String> {
    let configured: Vec<String> = configured
        .iter()
        .map(|provider| provider.trim().to_string())
        .filter(|provider| !provider.is_empty())
        .collect();
    if configured.is_empty() {
        builtin
            .iter()
            .map(|provider| (*provider).to_string())
            .collect()
    } else {
        configured
    }
}

/// Provider ids from `provider/model` lines. Other lines are ignored.
pub fn parse_cli_provider_ids(stdout: &str) -> Vec<String> {
    let mut providers = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Error") {
            continue;
        }
        let Some((provider, model)) = line.split_once('/') else {
            continue;
        };
        if provider.is_empty() || model.is_empty() || provider.contains(' ') {
            continue;
        }
        if !providers.iter().any(|existing| existing == provider) {
            providers.push(provider.to_string());
        }
    }
    providers
}

/// User-visible failure for a models listing. Full output is logged, not shown.
pub fn models_failure_message(
    invocation: &str,
    status: impl std::fmt::Display,
    stdout: &str,
    stderr: &str,
) -> String {
    tracing::warn!(
        invocation,
        %status,
        %stdout,
        %stderr,
        "cli models command failed"
    );
    let line = crate::host_env::first_text_line(stderr)
        .or_else(|| crate::host_env::first_text_line(stdout))
        .unwrap_or_else(|| "no output".to_string());
    format!("{invocation} failed ({status}): {line}. Full output is in the log.")
}

/// Run a CLI through the host environment. `Err` is not found, timed out, or a spawn failure.
pub async fn capture_cli(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let host = crate::host_env::current();
    match host.run_with_timeout(command, args, timeout).await {
        crate::host_env::RunOutput::NotFound(err) => Err(err.to_string()),
        crate::host_env::RunOutput::TimedOut { stderr_line } => {
            if let Some(line) = &stderr_line {
                tracing::debug!(command, %line, "cli command timed out");
            }
            Err(format!(
                "`{command}` timed out after {}s",
                timeout.as_secs()
            ))
        }
        crate::host_env::RunOutput::Exit(exit) => {
            let status = match exit.status {
                Some(status) => status,
                None => {
                    let err = exit
                        .stderr_line
                        .unwrap_or_else(|| "unknown error".to_string());
                    tracing::warn!(command, %err, "failed to start cli");
                    return Err(format!("Couldn't start `{command}`: {err}"));
                }
            };
            Ok(std::process::Output {
                status,
                stdout: exit.stdout,
                stderr: exit.stderr,
            })
        }
    }
}

/// Run `{command} models`. Failure, timeout, or no parseable lines yield an empty list.
/// The empty result is not a successful discovery; callers must not cache it.
pub async fn discover_cli_providers(command: &str) -> Vec<String> {
    match capture_cli(command, &["models"], CLI_LIST_TIMEOUT).await {
        Ok(output) if output.status.success() => {
            parse_cli_provider_ids(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(output) => {
            let detail = crate::host_env::first_stderr_line(&output.stderr)
                .unwrap_or_else(|| format!("exit {}", output.status));
            tracing::warn!(command, %detail, "model provider discovery failed");
            Vec::new()
        }
        Err(err) => {
            tracing::warn!(command, error = %err, "model provider discovery failed");
            Vec::new()
        }
    }
}

/// Remembers a CLI provider list so health checks do not respawn the CLI every time.
#[derive(Default)]
pub struct ProviderCache {
    inner: Mutex<Option<(Instant, Vec<String>)>>,
}

impl ProviderCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Option<Vec<String>> {
        let guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard
            .as_ref()
            .and_then(|(at, providers)| (at.elapsed() < CLI_LIST_CACHE).then(|| providers.clone()))
    }

    pub fn store(&self, providers: Vec<String>) {
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        *guard = Some((Instant::now(), providers));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_builtin_providers() {
        assert_eq!(
            effective_model_providers(&[], &["cursor"]),
            vec!["cursor".to_string()]
        );
        assert_eq!(
            effective_model_providers(&[String::new(), "  ".into()], &["openai"]),
            vec!["openai".to_string()]
        );
    }

    #[test]
    fn non_empty_config_replaces_builtin_providers() {
        assert_eq!(
            effective_model_providers(&["azure".into(), "openai".into()], &["openai"]),
            vec!["azure".to_string(), "openai".to_string()]
        );
    }

    #[test]
    fn models_failure_uses_the_first_line_and_points_at_the_log() {
        let long = "x".repeat(250);
        let message = models_failure_message(
            "`agent models`",
            "exit status: 1",
            &format!("ignored stdout\n{long}"),
            "\nnot signed in\nmore detail\n",
        );
        assert_eq!(
            message,
            "`agent models` failed (exit status: 1): not signed in. Full output is in the log."
        );
        let from_stdout = models_failure_message("`agent models`", "exit status: 2", &long, "  \n");
        let line = crate::host_env::first_text_line(&long).unwrap();
        assert_eq!(line.chars().count(), 201);
        assert!(line.ends_with('…'));
        assert_eq!(
            from_stdout,
            format!("`agent models` failed (exit status: 2): {line}. Full output is in the log.")
        );
    }

    #[test]
    fn parses_provider_ids_from_model_lines() {
        let stdout = "\nError: ignored\nanthropic/claude-sonnet\nopenai/gpt-5\nanthropic/claude-opus\nnot a model\n";
        assert_eq!(
            parse_cli_provider_ids(stdout),
            vec!["anthropic".to_string(), "openai".to_string()]
        );
    }

    #[tokio::test]
    async fn missing_cli_falls_back_to_no_providers() {
        let providers = discover_cli_providers("coppice-missing-model-cli").await;
        assert!(providers.is_empty());
    }
}
