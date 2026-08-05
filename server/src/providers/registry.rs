use std::collections::HashMap;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::providers::claude_code::ClaudeCodeProvider;
use crate::providers::codex::CodexProvider;
use crate::providers::cursor::CursorProvider;
use crate::providers::kilo_code::KiloCodeProvider;
use crate::providers::mock::MockProvider;
use crate::providers::opencode::OpenCodeProvider;
use crate::providers::AgentProvider;
use crate::sessions::opencode_serve::OpenCodeServeManager;

pub struct ConnectorRegistry {
    connectors: HashMap<String, Arc<dyn AgentProvider>>,
    opencode_model_providers: Vec<String>,
    claude_code_model_providers: Vec<String>,
    codex_model_providers: Vec<String>,
    kilo_code_model_providers: Vec<String>,
    cursor_model_providers: Vec<String>,
}

impl ConnectorRegistry {
    pub fn from_config(
        config: &AppConfig,
        opencode_serve: Option<Arc<OpenCodeServeManager>>,
    ) -> Self {
        let mut connectors: HashMap<String, Arc<dyn AgentProvider>> = HashMap::new();
        connectors.insert("mock".into(), Arc::new(MockProvider::default()));

        let opencode_available = config.agent.connectors.opencode.enabled
            || opencode_serve.is_some();
        if opencode_available {
            if let Some(serve) = opencode_serve {
                connectors.insert(
                    "opencode".into(),
                    Arc::new(OpenCodeProvider::new(
                        serve,
                        config.agent.connectors.opencode.clone(),
                    )),
                );
            }
        }

        if config.agent.connectors.claude_code.enabled {
            connectors.insert(
                "claude-code".into(),
                Arc::new(ClaudeCodeProvider::new(
                    config.agent.connectors.claude_code.clone(),
                )),
            );
        }

        if config.agent.connectors.codex.enabled {
            connectors.insert(
                "codex".into(),
                Arc::new(CodexProvider::new(
                    config.agent.connectors.codex.clone(),
                )),
            );
        }

        if config.agent.connectors.kilo_code.enabled {
            connectors.insert(
                "kilo-code".into(),
                Arc::new(KiloCodeProvider::new(
                    config.agent.connectors.kilo_code.clone(),
                )),
            );
        }

        if config.agent.connectors.cursor.enabled {
            connectors.insert(
                "cursor".into(),
                Arc::new(CursorProvider::new(
                    config.agent.connectors.cursor.clone(),
                )),
            );
        }

        Self {
            connectors,
            opencode_model_providers: config.agent.connectors.opencode.model_providers.clone(),
            claude_code_model_providers: config.agent.connectors.claude_code.model_providers.clone(),
            codex_model_providers: config.agent.connectors.codex.model_providers.clone(),
            kilo_code_model_providers: config.agent.connectors.kilo_code.model_providers.clone(),
            cursor_model_providers: config.agent.connectors.cursor.model_providers.clone(),
        }
    }

    pub fn has(&self, connector: &str) -> bool {
        self.connectors.contains_key(connector)
    }

    pub fn get(&self, connector: &str) -> Option<Arc<dyn AgentProvider>> {
        self.connectors.get(connector).cloned()
    }

    pub fn configured_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.connectors.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn model_providers_for(&self, connector: &str) -> Vec<String> {
        match connector {
            "opencode" => self.opencode_model_providers.clone(),
            "claude-code" => self.claude_code_model_providers.clone(),
            "codex" => self.codex_model_providers.clone(),
            "kilo-code" => self.kilo_code_model_providers.clone(),
            "cursor" => self.cursor_model_providers.clone(),
            _ => vec![],
        }
    }

    pub fn has_model_provider(&self, connector: &str, model_provider: &str) -> bool {
        self.model_providers_for(connector)
            .iter()
            .any(|p| p == model_provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_configured_provider_ids() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(registry.has("mock"));
        assert!(!registry.has("opencode")); // serve not started in test
    }

    #[test]
    fn lists_model_providers_from_config() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.opencode.model_providers = vec!["zai-coding-plan".into()];
        let registry = ConnectorRegistry::from_config(&config, None);
        assert_eq!(
            registry.model_providers_for("opencode"),
            vec!["zai-coding-plan"]
        );
        assert!(registry.model_providers_for("mock").is_empty());
    }

    #[test]
    fn registers_claude_code_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.claude_code.enabled = true;
        config.agent.connectors.claude_code.model_providers =
            vec!["sonnet".into(), "opus".into()];
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(registry.has("claude-code"));
        assert_eq!(
            registry.model_providers_for("claude-code"),
            vec!["sonnet", "opus"]
        );
    }

    #[test]
    fn does_not_register_claude_code_when_disabled() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(!registry.has("claude-code"));
    }

    #[test]
    fn registers_codex_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.codex.enabled = true;
        config.agent.connectors.codex.model_providers =
            vec!["openai".into(), "azure".into()];
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(registry.has("codex"));
        assert_eq!(
            registry.model_providers_for("codex"),
            vec!["openai", "azure"]
        );
    }

    #[test]
    fn does_not_register_codex_when_disabled() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(!registry.has("codex"));
    }

    #[test]
    fn registers_kilo_code_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.kilo_code.enabled = true;
        config.agent.connectors.kilo_code.model_providers =
            vec!["anthropic".into(), "openai".into()];
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(registry.has("kilo-code"));
        assert_eq!(
            registry.model_providers_for("kilo-code"),
            vec!["anthropic", "openai"]
        );
    }

    #[test]
    fn does_not_register_kilo_code_when_disabled() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(!registry.has("kilo-code"));
    }

    #[test]
    fn registers_cursor_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.cursor.enabled = true;
        config.agent.connectors.cursor.model_providers = vec!["cursor".into()];
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(registry.has("cursor"));
        assert_eq!(registry.model_providers_for("cursor"), vec!["cursor"]);
    }

    #[test]
    fn does_not_register_cursor_when_disabled() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, None);
        assert!(!registry.has("cursor"));
    }
}
