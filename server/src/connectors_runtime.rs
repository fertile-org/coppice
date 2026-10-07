//! Live connector config and registry. Enabling or disabling rebuilds the
//! registry in this process and, when a config path is set, patches that file.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use coppice_config::connector_file::{write_toml_atomic, ConnectorFilePatch};
use coppice_config::settings_file::{self, ConfigDocument, ConfigTextError, SaveError};
use coppice_config::AppConfig;
use coppice_connectors::{ConnectorDescriptor, CURSOR, KILO_CODE, MOCK, OPENCODE};
use toml_edit::DocumentMut;

use crate::providers::ConnectorRegistry;
use crate::sessions::opencode_run_server::OpenCodeRunServers;

#[derive(Debug)]
pub struct SavedConfigFile {
    pub revision: String,
    pub restart_required: bool,
    pub backup_available: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigFileError {
    #[error("config file is not available")]
    NoPath,
    #[error("{message}")]
    Invalid {
        line: u32,
        column: u32,
        message: String,
    },
    #[error("config changed on disk")]
    Conflict { text: String, revision: String },
    #[error("config backup is missing")]
    BackupMissing,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<ConfigTextError> for ConfigFileError {
    fn from(error: ConfigTextError) -> Self {
        Self::Invalid {
            line: error.line,
            column: error.column,
            message: error.message,
        }
    }
}

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

    pub fn config_path(&self) -> Option<PathBuf> {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .path
            .clone()
    }

    /// The config file the process writes, plus its current text.
    pub fn read_config_document(&self) -> Result<(PathBuf, ConfigDocument), ConfigFileError> {
        let path = self.config_path().ok_or(ConfigFileError::NoPath)?;
        let document = settings_file::read_config_document(&path)?;
        Ok((path, document))
    }

    pub fn read_backup_text(&self) -> Result<String, ConfigFileError> {
        let path = self.config_path().ok_or(ConfigFileError::NoPath)?;
        settings_file::read_backup_text(&path)?.ok_or(ConfigFileError::BackupMissing)
    }

    /// Write `text` exactly, then rebuild the connector registry from it.
    ///
    /// Connector settings apply in this process. Anything else is reported as
    /// needing a restart. The file is not rewritten a second time.
    pub fn save_config_text(
        &self,
        text: &str,
        base_revision: &str,
    ) -> Result<SavedConfigFile, ConfigFileError> {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let path = guard.path.clone().ok_or(ConfigFileError::NoPath)?;
        let outcome =
            settings_file::save_config_verbatim(&path, text, base_revision).map_err(|error| {
                match error {
                    SaveError::Invalid(error) => ConfigFileError::from(error),
                    SaveError::Conflict { text, revision } => {
                        ConfigFileError::Conflict { text, revision }
                    }
                    SaveError::Io(error) => ConfigFileError::Io(error),
                }
            })?;
        guard.config.agent.connectors = outcome.parsed.agent.connectors;
        guard.registry = Arc::new(ConnectorRegistry::from_config(
            &guard.config,
            self.opencode_runs.clone(),
        ));
        Ok(SavedConfigFile {
            revision: outcome.revision,
            restart_required: outcome.restart_required,
            backup_available: outcome.backup_available,
        })
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

    #[test]
    fn settings_save_keeps_comments_and_enables_the_connector() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# keep me\n[agent.connectors.claude-code]\nenabled = false\n";
        std::fs::write(&path, original).unwrap();
        let rt = runtime(Some(path.clone()));
        let revision = coppice_config::settings_file::content_revision(original.as_bytes());
        let next = "# keep me\n[agent.connectors.claude-code]\nenabled = true\n";

        let saved = rt.save_config_text(next, &revision).unwrap();

        assert!(!saved.restart_required);
        assert!(saved.backup_available);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), next);
        assert_eq!(
            std::fs::read_to_string(path.with_file_name("config.toml.bak")).unwrap(),
            original
        );
        assert!(rt.registry().has(coppice_connectors::CLAUDE_CODE));
        assert!(rt.config().agent.connectors.claude_code.enabled);
    }
}
