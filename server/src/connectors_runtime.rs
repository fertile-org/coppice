//! Live connector config and registry. Enabling or disabling rebuilds the
//! registry in this process and, when a config path is set, patches that file.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use coppice_config::connector_file::{write_toml_atomic, ConnectorFilePatch};
use coppice_config::AppConfig;
use coppice_connectors::{ConnectorDescriptor, CURSOR, KILO_CODE, MOCK, OPENCODE};
use toml_edit::DocumentMut;

use crate::providers::ConnectorRegistry;
use crate::sessions::opencode_run_server::OpenCodeRunServers;

#[derive(Debug, thiserror::Error)]
pub enum ConnectorConfigError {
    #[error("unknown connector")]
    Unknown,
    #[error("mock cannot be toggled")]
    Mock,
    #[error("invalid config toml: {0}")]
    Toml(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorChange {
    pub id: &'static str,
    pub display_name: &'static str,
    /// The enabled flag was different before this call.
    pub changed: bool,
    pub enabled: bool,
}

struct Inner {
    config: AppConfig,
    registry: Arc<ConnectorRegistry>,
    path: Option<PathBuf>,
}

pub struct ConnectorsRuntime {
    opencode_runs: Arc<OpenCodeRunServers>,
    inner: RwLock<Inner>,
}

impl ConnectorsRuntime {
    pub fn new(
        config: AppConfig,
        opencode_runs: Arc<OpenCodeRunServers>,
        path: Option<PathBuf>,
    ) -> Self {
        let registry = Arc::new(ConnectorRegistry::from_config(
            &config,
            opencode_runs.clone(),
        ));
        Self {
            opencode_runs,
            inner: RwLock::new(Inner {
                config,
                registry,
                path,
            }),
        }
    }

    pub fn registry(&self) -> Arc<ConnectorRegistry> {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .registry
            .clone()
    }

    pub fn config(&self) -> AppConfig {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .config
            .clone()
    }

    pub fn set_config_path(&self, path: Option<PathBuf>) {
        self.inner.write().unwrap_or_else(|e| e.into_inner()).path = path;
    }

    /// Turn a connector on or off. A no-op when the flag already matches.
    /// File comments and unrelated keys are kept.
    pub fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<ConnectorChange, ConnectorConfigError> {
        let descriptor = coppice_connectors::get(id).ok_or(ConnectorConfigError::Unknown)?;
        if descriptor.id == MOCK {
            return Err(ConnectorConfigError::Mock);
        }
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let currently = guard
            .config
            .agent
            .connectors
            .enabled(descriptor.id)
            .unwrap_or(false);
        let change = ConnectorChange {
            id: descriptor.id,
            display_name: descriptor.display_name,
            changed: currently != enabled,
            enabled,
        };
        if !change.changed {
            return Ok(change);
        }
        if let Some(path) = guard.path.clone() {
            persist_enabled(&path, descriptor, enabled)?;
        }
        guard
            .config
            .agent
            .connectors
            .set_enabled_flag(descriptor.id, enabled, descriptor.default_model_providers)
            .map_err(|_| ConnectorConfigError::Unknown)?;
        guard.registry = Arc::new(ConnectorRegistry::from_config(
            &guard.config,
            self.opencode_runs.clone(),
        ));
        Ok(change)
    }
}

/// Shown when a known connector is installed in config terms but turned off.
pub fn turned_off_message(display_name: &str) -> String {
    format!(
        "{display_name} is turned off. Turn it on in Tools → Connectors, or switch this agent to another connector."
    )
}

/// Worker-facing error when the live registry has no provider for `id`.
pub fn connector_unavailable_message(id: &str) -> String {
    match coppice_connectors::get(id) {
        Some(descriptor) if descriptor.id != MOCK => turned_off_message(descriptor.display_name),
        _ => format!("agent connector not configured: {id}"),
    }
}

fn persist_enabled(
    path: &std::path::Path,
    descriptor: &ConnectorDescriptor,
    enabled: bool,
) -> Result<(), ConnectorConfigError> {
    let text = if path.is_file() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|err| ConnectorConfigError::Toml(err.to_string()))?;
    let command_if_missing =
        matches!(descriptor.id, CURSOR | KILO_CODE | OPENCODE).then_some(descriptor.binary);
    coppice_config::connector_file::set_connector_enabled(
        &mut doc,
        &ConnectorFilePatch {
            id: descriptor.id,
            enabled,
            default_model_providers: descriptor.default_model_providers,
            command_if_missing,
        },
    )
    .map_err(ConnectorConfigError::Toml)?;
    write_toml_atomic(path, &doc.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(path: Option<PathBuf>) -> ConnectorsRuntime {
        let config = AppConfig::load_defaults().expect("defaults");
        let runs = OpenCodeRunServers::new("opencode".into(), "127.0.0.1".into());
        ConnectorsRuntime::new(config, runs, path)
    }

    #[test]
    fn enable_hot_reloads_the_registry_and_disable_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "# keep\n[agent]\ndefault_connector = \"claude-code\"\n",
        )
        .unwrap();
        let rt = runtime(Some(path.clone()));
        assert!(!rt.registry().has(coppice_connectors::CLAUDE_CODE));

        let on = rt
            .set_enabled(coppice_connectors::CLAUDE_CODE, true)
            .unwrap();
        assert!(on.changed);
        assert_eq!(on.display_name, "Claude Code");
        assert!(rt.registry().has(coppice_connectors::CLAUDE_CODE));
        assert!(rt.config().agent.connectors.claude_code.enabled);
        assert_eq!(
            rt.config().agent.connectors.claude_code.model_providers,
            vec![
                "sonnet".to_string(),
                "opus".to_string(),
                "haiku".to_string()
            ]
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# keep"), "{text}");
        assert!(text.contains("enabled = true"), "{text}");
        assert!(text.contains("sonnet"), "{text}");

        let again = rt
            .set_enabled(coppice_connectors::CLAUDE_CODE, true)
            .unwrap();
        assert!(!again.changed);

        let off = rt
            .set_enabled(coppice_connectors::CLAUDE_CODE, false)
            .unwrap();
        assert!(off.changed);
        assert!(!rt.registry().has(coppice_connectors::CLAUDE_CODE));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("enabled = false"), "{text}");
        assert!(text.contains("# keep"), "{text}");
        assert!(
            text.contains("sonnet"),
            "disable must keep providers: {text}"
        );
    }

    #[test]
    fn disable_does_not_clear_a_custom_command() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[agent.connectors.cursor]\nenabled = true\ncommand = \"my-agent\"\nmodel_providers = [\"cursor\"]\n",
        )
        .unwrap();
        let mut config = AppConfig::load_defaults().expect("defaults");
        config.agent.connectors.cursor.enabled = true;
        config.agent.connectors.cursor.command = "my-agent".into();
        config.agent.connectors.cursor.model_providers = vec!["cursor".into()];
        let runs = OpenCodeRunServers::new("opencode".into(), "127.0.0.1".into());
        let rt = ConnectorsRuntime::new(config, runs, Some(path.clone()));
        rt.set_enabled(coppice_connectors::CURSOR, false).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("command = \"my-agent\""), "{text}");
        assert!(text.contains("enabled = false"), "{text}");
        assert!(!rt.registry().has(coppice_connectors::CURSOR));
    }

    #[test]
    fn mock_and_unknown_are_rejected() {
        let rt = runtime(None);
        let mock_err = rt.set_enabled(coppice_connectors::MOCK, false).unwrap_err();
        // The mock descriptor exists only when the feature is compiled in.
        #[cfg(feature = "mock-provider")]
        assert!(matches!(mock_err, ConnectorConfigError::Mock));
        #[cfg(not(feature = "mock-provider"))]
        assert!(matches!(mock_err, ConnectorConfigError::Unknown));
        assert!(matches!(
            rt.set_enabled("nope", true),
            Err(ConnectorConfigError::Unknown)
        ));
    }
}
