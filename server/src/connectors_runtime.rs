//! Live connector config and registry. Enabling or disabling rebuilds the
//! registry in this process and, when a config path is set, patches that file.
//!
//! At startup an Automatic connector is off until its sign-in check passes,
//! which takes a few seconds. Code that would refuse a run for a missing
//! connector waits for that check first ([`ConnectorsRuntime::registry_for`]).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::watch;

use coppice_config::connector_file::{write_toml_atomic, ConnectorFilePatch};
use coppice_config::settings_file::{self, ConfigDocument, ConfigTextError, SaveError};
use coppice_config::AppConfig;
use coppice_connectors::{
    ConnectorDescriptor, COPILOT, CURSOR, GROK, KILO_CODE, KIMI, MOCK, OPENCODE, QWEN_CODE,
};
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
    /// Connector ids whose free sign-in check passed.
    ready: HashSet<String>,
}

/// Longest a run waits for the startup connector check before it is refused.
pub const READINESS_WAIT: Duration = Duration::from_secs(30);

pub struct ConnectorsRuntime {
    opencode_runs: Arc<OpenCodeRunServers>,
    inner: RwLock<Inner>,
    /// False while the startup connector check runs.
    checked: watch::Sender<bool>,
    /// Bumped whenever the registry may have changed, so agent health can be
    /// checked again.
    generation: watch::Sender<u64>,
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
                ready: HashSet::new(),
            }),
            checked: watch::Sender::new(true),
            generation: watch::Sender::new(0),
        }
    }

    /// Rebuild from the latest free sign-in results. Explicit `enabled = false`
    /// stays off. A missing key is on when `ready` contains that id.
    pub fn sync_readiness(&self, ready_ids: impl IntoIterator<Item = impl Into<String>>) {
        let mut guard = self.inner.write().unwrap_or_else(|err| err.into_inner());
        let ready: HashSet<String> = ready_ids.into_iter().map(Into::into).collect();
        if ready == guard.ready {
            return;
        }
        guard.ready = ready;
        rebuild(&mut guard, &self.opencode_runs);
        drop(guard);
        self.bump();
    }

    /// The startup connector check has begun; [`Self::registry_for`] waits for
    /// [`Self::finish_readiness_check`]. Other `sync_readiness` calls (such as
    /// opening Connectors) use partial results and do not end the wait.
    pub fn begin_readiness_check(&self) {
        self.checked.send_replace(false);
    }

    pub fn finish_readiness_check(&self, ready_ids: impl IntoIterator<Item = impl Into<String>>) {
        self.sync_readiness(ready_ids);
        self.checked.send_replace(true);
    }

    pub fn readiness_checked(&self) -> bool {
        *self.checked.borrow()
    }

    /// Waits up to `limit` for the startup connector check. `true` when it is done.
    pub async fn wait_readiness_checked(&self, limit: Duration) -> bool {
        let mut checked = self.checked.subscribe();
        tokio::time::timeout(limit, checked.wait_for(|done| *done))
            .await
            .is_ok_and(|done| done.is_ok())
    }

    /// The registry to run `id` with. When `id` is missing only because the
    /// startup check hasn't finished, waits for it (up to `limit`) first.
    pub async fn registry_for(&self, id: &str, limit: Duration) -> Arc<ConnectorRegistry> {
        let registry = self.registry();
        let automatic = matches!(
            self.config().agent.connectors.switch(id),
            coppice_config::ConnectorSwitch::Automatic
        );
        if registry.has(id)
            || !automatic
            || coppice_connectors::get(id).is_none()
            || self.readiness_checked()
        {
            return registry;
        }
        self.wait_readiness_checked(limit).await;
        self.registry()
    }

    /// Changes whenever the registry may have changed.
    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.generation.subscribe()
    }

    fn bump(&self) {
        self.generation.send_modify(|generation| *generation += 1);
    }

    /// Shared on/off used by the picker, Ready, and the registry.
    /// `None` is an unknown connector id.
    pub fn is_enabled(&self, id: &str) -> Option<bool> {
        coppice_connectors::get(id)?;
        Some(self.registry().has(id))
    }

    pub fn explicitly_off(&self, id: &str) -> bool {
        matches!(
            self.config().agent.connectors.switch(id),
            coppice_config::ConnectorSwitch::Off
        )
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
        guard.config.keybindings = outcome.parsed.keybindings;
        rebuild(&mut guard, &self.opencode_runs);
        drop(guard);
        self.bump();
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
        let currently = guard.registry.has(descriptor.id);
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
        rebuild(&mut guard, &self.opencode_runs);
        drop(guard);
        self.bump();
        Ok(change)
    }
}

fn rebuild(guard: &mut Inner, runs: &Arc<OpenCodeRunServers>) {
    let ready = guard.ready.clone();
    guard.registry = Arc::new(ConnectorRegistry::with_readiness(
        &guard.config,
        runs.clone(),
        &|id| ready.contains(id),
    ));
}

/// Logged when a known connector is turned off and something asks for its models.
pub fn models_turned_off_log(display_name: &str) -> String {
    format!("Can't list {display_name}'s models because it's turned off in Connectors.")
}

/// Shown when a known connector is installed in config terms but turned off.
pub fn turned_off_message(display_name: &str) -> String {
    format!(
        "{display_name} is turned off. Turn it on in System → Connectors, or switch this agent to another connector."
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
        matches!(descriptor.id, CURSOR | KILO_CODE | OPENCODE | COPILOT | QWEN_CODE | KIMI | GROK)
            .then_some(descriptor.binary);
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

    #[tokio::test(start_paused = true)]
    async fn a_run_waits_for_the_startup_check_and_then_has_the_connector() {
        let rt = Arc::new(runtime(None));
        let id = coppice_connectors::CLAUDE_CODE;
        rt.begin_readiness_check();
        let waiter = tokio::spawn({
            let rt = rt.clone();
            async move { rt.registry_for(id, READINESS_WAIT).await.has(id) }
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        // Opening Connectors mid-check syncs partial results; still waiting.
        rt.sync_readiness(Vec::<String>::new());
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(!waiter.is_finished());

        rt.finish_readiness_check([id]);
        assert!(waiter.await.unwrap());
        assert!(rt.readiness_checked());
    }

    #[tokio::test(start_paused = true)]
    async fn an_explicitly_off_connector_is_refused_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();
        let rt = runtime(Some(path));
        let id = coppice_connectors::CLAUDE_CODE;
        rt.set_enabled(id, true).unwrap();
        rt.set_enabled(id, false).unwrap();
        assert!(rt.explicitly_off(id));
        rt.begin_readiness_check();
        let started = tokio::time::Instant::now();
        assert!(!rt.registry_for(id, READINESS_WAIT).await.has(id));
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn a_check_that_never_finishes_stops_the_wait_at_the_limit() {
        let rt = runtime(None);
        let id = coppice_connectors::CLAUDE_CODE;
        rt.begin_readiness_check();
        let started = tokio::time::Instant::now();
        assert!(!rt.registry_for(id, READINESS_WAIT).await.has(id));
        assert_eq!(started.elapsed(), READINESS_WAIT);
    }

    #[tokio::test]
    async fn a_new_runtime_does_not_wait() {
        let rt = runtime(None);
        assert!(rt.readiness_checked());
        assert!(rt.wait_readiness_checked(Duration::ZERO).await);
    }

    #[test]
    fn registry_changes_are_announced_once_per_change() {
        let rt = runtime(None);
        let mut changes = rt.subscribe_changes();
        rt.sync_readiness([coppice_connectors::CLAUDE_CODE]);
        assert!(changes.has_changed().unwrap());
        changes.mark_unchanged();
        rt.sync_readiness([coppice_connectors::CLAUDE_CODE]);
        assert!(!changes.has_changed().unwrap());
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
        assert_eq!(rt.config().agent.connectors.claude_code.enabled, Some(true));
        assert_eq!(
            rt.config().agent.connectors.claude_code.model_providers,
            vec![
                "sonnet".to_string(),
                "opus".to_string(),
                "haiku".to_string(),
                "fable".to_string()
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
        config.agent.connectors.cursor.enabled = Some(true);
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
        assert_eq!(rt.config().agent.connectors.claude_code.enabled, Some(true));
    }

    #[test]
    fn settings_save_applies_keybindings_without_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# keep\n[server]\nport = 5000\n";
        std::fs::write(&path, original).unwrap();
        let rt = runtime(Some(path.clone()));
        let revision = coppice_config::settings_file::content_revision(original.as_bytes());
        let next = "# keep\n[server]\nport = 5000\n\n[keybindings]\n\"palette.open\" = \"Mod+P\"\n";

        let saved = rt.save_config_text(next, &revision).unwrap();

        assert!(!saved.restart_required);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), next);
        let api = rt.config().keybindings.api();
        let binding = &api.overrides["palette.open"][0];
        match binding {
            coppice_config::keybindings::ParsedBinding::Chord { chord } => {
                assert_eq!(chord.key, "p");
                assert!(chord.mod_key);
            }
            other => panic!("expected a chord, got {other:?}"),
        }
        assert!(api.warnings.is_empty());
    }

    #[test]
    fn settings_save_keeps_a_skipped_shortcut_and_applies_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# keep\n[server]\nport = 5000\n";
        std::fs::write(&path, original).unwrap();
        let rt = runtime(Some(path.clone()));
        let revision = coppice_config::settings_file::content_revision(original.as_bytes());
        let next = "\
# keep
[server]
port = 5000

[keybindings]
\"ticket.create\" = \"Mod+W\"
\"palette.open\" = \"Mod+P\"
";

        let saved = rt.save_config_text(next, &revision).unwrap();

        assert!(!saved.restart_required);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), next);
        let api = rt.config().keybindings.api();
        assert!(api.overrides.contains_key("palette.open"));
        assert!(!api.overrides.contains_key("ticket.create"));
        assert_eq!(
            api.warnings,
            vec![
                "\"Mod+W\" belongs to the browser or the system. Create ticket keeps its default."
                    .to_string()
            ]
        );
    }
}
