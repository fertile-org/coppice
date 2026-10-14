//! Per-connector model catalogs: which model providers a connector is
//! configured for, and how to list the models of one of them.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

/// Successful model lists, per provider id. `refresh=true` always loads again.
const MODEL_LIST_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

pub struct ModelListCache {
    inner: Mutex<HashMap<String, (Instant, Vec<ModelInfo>)>>,
}

impl Default for ModelListCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelListCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, provider: &str, refresh: bool) -> Option<Vec<ModelInfo>> {
        let guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        let (at, models) = guard.get(provider)?;
        cache_hit(at.elapsed(), refresh).then(|| models.clone())
    }

    pub fn store(&self, provider: &str, models: Vec<ModelInfo>) {
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard.insert(provider.to_string(), (Instant::now(), models));
    }
}

fn cache_hit(age: Duration, refresh: bool) -> bool {
    !refresh && age < MODEL_LIST_CACHE_TTL
}

#[async_trait]
pub trait ModelCatalog: Send + Sync {
    /// Model providers: a non-empty config list, otherwise the connector's built-in defaults.
    async fn model_providers(&self) -> Vec<String>;

    /// `refresh` always loads again. A list already in flight is not shared.
    async fn list_models(
        &self,
        model_provider: &str,
        refresh: bool,
    ) -> anyhow::Result<Vec<ModelInfo>>;

    /// Whether agent health requires `agent.model_provider` to be one of
    /// `model_providers()`. The mock connector ignores model providers.
    fn checks_model_provider(&self) -> bool {
        true
    }
}

/// The mock connector has no model providers and ignores an agent's choice.
#[cfg(feature = "mock-provider")]
pub struct MockModels;

#[cfg(feature = "mock-provider")]
#[async_trait]
impl ModelCatalog for MockModels {
    async fn model_providers(&self) -> Vec<String> {
        Vec::new()
    }

    async fn list_models(
        &self,
        _model_provider: &str,
        _refresh: bool,
    ) -> anyhow::Result<Vec<ModelInfo>> {
        Ok(vec![])
    }

    fn checks_model_provider(&self) -> bool {
        false
    }
}

pub struct OpenCodeModels {
    pub command: String,
    /// Non-empty config override. Empty means discover providers from the CLI.
    pub model_providers: Vec<String>,
    cache: crate::providers::cli_catalog::ProviderCache,
    models_cache: ModelListCache,
}

impl OpenCodeModels {
    pub fn new(command: String, model_providers: Vec<String>) -> Self {
        Self {
            command,
            model_providers,
            cache: crate::providers::cli_catalog::ProviderCache::new(),
            models_cache: ModelListCache::new(),
        }
    }
}

#[async_trait]
impl ModelCatalog for OpenCodeModels {
    async fn model_providers(&self) -> Vec<String> {
        let configured =
            crate::providers::cli_catalog::effective_model_providers(&self.model_providers, &[]);
        if !configured.is_empty() {
            return configured;
        }
        if let Some(cached) = self.cache.get().filter(|providers| !providers.is_empty()) {
            return cached;
        }
        let discovered = crate::providers::cli_catalog::discover_cli_providers(&self.command).await;
        if !discovered.is_empty() {
            self.cache.store(discovered.clone());
        }
        discovered
    }

    async fn list_models(
        &self,
        model_provider: &str,
        refresh: bool,
    ) -> anyhow::Result<Vec<ModelInfo>> {
        if let Some(hit) = self.models_cache.get(model_provider, refresh) {
            return Ok(hit);
        }
        let models =
            crate::providers::opencode_models::list_opencode_models(&self.command, model_provider)
                .await?;
        let info: Vec<ModelInfo> = models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                name: m.name,
            })
            .collect();
        self.models_cache.store(model_provider, info.clone());
        Ok(info)
    }
}

pub struct ClaudeCodeModels {
    pub model_providers: Vec<String>,
    models_cache: ModelListCache,
}

impl ClaudeCodeModels {
    pub fn new(model_providers: Vec<String>) -> Self {
        Self {
            model_providers,
            models_cache: ModelListCache::new(),
        }
    }
}

#[async_trait]
impl ModelCatalog for ClaudeCodeModels {
    async fn model_providers(&self) -> Vec<String> {
        crate::providers::cli_catalog::effective_model_providers(
            &self.model_providers,
            coppice_connectors::get(coppice_connectors::CLAUDE_CODE)
                .map(|descriptor| descriptor.default_model_providers)
                .unwrap_or(&[]),
        )
    }

    async fn list_models(
        &self,
        model_provider: &str,
        refresh: bool,
    ) -> anyhow::Result<Vec<ModelInfo>> {
        if let Some(hit) = self.models_cache.get(model_provider, refresh) {
            return Ok(hit);
        }
        let info: Vec<ModelInfo> = known_claude_code_models(model_provider)
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.to_string(),
                name: m.name.to_string(),
            })
            .collect();
        self.models_cache.store(model_provider, info.clone());
        Ok(info)
    }
}

pub struct CodexModels {
    pub command: String,
    pub model_providers: Vec<String>,
    models_cache: ModelListCache,
}

impl CodexModels {
    pub fn new(command: String, model_providers: Vec<String>) -> Self {
        Self {
            command,
            model_providers,
            models_cache: ModelListCache::new(),
        }
    }
}

#[async_trait]
impl ModelCatalog for CodexModels {
    async fn model_providers(&self) -> Vec<String> {
        crate::providers::cli_catalog::effective_model_providers(
            &self.model_providers,
            coppice_connectors::get(coppice_connectors::CODEX)
                .map(|descriptor| descriptor.default_model_providers)
                .unwrap_or(&[]),
        )
    }

    async fn list_models(
        &self,
        model_provider: &str,
        refresh: bool,
    ) -> anyhow::Result<Vec<ModelInfo>> {
        if let Some(hit) = self.models_cache.get(model_provider, refresh) {
            return Ok(hit);
        }
        let models =
            crate::providers::codex_models::list_codex_models(&self.command, model_provider)
                .await?;
        let info: Vec<ModelInfo> = models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                name: m.name,
            })
            .collect();
        self.models_cache.store(model_provider, info.clone());
        Ok(info)
    }
}

pub struct KiloCodeModels {
    pub command: String,
    /// Non-empty config override. Empty means discover providers from the CLI.
    pub model_providers: Vec<String>,
    cache: crate::providers::cli_catalog::ProviderCache,
    /// One `kilo models` listing, shared by every provider of this connector.
    models_stdout: Mutex<Option<(Instant, String)>>,
    models_gate: tokio::sync::Mutex<()>,
}

impl KiloCodeModels {
    pub fn new(command: String, model_providers: Vec<String>) -> Self {
        Self {
            command,
            model_providers,
            cache: crate::providers::cli_catalog::ProviderCache::new(),
            models_stdout: Mutex::new(None),
            models_gate: tokio::sync::Mutex::new(()),
        }
    }

    fn cached_stdout(&self, refresh: bool) -> Option<String> {
        let guard = self
            .models_stdout
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let (at, text) = guard.as_ref()?;
        cache_hit(at.elapsed(), refresh).then(|| text.clone())
    }

    fn store_stdout(&self, text: String) {
        let mut guard = self
            .models_stdout
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        *guard = Some((Instant::now(), text));
    }

    async fn models_stdout(&self, refresh: bool) -> anyhow::Result<String> {
        let _guard = self.models_gate.lock().await;
        if let Some(hit) = self.cached_stdout(refresh) {
            return Ok(hit);
        }
        let text = crate::providers::kilo_models::fetch_kilo_models_stdout(&self.command).await?;
        self.store_stdout(text.clone());
        Ok(text)
    }
}

#[async_trait]
impl ModelCatalog for KiloCodeModels {
    async fn model_providers(&self) -> Vec<String> {
        let configured =
            crate::providers::cli_catalog::effective_model_providers(&self.model_providers, &[]);
        if !configured.is_empty() {
            return configured;
        }
        if let Some(cached) = self.cache.get().filter(|providers| !providers.is_empty()) {
            return cached;
        }
        let discovered =
            crate::providers::kilo_models::discover_kilo_providers(&self.command).await;
        if !discovered.is_empty() {
            self.cache.store(discovered.clone());
        }
        discovered
    }

    async fn list_models(
        &self,
        model_provider: &str,
        refresh: bool,
    ) -> anyhow::Result<Vec<ModelInfo>> {
        let stdout = self.models_stdout(refresh).await?;
        Ok(
            crate::providers::kilo_models::parse_kilo_models_stdout(&stdout, model_provider)
                .into_iter()
                .map(|m| ModelInfo {
                    id: m.id,
                    name: m.name,
                })
                .collect(),
        )
    }
}

pub struct CursorModels {
    pub command: String,
    pub model_providers: Vec<String>,
    models_cache: ModelListCache,
}

impl CursorModels {
    pub fn new(command: String, model_providers: Vec<String>) -> Self {
        Self {
            command,
            model_providers,
            models_cache: ModelListCache::new(),
        }
    }
}

#[async_trait]
impl ModelCatalog for CursorModels {
    async fn model_providers(&self) -> Vec<String> {
        let mut providers = crate::providers::cli_catalog::effective_model_providers(
            &self.model_providers,
            coppice_connectors::get(coppice_connectors::CURSOR)
                .map(|descriptor| descriptor.default_model_providers)
                .unwrap_or(&[]),
        );
        // Old configs can list other ids and omit `cursor`. The CLI still has
        // one namespace, so the picker must keep being able to ask for it.
        if !providers.iter().any(|provider| provider == "cursor") {
            providers.push("cursor".to_string());
        }
        providers
    }

    /// Cursor has one model namespace. Any other provider id is an error.
    async fn list_models(
        &self,
        model_provider: &str,
        refresh: bool,
    ) -> anyhow::Result<Vec<ModelInfo>> {
        if model_provider != "cursor" {
            tracing::warn!(
                provider = model_provider,
                "cursor model list rejected an unknown provider"
            );
            return Err(anyhow::anyhow!("unknown model provider `{model_provider}`"));
        }
        if let Some(hit) = self.models_cache.get(model_provider, refresh) {
            return Ok(hit);
        }
        let models = crate::providers::cursor_models::list_cursor_models(&self.command).await?;
        let info: Vec<ModelInfo> = models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                name: m.name,
            })
            .collect();
        self.models_cache.store(model_provider, info.clone());
        Ok(info)
    }
}

struct KnownModel {
    id: &'static str,
    name: &'static str,
}

/// Curated Claude Code model IDs (Anthropic API / subscription CLI).
/// No live CLI catalog exists yet; refresh from https://platform.claude.com/docs/en/about-claude/models/overview
/// `fable` is a `--model` alias (`claude --help` lists fable, opus, and sonnet).
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
        "fable" => vec![KnownModel {
            id: "fable",
            name: "fable",
        }],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_models_include_fable_alias_and_current_opus() {
        let opus = known_claude_code_models("opus");
        assert!(opus.iter().any(|m| m.id == "claude-opus-4-8"));
        assert!(!opus.iter().any(|m| m.id.contains("fable")));
        let sonnet = known_claude_code_models("sonnet");
        assert!(sonnet.iter().any(|m| m.id == "claude-sonnet-4-6"));
        let fable = known_claude_code_models("fable");
        assert_eq!(fable.len(), 1);
        assert_eq!(fable[0].id, "fable");
        assert_eq!(fable[0].name, "fable");
    }

    #[tokio::test]
    async fn cursor_unknown_provider_is_an_error() {
        let models = CursorModels::new(
            "definitely-not-a-cursor-binary".into(),
            vec!["cursor".into(), "other".into()],
        );
        let err = models
            .list_models("other", false)
            .await
            .expect_err("unknown provider");
        assert!(err.to_string().contains("other"), "{err}");
    }

    #[test]
    fn refresh_does_not_reuse_a_fresh_cache_entry() {
        let cache = ModelListCache::new();
        cache.store(
            "cursor",
            vec![ModelInfo {
                id: "auto".into(),
                name: "Auto".into(),
            }],
        );
        assert!(cache.get("cursor", false).is_some());
        assert!(cache.get("cursor", true).is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn config_without_cursor_still_lists_cursor_models() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("agent");
        std::fs::write(
            &bin,
            "#!/bin/sh\necho 'auto - Auto (current, default)'\necho 'composer-2.5 - Composer 2.5'\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let models = CursorModels::new(bin.display().to_string(), vec!["other".into()]);
        let providers = models.model_providers().await;
        assert!(
            providers.iter().any(|provider| provider == "cursor"),
            "{providers:?}"
        );
        let listed = models
            .list_models("cursor", true)
            .await
            .expect("cursor models");
        assert_eq!(listed[0].id, "auto");
        assert_eq!(listed[0].name, "Auto");
        assert!(listed.iter().any(|model| model.id == "composer-2.5"));
    }

    #[cfg(feature = "mock-provider")]
    #[tokio::test]
    async fn mock_lists_nothing_and_skips_provider_check() {
        assert!(MockModels
            .list_models("x", false)
            .await
            .expect("empty")
            .is_empty());
        assert!(MockModels.model_providers().await.is_empty());
        assert!(!MockModels.checks_model_provider());
    }
}
