use std::collections::HashMap;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::providers::mock::MockProvider;
use crate::providers::opencode::OpenCodeProvider;
use crate::providers::AgentProvider;
use crate::sessions::opencode_serve::OpenCodeServeManager;

pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn AgentProvider>>,
    opencode_default_model: Option<String>,
}

impl ProviderRegistry {
    pub fn from_config(
        config: &AppConfig,
        opencode_serve: Option<Arc<OpenCodeServeManager>>,
    ) -> Self {
        let mut providers: HashMap<String, Arc<dyn AgentProvider>> = HashMap::new();
        providers.insert("mock".into(), Arc::new(MockProvider::default()));

        let opencode_available = config.agent.providers.opencode.enabled
            || opencode_serve.is_some();
        if opencode_available {
            if let Some(serve) = opencode_serve {
                providers.insert(
                    "opencode".into(),
                    Arc::new(OpenCodeProvider::new(
                        serve,
                        config.agent.providers.opencode.clone(),
                    )),
                );
            }
        }

        Self {
            providers,
            opencode_default_model: config.agent.providers.opencode.model.clone(),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn AgentProvider>> {
        self.providers.get(name).cloned()
    }

    pub fn configured_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.providers.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn default_model_for(&self, provider: &str) -> Option<String> {
        match provider {
            "opencode" => self.opencode_default_model.clone(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_configured_provider_ids() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ProviderRegistry::from_config(&config, None);
        assert!(registry.has("mock"));
        assert!(!registry.has("opencode")); // serve not started in test
    }
}
