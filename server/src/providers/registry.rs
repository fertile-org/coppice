use std::collections::HashMap;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::providers::mock::MockProvider;
use crate::providers::opencode::OpenCodeProvider;
use crate::providers::AgentProvider;
use crate::sessions::opencode_serve::OpenCodeServeManager;

pub struct ConnectorRegistry {
    connectors: HashMap<String, Arc<dyn AgentProvider>>,
    opencode_model_providers: Vec<String>,
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

        Self {
            connectors,
            opencode_model_providers: config.agent.connectors.opencode.model_providers.clone(),
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
}
