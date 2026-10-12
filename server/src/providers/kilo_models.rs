use std::process::Stdio;

use crate::providers::cli_catalog::{self, MODEL_LIST_TIMEOUT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

/// One `kilo models` row. Lines are `kilo/<provider>/<model>` or
/// `kilo/~provider/...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KiloListedModel {
    /// Segment immediately after `kilo/` (`anthropic`, `~openai`).
    pub provider: String,
    /// `--model` value: the line without the `kilo/` prefix. It contains `/`,
    /// so [`crate::providers::kilo_code::KiloCodeProvider`] passes it through.
    pub id: String,
    /// The CLI line, trimmed.
    pub name: String,
}

pub fn parse_kilo_model_line(line: &str) -> Option<KiloListedModel> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("Error") {
        return None;
    }
    let rest = line.strip_prefix("kilo/")?;
    let (provider, model) = rest.split_once('/')?;
    if provider.is_empty() || model.is_empty() || provider.contains(' ') {
        return None;
    }
    Some(KiloListedModel {
        provider: provider.to_string(),
        id: rest.to_string(),
        name: line.to_string(),
    })
}

/// Models whose provider segment matches `model_provider`, in CLI order.
pub fn parse_kilo_models_stdout(stdout: &str, model_provider: &str) -> Vec<ModelOption> {
    let mut models = Vec::new();
    for line in stdout.lines() {
        let Some(parsed) = parse_kilo_model_line(line) else {
            continue;
        };
        if parsed.provider != model_provider {
            continue;
        }
        models.push(ModelOption {
            id: parsed.id,
            name: parsed.name,
        });
    }
    dedup_keep_first(&mut models);
    models
}

/// Provider segments in first-seen order.
pub fn kilo_provider_ids(stdout: &str) -> Vec<String> {
    let mut providers = Vec::new();
    for line in stdout.lines() {
        let Some(parsed) = parse_kilo_model_line(line) else {
            continue;
        };
        if !providers
            .iter()
            .any(|existing| existing == &parsed.provider)
        {
            providers.push(parsed.provider);
        }
    }
    providers
}

fn dedup_keep_first(models: &mut Vec<ModelOption>) {
    let mut seen = std::collections::HashSet::new();
    models.retain(|model| seen.insert(model.id.clone()));
}

/// Run `kilo models` and keep rows for `model_provider`.
pub async fn list_kilo_models(
    command: &str,
    model_provider: &str,
) -> anyhow::Result<Vec<ModelOption>> {
    let stdout = fetch_kilo_models_stdout(command).await?;
    let models = parse_kilo_models_stdout(&stdout, model_provider);
    if models.is_empty() {
        return Err(cli_catalog::model_list_err(
            format!("no kilo models available for provider `{model_provider}`"),
            None,
        ));
    }
    Ok(models)
}

pub async fn fetch_kilo_models_stdout(command: &str) -> anyhow::Result<String> {
    let mut cmd = tokio::process::Command::new(command);
    cmd.arg("models")
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = match tokio::time::timeout(MODEL_LIST_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return Err(cli_catalog::model_list_err(
                format!("failed to spawn `{command} models`: {err}"),
                Some(err.to_string()),
            ));
        }
        Err(_) => {
            let message = format!(
                "`{command} models` timed out after {}s",
                MODEL_LIST_TIMEOUT.as_secs()
            );
            return Err(cli_catalog::model_list_err(message.clone(), Some(message)));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let stderr_line = first_line(&stderr);
    if !output.status.success() {
        tracing::warn!(
            status = %output.status,
            stderr = %stderr,
            command,
            "kilo models command failed"
        );
        return Err(cli_catalog::model_list_err(
            format!("`{command} models` exited {}: {stderr}", output.status),
            stderr_line,
        ));
    }
    let text = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    if kilo_provider_ids(&text).is_empty() {
        return Err(cli_catalog::model_list_err(
            format!("no kilo models parsed from `{command} models`"),
            stderr_line,
        ));
    }
    Ok(text)
}

/// Provider ids from `kilo models`. Failure or timeout yields an empty list.
pub async fn discover_kilo_providers(command: &str) -> Vec<String> {
    let run = tokio::process::Command::new(command).arg("models").output();
    match tokio::time::timeout(cli_catalog::CLI_LIST_TIMEOUT, run).await {
        Ok(Ok(output)) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let text = if stdout.trim().is_empty() {
                String::from_utf8_lossy(&output.stderr).into_owned()
            } else {
                stdout.into_owned()
            };
            kilo_provider_ids(&text)
        }
        _ => Vec::new(),
    }
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
kilo/anthropic/claude-sonnet-4-20250514
kilo/anthropic/claude-opus-4-20250514
kilo/~openai/gpt-4o
kilo/openai/gpt-5
";

    #[test]
    fn groups_by_the_provider_segment_after_kilo() {
        let anthropic = parse_kilo_models_stdout(SAMPLE, "anthropic");
        assert_eq!(
            anthropic,
            vec![
                ModelOption {
                    id: "anthropic/claude-sonnet-4-20250514".into(),
                    name: "kilo/anthropic/claude-sonnet-4-20250514".into(),
                },
                ModelOption {
                    id: "anthropic/claude-opus-4-20250514".into(),
                    name: "kilo/anthropic/claude-opus-4-20250514".into(),
                },
            ]
        );
        let tilde = parse_kilo_models_stdout(SAMPLE, "~openai");
        assert_eq!(tilde.len(), 1);
        assert_eq!(tilde[0].id, "~openai/gpt-4o");
        assert_eq!(tilde[0].name, "kilo/~openai/gpt-4o");
        assert!(parse_kilo_models_stdout(SAMPLE, "openai")
            .iter()
            .all(|model| model.id == "openai/gpt-5"));
    }

    #[test]
    fn provider_ids_follow_first_seen_order() {
        assert_eq!(
            kilo_provider_ids(SAMPLE),
            vec![
                "anthropic".to_string(),
                "~openai".to_string(),
                "openai".to_string()
            ]
        );
    }

    #[test]
    fn skips_empty_and_error_lines() {
        let stdout = "\nError: not logged in\nkilo/anthropic/claude-sonnet-4-20250514\n";
        let models = parse_kilo_models_stdout(stdout, "anthropic");
        assert_eq!(models.len(), 1);
    }

    #[test]
    fn ignores_lines_that_are_not_kilo_prefixed() {
        let stdout = "anthropic/claude-sonnet-4-20250514\nkilo/anthropic/claude-opus-4-20250514\n";
        let models = parse_kilo_models_stdout(stdout, "anthropic");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "anthropic/claude-opus-4-20250514");
    }
}
