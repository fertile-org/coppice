use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct CodexModelCatalog {
    models: Vec<CodexCatalogModel>,
}

#[derive(Debug, Deserialize)]
struct CodexCatalogModel {
    slug: String,
    display_name: String,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    priority: Option<i32>,
}

pub fn parse_codex_models_json(raw: &str, model_provider: &str) -> Vec<ModelOption> {
    let catalog: CodexModelCatalog = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut models: Vec<(i32, ModelOption)> = catalog
        .models
        .into_iter()
        .filter(model_is_visible)
        .filter(|model| model_matches_provider(&model.slug, model_provider))
        .map(|model| {
            (
                model.priority.unwrap_or(0),
                ModelOption {
                    id: model.slug,
                    name: model.display_name,
                },
            )
        })
        .collect();

    models.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    let mut options: Vec<ModelOption> = models.into_iter().map(|(_, option)| option).collect();
    options.dedup_by(|a, b| a.id == b.id);
    options
}

fn model_is_visible(model: &CodexCatalogModel) -> bool {
    !matches!(model.visibility.as_deref(), Some("hide"))
}

fn model_matches_provider(slug: &str, provider_id: &str) -> bool {
    match provider_id {
        "azure" => slug.starts_with("azure/"),
        "openai" => !slug.starts_with("azure/"),
        other => slug.starts_with(&format!("{other}/")),
    }
}

pub async fn list_codex_models(model_provider: &str) -> anyhow::Result<Vec<ModelOption>> {
    let output = tokio::process::Command::new("codex")
        .args(["debug", "models"])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("codex debug models failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = parse_codex_models_json(&stdout, model_provider);
    if models.is_empty() {
        anyhow::bail!("no codex models available for provider `{model_provider}`");
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CATALOG: &str = r#"{
        "models": [
            {
                "slug": "gpt-5.5",
                "display_name": "GPT-5.5",
                "visibility": "list",
                "priority": 9
            },
            {
                "slug": "gpt-5.4",
                "display_name": "GPT-5.4",
                "visibility": "list",
                "priority": 8
            },
            {
                "slug": "codex-auto-review",
                "display_name": "Codex Auto Review",
                "visibility": "hide",
                "priority": 1
            },
            {
                "slug": "azure/gpt-5.4",
                "display_name": "Azure GPT-5.4",
                "visibility": "list",
                "priority": 5
            }
        ]
    }"#;

    #[test]
    fn parses_openai_models_from_catalog() {
        let models = parse_codex_models_json(SAMPLE_CATALOG, "openai");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-5.5");
        assert_eq!(models[0].name, "GPT-5.5");
        assert_eq!(models[1].id, "gpt-5.4");
    }

    #[test]
    fn parses_azure_models_from_catalog() {
        let models = parse_codex_models_json(SAMPLE_CATALOG, "azure");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "azure/gpt-5.4");
    }

    #[test]
    fn hides_non_list_models() {
        let models = parse_codex_models_json(SAMPLE_CATALOG, "openai");
        assert!(!models.iter().any(|m| m.id == "codex-auto-review"));
    }
}
