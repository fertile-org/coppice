use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

/// Parse the stdout of `kilo models <provider>`. Kilo is a documented OpenCode
/// fork and `kilo models` prints one `provider/model` (or bare `model`) entry
/// per line, matching the OpenCode CLI shape. We reuse that parsing strategy;
/// lines that do not match the requested provider are dropped.
pub fn parse_kilo_models_stdout(stdout: &str, model_provider: &str) -> Vec<ModelOption> {
    let prefix = format!("{model_provider}/");
    let mut models = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Error") {
            continue;
        }
        let id = if let Some(rest) = line.strip_prefix(&prefix) {
            rest.to_string()
        } else if line.contains('/') {
            // Belongs to a different provider; skip.
            continue;
        } else {
            line.to_string()
        };
        if !id.is_empty() {
            models.push(ModelOption {
                name: id.clone(),
                id,
            });
        }
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

/// Live-list models for a provider via `kilo models <provider>`.
/// Requires the Kilo CLI to be installed and authenticated on the host.
pub async fn list_kilo_models(
    command: &str,
    model_provider: &str,
) -> anyhow::Result<Vec<ModelOption>> {
    let output = tokio::process::Command::new(command)
        .args(["models", model_provider])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("kilo models failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = parse_kilo_models_stdout(&stdout, model_provider);
    if models.is_empty() {
        anyhow::bail!("no kilo models available for provider `{model_provider}`");
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_provider_prefixed_lines() {
        let stdout = "anthropic/claude-sonnet-4-20250514\nanthropic/claude-opus-4-20250514\n";
        let models = parse_kilo_models_stdout(stdout, "anthropic");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-opus-4-20250514");
        assert_eq!(models[1].id, "claude-sonnet-4-20250514");
    }

    #[test]
    fn drops_other_providers() {
        let stdout =
            "anthropic/claude-sonnet-4-20250514\nopenai/gpt-4o\nanthropic/claude-opus-4-20250514\n";
        let models = parse_kilo_models_stdout(stdout, "anthropic");
        assert_eq!(models.len(), 2);
        assert!(models.iter().all(|m| !m.id.contains("gpt")));
    }

    #[test]
    fn skips_empty_and_error_lines() {
        let stdout = "\nError: not logged in\nanthropic/claude-sonnet-4-20250514\n";
        let models = parse_kilo_models_stdout(stdout, "anthropic");
        assert_eq!(models.len(), 1);
    }
}
