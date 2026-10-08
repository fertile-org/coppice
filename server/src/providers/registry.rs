use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "mock-provider")]
use coppice_connectors::MOCK;
use coppice_connectors::{CLAUDE_CODE, CODEX, CURSOR, KILO_CODE, OPENCODE};

use crate::config::AppConfig;
use crate::providers::claude_code::ClaudeCodeProvider;
use crate::providers::codex::CodexProvider;
use crate::providers::cursor::CursorProvider;
use crate::providers::kilo_code::KiloCodeProvider;
#[cfg(feature = "mock-provider")]
use crate::providers::mock::MockProvider;
#[cfg(feature = "mock-provider")]
use crate::providers::models::MockModels;
use crate::providers::models::{
    ClaudeCodeModels, CodexModels, CursorModels, KiloCodeModels, ModelCatalog, OpenCodeModels,
};
use crate::providers::opencode::OpenCodeProvider;
use crate::providers::AgentProvider;
use crate::sessions::opencode_run_server::OpenCodeRunServers;

/// Shared runtime handles a factory may need beyond config.
pub struct FactoryDeps {
    pub opencode_runs: Arc<OpenCodeRunServers>,
}

pub struct BuiltConnector {
    pub provider: Arc<dyn AgentProvider>,
    pub models: Arc<dyn ModelCatalog>,
}

pub struct ConnectorFactory {
    pub id: &'static str,
    /// `None` when the connector is disabled in config.
    pub build: fn(&AppConfig, &FactoryDeps) -> Option<BuiltConnector>,
}

/// One entry per `coppice_connectors` descriptor.
pub static FACTORIES: &[ConnectorFactory] = &[
    #[cfg(feature = "mock-provider")]
    ConnectorFactory {
        id: MOCK,
        build: build_mock,
    },
    ConnectorFactory {
        id: CURSOR,
        build: build_cursor,
    },
    ConnectorFactory {
        id: CLAUDE_CODE,
        build: build_claude_code,
    },
    ConnectorFactory {
        id: CODEX,
        build: build_codex,
    },
    ConnectorFactory {
        id: KILO_CODE,
        build: build_kilo_code,
    },
    ConnectorFactory {
        id: OPENCODE,
        build: build_opencode,
    },
];

#[cfg(feature = "mock-provider")]
fn build_mock(_config: &AppConfig, _deps: &FactoryDeps) -> Option<BuiltConnector> {
    Some(BuiltConnector {
        provider: Arc::new(MockProvider::default()),
        models: Arc::new(MockModels),
    })
}

fn build_opencode(config: &AppConfig, deps: &FactoryDeps) -> Option<BuiltConnector> {
    let cfg = &config.agent.connectors.opencode;
    if !cfg.enabled {
        return None;
    }
    Some(BuiltConnector {
        provider: Arc::new(OpenCodeProvider::new(
            deps.opencode_runs.clone(),
            cfg.clone(),
        )),
        models: Arc::new(OpenCodeModels::new(
            cfg.command.clone(),
            cfg.model_providers.clone(),
        )),
    })
}

fn build_claude_code(config: &AppConfig, _deps: &FactoryDeps) -> Option<BuiltConnector> {
    let cfg = &config.agent.connectors.claude_code;
    if !cfg.enabled {
        return None;
    }
    Some(BuiltConnector {
        provider: Arc::new(ClaudeCodeProvider::new(cfg.clone())),
        models: Arc::new(ClaudeCodeModels {
            model_providers: cfg.model_providers.clone(),
        }),
    })
}

fn build_codex(config: &AppConfig, _deps: &FactoryDeps) -> Option<BuiltConnector> {
    let cfg = &config.agent.connectors.codex;
    if !cfg.enabled {
        return None;
    }
    Some(BuiltConnector {
        provider: Arc::new(CodexProvider::new(cfg.clone())),
        models: Arc::new(CodexModels {
            model_providers: cfg.model_providers.clone(),
        }),
    })
}

fn build_kilo_code(config: &AppConfig, _deps: &FactoryDeps) -> Option<BuiltConnector> {
    let cfg = &config.agent.connectors.kilo_code;
    if !cfg.enabled {
        return None;
    }
    Some(BuiltConnector {
        provider: Arc::new(KiloCodeProvider::new(cfg.clone())),
        models: Arc::new(KiloCodeModels::new(
            cfg.command.clone(),
            cfg.model_providers.clone(),
        )),
    })
}

fn build_cursor(config: &AppConfig, _deps: &FactoryDeps) -> Option<BuiltConnector> {
    let cfg = &config.agent.connectors.cursor;
    if !cfg.enabled {
        return None;
    }
    Some(BuiltConnector {
        provider: Arc::new(CursorProvider::new(cfg.clone())),
        models: Arc::new(CursorModels {
            command: cfg.command.clone(),
            model_providers: cfg.model_providers.clone(),
        }),
    })
}

pub struct ConnectorRegistry {
    connectors: HashMap<String, BuiltConnector>,
}

impl ConnectorRegistry {
    pub fn from_config(config: &AppConfig, opencode_runs: Arc<OpenCodeRunServers>) -> Self {
        assert!(
            FACTORIES.len() == coppice_connectors::all().len()
                && FACTORIES
                    .iter()
                    .all(|f| coppice_connectors::get(f.id).is_some()),
            "connector factories and descriptors are out of sync"
        );
        let deps = FactoryDeps { opencode_runs };
        let connectors = FACTORIES
            .iter()
            .filter_map(|factory| {
                (factory.build)(config, &deps).map(|built| (factory.id.to_string(), built))
            })
            .collect();
        Self { connectors }
    }

    pub fn has(&self, connector: &str) -> bool {
        self.connectors.contains_key(connector)
    }

    pub fn get(&self, connector: &str) -> Option<Arc<dyn AgentProvider>> {
        self.connectors.get(connector).map(|c| c.provider.clone())
    }

    pub fn models(&self, connector: &str) -> Option<Arc<dyn ModelCatalog>> {
        self.connectors.get(connector).map(|c| c.models.clone())
    }

    pub fn configured_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.connectors.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub async fn has_model_provider(&self, connector: &str, model_provider: &str) -> bool {
        let Some(built) = self.connectors.get(connector) else {
            return false;
        };
        built
            .models
            .model_providers()
            .await
            .iter()
            .any(|provider| provider == model_provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs() -> Arc<OpenCodeRunServers> {
        OpenCodeRunServers::new("opencode".into(), "127.0.0.1".into())
    }

    async fn providers_of(registry: &ConnectorRegistry, id: &str) -> Vec<String> {
        match registry.models(id) {
            Some(models) => models.model_providers().await,
            None => Vec::new(),
        }
    }

    #[test]
    fn factories_and_descriptors_match() {
        for factory in FACTORIES {
            assert!(
                coppice_connectors::get(factory.id).is_some(),
                "factory `{}` has no descriptor",
                factory.id
            );
        }
        for descriptor in coppice_connectors::all() {
            assert!(
                FACTORIES.iter().any(|f| f.id == descriptor.id),
                "descriptor `{}` has no factory",
                descriptor.id
            );
        }
        assert_eq!(FACTORIES.len(), coppice_connectors::all().len());
    }

    #[test]
    fn opencode_stays_off_when_it_is_only_the_default() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.opencode.enabled = false;
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(!registry.has(OPENCODE));
    }

    /// Connector ids may only appear as string literals in adapter code
    /// (`providers/`, `sessions/opencode*`) or in tests; everything else reads
    /// the descriptor or uses the `coppice_connectors` id constants.
    #[test]
    fn no_connector_literals_outside_providers() {
        let needles: Vec<String> = coppice_connectors::all()
            .iter()
            .map(|d| format!("\"{}\"", d.id))
            .collect();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut hits = Vec::new();
        for entry in walkdir::WalkDir::new(&src) {
            let entry = entry.expect("walk src");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path.strip_prefix(&src).expect("relative");
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.starts_with("providers/") || rel_str.starts_with("sessions/opencode") {
                continue;
            }
            let text = std::fs::read_to_string(path).expect("read source");
            for (idx, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("#[cfg(test)]") {
                    break;
                }
                for needle in &needles {
                    if line.contains(needle.as_str()) {
                        hits.push(format!("{rel_str}:{}: {}", idx + 1, line.trim()));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "connector id literals outside providers/:\n{}",
            hits.join("\n")
        );
    }

    #[tokio::test]
    async fn models_catalog_reports_config_model_providers() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.opencode.enabled = true;
        config.agent.connectors.opencode.model_providers = vec!["zai-coding-plan".into()];
        let registry = ConnectorRegistry::from_config(&config, runs());
        let models = registry.models("opencode").expect("opencode catalog");
        assert_eq!(
            models.model_providers().await,
            ["zai-coding-plan".to_string()]
        );
        #[cfg(feature = "mock-provider")]
        assert!(registry
            .models("mock")
            .expect("mock catalog")
            .model_providers()
            .await
            .is_empty());
        #[cfg(not(feature = "mock-provider"))]
        assert!(registry.models("mock").is_none());
        assert!(registry.models("cursor").is_none());
    }

    #[test]
    fn lists_configured_provider_ids() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, runs());
        #[cfg(feature = "mock-provider")]
        assert!(registry.has("mock"));
        #[cfg(not(feature = "mock-provider"))]
        assert!(!registry.has("mock"));
        assert!(!registry.has("opencode"));
    }

    #[test]
    fn registers_opencode_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.opencode.enabled = true;
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(registry.has("opencode"));
    }

    #[tokio::test]
    async fn lists_model_providers_from_config() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.opencode.enabled = true;
        config.agent.connectors.opencode.model_providers = vec!["zai-coding-plan".into()];
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert_eq!(
            providers_of(&registry, "opencode").await,
            vec!["zai-coding-plan"]
        );
        assert!(providers_of(&registry, "mock").await.is_empty());
    }

    #[tokio::test]
    async fn empty_config_uses_builtin_model_providers() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.cursor.enabled = true;
        config.agent.connectors.claude_code.enabled = true;
        config.agent.connectors.codex.enabled = true;
        config.agent.connectors.kilo_code.enabled = true;
        config.agent.connectors.kilo_code.command = "coppice-missing-kilo".into();
        config.agent.connectors.opencode.enabled = true;
        config.agent.connectors.opencode.command = "coppice-missing-opencode".into();
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert_eq!(providers_of(&registry, "cursor").await, vec!["cursor"]);
        assert_eq!(
            providers_of(&registry, "claude-code").await,
            vec!["sonnet", "opus", "haiku"]
        );
        assert_eq!(providers_of(&registry, "codex").await, vec!["openai"]);
        assert!(providers_of(&registry, "kilo-code").await.is_empty());
        assert!(providers_of(&registry, "opencode").await.is_empty());
    }

    #[tokio::test]
    async fn non_empty_config_replaces_builtin_model_providers() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.cursor.enabled = true;
        config.agent.connectors.cursor.model_providers = vec!["other".into()];
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert_eq!(providers_of(&registry, "cursor").await, vec!["other"]);
    }

    #[tokio::test]
    async fn registers_claude_code_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.claude_code.enabled = true;
        config.agent.connectors.claude_code.model_providers = vec!["sonnet".into(), "opus".into()];
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(registry.has("claude-code"));
        assert_eq!(
            providers_of(&registry, "claude-code").await,
            vec!["sonnet", "opus"]
        );
    }

    #[test]
    fn does_not_register_claude_code_when_disabled() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(!registry.has("claude-code"));
    }

    #[tokio::test]
    async fn registers_codex_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.codex.enabled = true;
        config.agent.connectors.codex.model_providers = vec!["openai".into(), "azure".into()];
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(registry.has("codex"));
        assert_eq!(
            providers_of(&registry, "codex").await,
            vec!["openai", "azure"]
        );
    }

    #[test]
    fn does_not_register_codex_when_disabled() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(!registry.has("codex"));
    }

    #[tokio::test]
    async fn registers_kilo_code_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.kilo_code.enabled = true;
        config.agent.connectors.kilo_code.model_providers =
            vec!["anthropic".into(), "openai".into()];
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(registry.has("kilo-code"));
        assert_eq!(
            providers_of(&registry, "kilo-code").await,
            vec!["anthropic", "openai"]
        );
    }

    #[test]
    fn does_not_register_kilo_code_when_disabled() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(!registry.has("kilo-code"));
    }

    #[tokio::test]
    async fn registers_cursor_when_enabled() {
        let mut config = AppConfig::load_defaults().expect("config");
        config.agent.connectors.cursor.enabled = true;
        config.agent.connectors.cursor.model_providers = vec!["cursor".into()];
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(registry.has("cursor"));
        assert_eq!(providers_of(&registry, "cursor").await, vec!["cursor"]);
    }

    /// End-user binaries are built without `mock-provider`. The connector must
    /// be absent from the registry and from the descriptor list.
    #[cfg(not(feature = "mock-provider"))]
    #[test]
    fn release_build_does_not_register_mock() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(!registry.has(coppice_connectors::MOCK));
        assert!(coppice_connectors::get(coppice_connectors::MOCK).is_none());
        assert!(registry
            .configured_ids()
            .iter()
            .all(|id| id != coppice_connectors::MOCK));
    }

    #[test]
    fn does_not_register_cursor_when_disabled() {
        let config = AppConfig::load_defaults().expect("config");
        let registry = ConnectorRegistry::from_config(&config, runs());
        assert!(!registry.has("cursor"));
    }
}
