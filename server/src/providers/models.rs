//! Per-connector model catalogs: which model providers a connector is
//! configured for, and how to list the models of one of them.

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

#[async_trait]
pub trait ModelCatalog: Send + Sync {
    /// Model providers from the connector's config.
    fn model_providers(&self) -> &[String];

    async fn list_models(&self, model_provider: &str) -> anyhow::Result<Vec<ModelInfo>>;

    /// Whether agent health requires `agent.model_provider` to be one of
    /// `model_providers()`. The mock connector ignores model providers.
    fn checks_model_provider(&self) -> bool {
        true
    }
}

/// The mock connector has no model providers and ignores an agent's choice.
pub struct MockModels;

#[async_trait]
impl ModelCatalog for MockModels {
    fn model_providers(&self) -> &[String] {
        &[]
    }

    async fn list_models(&self, _model_provider: &str) -> anyhow::Result<Vec<ModelInfo>> {
        Ok(vec![])
    }

    fn checks_model_provider(&self) -> bool {
        false
    }
}

pub struct OpenCodeModels {
    pub command: String,
    pub model_providers: Vec<String>,
}

#[async_trait]
impl ModelCatalog for OpenCodeModels {
    fn model_providers(&self) -> &[String] {
        &self.model_providers
    }

    async fn list_models(&self, model_provider: &str) -> anyhow::Result<Vec<ModelInfo>> {
        let models =
            crate::providers::opencode_models::list_opencode_models(&self.command, model_provider)
                .await?;
        Ok(models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                name: m.name,
            })
            .collect())
    }
}

pub struct ClaudeCodeModels {
    pub model_providers: Vec<String>,
}

#[async_trait]
impl ModelCatalog for ClaudeCodeModels {
    fn model_providers(&self) -> &[String] {
        &self.model_providers
    }

    async fn list_models(&self, model_provider: &str) -> anyhow::Result<Vec<ModelInfo>> {
        Ok(known_claude_code_models(model_provider)
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.to_string(),
                name: m.name.to_string(),
            })
            .collect())
    }
}

pub struct CodexModels {
    pub model_providers: Vec<String>,
}

#[async_trait]
impl ModelCatalog for CodexModels {
    fn model_providers(&self) -> &[String] {
        &self.model_providers
    }

    async fn list_models(&self, model_provider: &str) -> anyhow::Result<Vec<ModelInfo>> {
        let models = crate::providers::codex_models::list_codex_models(model_provider).await?;
        Ok(models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                name: m.name,
            })
            .collect())
    }
}

pub struct KiloCodeModels {
    pub command: String,
    pub model_providers: Vec<String>,
}

#[async_trait]
impl ModelCatalog for KiloCodeModels {
    fn model_providers(&self) -> &[String] {
        &self.model_providers
    }

    async fn list_models(&self, model_provider: &str) -> anyhow::Result<Vec<ModelInfo>> {
        let models =
            crate::providers::kilo_models::list_kilo_models(&self.command, model_provider).await?;
        Ok(models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                name: m.name,
            })
            .collect())
    }
}

pub struct CursorModels {
    pub command: String,
    pub model_providers: Vec<String>,
}

#[async_trait]
impl ModelCatalog for CursorModels {
    fn model_providers(&self) -> &[String] {
        &self.model_providers
    }

    /// Cursor has one model namespace; any other provider id lists nothing.
    async fn list_models(&self, model_provider: &str) -> anyhow::Result<Vec<ModelInfo>> {
        if model_provider != "cursor" {
            return Ok(vec![]);
        }
        let models = crate::providers::cursor_models::list_cursor_models(&self.command).await?;
        Ok(models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                name: m.name,
            })
            .collect())
    }
}

struct KnownModel {
    id: &'static str,
    name: &'static str,
}

/// Curated Claude Code model IDs (Anthropic API / subscription CLI).
/// No live CLI catalog exists yet; refresh from https://platform.claude.com/docs/en/about-claude/models/overview
/// Fable 5 is intentionally omitted (not permitted in this deployment).
fn known_claude_code_models(provider_id: &str) -> Vec<KnownModel> {
    match provider_id {
        "sonnet" => vec![
            KnownModel {
                id: "claude-sonnet-4-6",
                name: "Claude Sonnet 4.6",
            },
            KnownModel {
                id: "sonnet[1m]",
                name: "Sonnet 4.6 (1M context)",
            },
            KnownModel {
                id: "claude-sonnet-4-5-20250929",
                name: "Claude Sonnet 4.5",
            },
        ],
        "opus" => vec![
            KnownModel {
                id: "claude-opus-4-8",
                name: "Claude Opus 4.8",
            },
            KnownModel {
                id: "opus[1m]",
                name: "Opus 4.8 (1M context)",
            },
            KnownModel {
                id: "claude-opus-4-7",
                name: "Claude Opus 4.7",
            },
            KnownModel {
                id: "claude-opus-4-6",
                name: "Claude Opus 4.6",
            },
        ],
        "haiku" => vec![
            KnownModel {
                id: "claude-haiku-4-5-20251001",
                name: "Claude Haiku 4.5",
            },
            KnownModel {
                id: "claude-haiku-4-5",
                name: "Claude Haiku 4.5 (alias)",
            },
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_models_include_current_opus_and_exclude_fable() {
        let opus = known_claude_code_models("opus");
        assert!(opus.iter().any(|m| m.id == "claude-opus-4-8"));
        assert!(!opus.iter().any(|m| m.id.contains("fable")));
        let sonnet = known_claude_code_models("sonnet");
        assert!(sonnet.iter().any(|m| m.id == "claude-sonnet-4-6"));
    }

    #[tokio::test]
    async fn cursor_lists_nothing_for_other_providers() {
        let models = CursorModels {
            command: "definitely-not-a-cursor-binary".into(),
            model_providers: vec!["cursor".into(), "other".into()],
        };
        assert!(models.list_models("other").await.expect("empty").is_empty());
    }

    #[tokio::test]
    async fn mock_lists_nothing_and_skips_provider_check() {
        assert!(MockModels.list_models("x").await.expect("empty").is_empty());
        assert!(MockModels.model_providers().is_empty());
        assert!(!MockModels.checks_model_provider());
    }
}
