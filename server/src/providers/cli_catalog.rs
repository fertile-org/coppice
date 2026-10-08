//! Discover model providers by asking a connector CLI to list models.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Bound for `opencode models` / `kilo models`. A hung CLI must not stall the form.
pub const CLI_LIST_TIMEOUT: Duration = Duration::from_secs(3);

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

/// Run `{command} models`. Failure, timeout, or no parseable lines yield an empty list.
pub async fn discover_cli_providers(command: &str) -> Vec<String> {
    let run = tokio::process::Command::new(command).arg("models").output();
    match tokio::time::timeout(CLI_LIST_TIMEOUT, run).await {
        Ok(Ok(output)) if output.status.success() => {
            parse_cli_provider_ids(&String::from_utf8_lossy(&output.stdout))
        }
        _ => Vec::new(),
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
