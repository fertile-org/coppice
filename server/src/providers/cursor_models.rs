#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

/// Parse `agent models` / `agent --list-models` human text:
/// `id - Display Name` per line.
pub fn parse_cursor_models_stdout(stdout: &str) -> Vec<ModelOption> {
    let mut models = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Available") || line.starts_with("Error") {
            continue;
        }
        let Some((id, name)) = line.split_once(" - ") else {
            continue;
        };
        let id = id.trim();
        let name = name.trim();
        if id.is_empty() || name.is_empty() || id.contains(' ') {
            continue;
        }
        models.push(ModelOption {
            id: id.to_string(),
            name: name.to_string(),
        });
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

pub async fn list_cursor_models(command: &str) -> anyhow::Result<Vec<ModelOption>> {
    let output = tokio::process::Command::new(command)
        .arg("models")
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cursor models failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = parse_cursor_models_stdout(&stdout);
    if models.is_empty() {
        anyhow::bail!("no cursor models available");
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(models[0].name, "Auto (current, default)");
        assert_eq!(models[1].id, "composer-2.5");
        assert_eq!(models[2].id, "gpt-5.5-high");
    }

    #[test]
    fn skips_headers_and_blank_lines() {
        let stdout = "Available models\n\nauto - Auto\n";
        let models = parse_cursor_models_stdout(stdout);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "auto");
    }
}
