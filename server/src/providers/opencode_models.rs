use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

pub fn parse_opencode_models_stdout(stdout: &str, model_provider: &str) -> Vec<ModelOption> {
    let prefix = format!("{model_provider}/");
    let mut models = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Error") {
            continue;
        }
        let id = if let Some(rest) = line.strip_prefix(&prefix) {
            rest.to_string()
        } else if let Some((_provider, model)) = line.rsplit_once('/') {
            model.to_string()
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

pub async fn list_opencode_models(
    command: &str,
    model_provider: &str,
) -> anyhow::Result<Vec<ModelOption>> {
    let output = crate::providers::cli_catalog::capture_cli(
        command,
        &["models", model_provider],
        crate::host_env::MODEL_LIST_TIMEOUT,
    )
    .await
    .map_err(|err| anyhow::anyhow!(err))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = crate::providers::cli_catalog::models_failure_message(
            &format!("`{command} models`"),
            output.status,
            &stdout,
            &stderr,
        );
        return Err(crate::providers::cli_catalog::model_list_err(
            message,
            crate::host_env::first_text_line(&stderr),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_opencode_models_stdout(&stdout, model_provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_provider_prefixed_lines() {
        let stdout = "zai-coding-plan/glm-5.1\nzai-coding-plan/glm-4.7\n";
        let models = parse_opencode_models_stdout(stdout, "zai-coding-plan");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "glm-4.7");
        assert_eq!(models[1].id, "glm-5.1");
    }
}
