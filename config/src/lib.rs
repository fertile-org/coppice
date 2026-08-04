use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const LOCAL_CONFIG_FILE: &str = "config.toml";
pub const GLOBAL_CONFIG_DIR: &str = "coppice";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
    pub agent: AgentConfig,
    pub web: WebConfig,
    #[serde(default)]
    pub workflow: WorkflowConfig,
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KnowledgeConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub extraction: ExtractionConfig,
    #[serde(default)]
    pub auto_save: KnowledgeAutoSaveConfig,
    #[serde(default)]
    pub retrieval: KnowledgeRetrievalConfig,
    #[serde(default)]
    pub context_budget: ContextBudgetConfig,
    #[serde(default = "default_knowledge_worker_count")]
    pub worker_count: u32,
    #[serde(default = "default_knowledge_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_knowledge_stale_lock_secs")]
    pub stale_lock_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embedding_provider")]
    pub provider: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_embedding_dimension")]
    pub dimension: usize,
    #[serde(default = "default_embedding_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_embedding_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractionConfig {
    #[serde(default = "default_extraction_provider")]
    pub provider: String,
    #[serde(default = "default_extraction_max_source_bytes")]
    pub max_source_bytes: usize,
    #[serde(default = "default_extraction_max_candidates")]
    pub max_candidates: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KnowledgeAutoSaveConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_types: Vec<String>,
    #[serde(default = "default_auto_save_confidence")]
    pub minimum_confidence: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KnowledgeRetrievalConfig {
    #[serde(default = "default_retrieval_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub allowed_types: Vec<String>,
    #[serde(default = "default_retrieval_min_confidence")]
    pub minimum_confidence: String,
    #[serde(default)]
    pub minimum_similarity: f32,
    #[serde(default = "default_knowledge_list_limit")]
    pub default_page_size: usize,
    #[serde(default = "default_knowledge_list_max")]
    pub max_page_size: usize,
    #[serde(default = "default_knowledge_project_capacity")]
    pub max_active_per_project: i64,
    #[serde(default = "default_knowledge_workspace_capacity")]
    pub max_active_workspace: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContextBudgetConfig {
    #[serde(default = "default_context_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_context_ticket_tokens")]
    pub ticket: usize,
    #[serde(default = "default_context_latest_comments_tokens")]
    pub latest_comments: usize,
    #[serde(default = "default_context_project_rules_tokens")]
    pub project_rules: usize,
    #[serde(default = "default_context_knowledge_tokens")]
    pub retrieved_knowledge: usize,
    #[serde(default = "default_context_previous_tokens")]
    pub previous_attempt_summary: usize,
    #[serde(default = "default_context_output_tokens")]
    pub output_contract: usize,
}

const LOW_RISK_AUTO_SAVE_TYPES: &[&str] = &[
    "bug_pattern",
    "coding_convention",
    "dependency_note",
    "performance_note",
    "review_feedback",
    "test_command",
];

const KNOWLEDGE_TYPES: &[&str] = &[
    "coding_convention",
    "architecture_rule",
    "bug_pattern",
    "test_command",
    "review_feedback",
    "dependency_note",
    "api_contract",
    "workflow_rule",
    "human_preference",
    "operational_runbook",
    "security_rule",
    "performance_note",
];

impl KnowledgeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.embedding.dimension == 0 {
            return Err("knowledge.embedding.dimension must be greater than zero".into());
        }
        if !matches!(
            self.embedding.provider.as_str(),
            "mock" | "openai_compatible"
        ) {
            return Err("knowledge.embedding.provider must be mock or openai_compatible".into());
        }
        if self.embedding.provider == "openai_compatible"
            && self
                .embedding
                .api_key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
        {
            return Err("knowledge.embedding.api_key is required for openai_compatible".into());
        }
        if self.extraction.provider != "mock" {
            return Err("knowledge.extraction.provider must be mock in M06".into());
        }
        if !matches!(self.auto_save.minimum_confidence.as_str(), "high") {
            return Err("knowledge.auto_save.minimum_confidence must be high".into());
        }
        for knowledge_type in &self.auto_save.allowed_types {
            if !LOW_RISK_AUTO_SAVE_TYPES.contains(&knowledge_type.as_str()) {
                return Err(format!(
                    "knowledge.auto_save.allowed_types contains high-impact or unknown type: {knowledge_type}"
                ));
            }
        }
        if self.retrieval.top_k == 0 || self.retrieval.top_k > 20 {
            return Err("knowledge.retrieval.top_k must be between 1 and 20".into());
        }
        for (index, knowledge_type) in self.retrieval.allowed_types.iter().enumerate() {
            if !KNOWLEDGE_TYPES.contains(&knowledge_type.as_str()) {
                return Err(format!(
                    "knowledge.retrieval.allowed_types contains unknown type: {knowledge_type}"
                ));
            }
            if self.retrieval.allowed_types[..index].contains(knowledge_type) {
                return Err(format!(
                    "knowledge.retrieval.allowed_types contains duplicate type: {knowledge_type}"
                ));
            }
        }
        if !matches!(
            self.retrieval.minimum_confidence.as_str(),
            "low" | "medium" | "high"
        ) {
            return Err(
                "knowledge.retrieval.minimum_confidence must be low, medium, or high".into(),
            );
        }
        if self.retrieval.default_page_size == 0
            || self.retrieval.default_page_size > self.retrieval.max_page_size
            || self.retrieval.max_page_size > 100
        {
            return Err(
                "knowledge retrieval page sizes must satisfy 1 <= default <= max <= 100".into(),
            );
        }
        if self.retrieval.max_active_per_project <= 0 || self.retrieval.max_active_workspace <= 0 {
            return Err("knowledge retrieval capacity limits must be greater than zero".into());
        }
        let budget = &self.context_budget;
        if budget.max_tokens == 0
            || budget.output_contract == 0
            || [
                budget.ticket,
                budget.latest_comments,
                budget.project_rules,
                budget.retrieved_knowledge,
                budget.previous_attempt_summary,
                budget.output_contract,
            ]
            .into_iter()
            .any(|allocation| allocation > budget.max_tokens)
        {
            return Err("knowledge context budget is invalid".into());
        }
        Ok(())
    }
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            embedding: EmbeddingConfig::default(),
            extraction: ExtractionConfig::default(),
            auto_save: KnowledgeAutoSaveConfig::default(),
            retrieval: KnowledgeRetrievalConfig::default(),
            context_budget: ContextBudgetConfig::default(),
            worker_count: default_knowledge_worker_count(),
            poll_interval_ms: default_knowledge_poll_interval_ms(),
            stale_lock_secs: default_knowledge_stale_lock_secs(),
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: default_embedding_provider(),
            model: default_embedding_model(),
            dimension: default_embedding_dimension(),
            base_url: default_embedding_base_url(),
            api_key: None,
            timeout_secs: default_embedding_timeout_secs(),
        }
    }
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            provider: default_extraction_provider(),
            max_source_bytes: default_extraction_max_source_bytes(),
            max_candidates: default_extraction_max_candidates(),
        }
    }
}

impl Default for KnowledgeAutoSaveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_types: Vec::new(),
            minimum_confidence: default_auto_save_confidence(),
        }
    }
}

impl Default for KnowledgeRetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: default_retrieval_top_k(),
            allowed_types: Vec::new(),
            minimum_confidence: default_retrieval_min_confidence(),
            minimum_similarity: 0.0,
            default_page_size: default_knowledge_list_limit(),
            max_page_size: default_knowledge_list_max(),
            max_active_per_project: default_knowledge_project_capacity(),
            max_active_workspace: default_knowledge_workspace_capacity(),
        }
    }
}

impl Default for ContextBudgetConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_context_max_tokens(),
            ticket: default_context_ticket_tokens(),
            latest_comments: default_context_latest_comments_tokens(),
            project_rules: default_context_project_rules_tokens(),
            retrieved_knowledge: default_context_knowledge_tokens(),
            previous_attempt_summary: default_context_previous_tokens(),
            output_contract: default_context_output_tokens(),
        }
    }
}

fn default_embedding_provider() -> String {
    "mock".into()
}
fn default_embedding_model() -> String {
    "coppice-mock-1536".into()
}
fn default_embedding_dimension() -> usize {
    1536
}
fn default_embedding_base_url() -> String {
    "https://api.openai.com/v1".into()
}
fn default_embedding_timeout_secs() -> u64 {
    30
}
fn default_extraction_provider() -> String {
    "mock".into()
}
fn default_extraction_max_source_bytes() -> usize {
    24_000
}
fn default_extraction_max_candidates() -> usize {
    5
}
fn default_auto_save_confidence() -> String {
    "high".into()
}
fn default_retrieval_top_k() -> usize {
    8
}
fn default_retrieval_min_confidence() -> String {
    "medium".into()
}
fn default_knowledge_list_limit() -> usize {
    25
}
fn default_knowledge_list_max() -> usize {
    100
}
fn default_knowledge_project_capacity() -> i64 {
    10_000
}
fn default_knowledge_workspace_capacity() -> i64 {
    1_000
}
fn default_knowledge_worker_count() -> u32 {
    1
}
fn default_knowledge_poll_interval_ms() -> u64 {
    500
}
fn default_knowledge_stale_lock_secs() -> u64 {
    300
}
fn default_context_max_tokens() -> usize {
    24_000
}
fn default_context_ticket_tokens() -> usize {
    5_000
}
fn default_context_latest_comments_tokens() -> usize {
    4_000
}
fn default_context_project_rules_tokens() -> usize {
    3_000
}
fn default_context_knowledge_tokens() -> usize {
    4_000
}
fn default_context_previous_tokens() -> usize {
    2_000
}
fn default_context_output_tokens() -> usize {
    1_000
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowConfig {
    #[serde(default)]
    pub auto_start_runs: bool,
    #[serde(default)]
    pub auto_assign: AutoAssignConfig,
    #[serde(default)]
    pub auto_split: AutoSplitConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoAssignConfig {
    #[serde(default = "default_true")]
    pub default: bool,
    #[serde(default)]
    pub backlog: Option<bool>,
    #[serde(default)]
    pub ready: Option<bool>,
    #[serde(default)]
    pub in_progress: Option<bool>,
    #[serde(default)]
    pub in_review: Option<bool>,
    #[serde(default)]
    pub in_qa: Option<bool>,
    #[serde(default)]
    pub wait_for_final_review: Option<bool>,
    #[serde(default)]
    pub blocked: Option<bool>,
    #[serde(default)]
    pub done: Option<bool>,
}

fn default_true() -> bool {
    true
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            auto_start_runs: false,
            auto_assign: AutoAssignConfig {
                default: true,
                backlog: Some(false),
                ready: None,
                in_progress: None,
                in_review: None,
                in_qa: None,
                wait_for_final_review: None,
                blocked: None,
                done: None,
            },
            auto_split: AutoSplitConfig::default(),
        }
    }
}

impl Default for AutoAssignConfig {
    fn default() -> Self {
        Self {
            default: true,
            backlog: None,
            ready: None,
            in_progress: None,
            in_review: None,
            in_qa: None,
            wait_for_final_review: None,
            blocked: None,
            done: None,
        }
    }
}

impl AutoAssignConfig {
    pub fn effective(&self, status: &str) -> bool {
        let override_val = match status {
            "backlog" => self.backlog,
            "ready" => self.ready,
            "in_progress" => self.in_progress,
            "in_review" => self.in_review,
            "in_qa" => self.in_qa,
            "wait_for_final_review" => self.wait_for_final_review,
            "blocked" => self.blocked,
            "done" => self.done,
            _ => None,
        };
        override_val.unwrap_or(self.default)
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AutoSplitConfig {
    #[serde(default = "default_false")]
    pub default: bool,
    #[serde(default)]
    pub backlog: Option<bool>,
    #[serde(default)]
    pub ready: Option<bool>,
    #[serde(default)]
    pub in_progress: Option<bool>,
    #[serde(default)]
    pub in_review: Option<bool>,
    #[serde(default)]
    pub in_qa: Option<bool>,
    #[serde(default)]
    pub wait_for_final_review: Option<bool>,
    #[serde(default)]
    pub blocked: Option<bool>,
    #[serde(default)]
    pub done: Option<bool>,
}

impl AutoSplitConfig {
    pub fn effective(&self, status: &str) -> bool {
        let override_val = match status {
            "backlog" => self.backlog,
            "ready" => self.ready,
            "in_progress" => self.in_progress,
            "in_review" => self.in_review,
            "in_qa" => self.in_qa,
            "wait_for_final_review" => self.wait_for_final_review,
            "blocked" => self.blocked,
            "done" => self.done,
            _ => None,
        };
        override_val.unwrap_or(self.default)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebConfig {
    pub port: u16,
    pub static_dir: String,
    pub api_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub session_secret: String,
    pub bootstrap_password: String,
    pub cookie_secure: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub artifacts_dir: String,
    pub max_upload_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    #[serde(alias = "default_provider")]
    pub default_connector: String,
    pub worktrees_path: String,
    pub worker_count: u32,
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u32,
    #[serde(default, alias = "providers")]
    pub connectors: AgentConnectorsConfig,
}

fn default_health_check_interval() -> u32 {
    60
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AgentConnectorsConfig {
    #[serde(default)]
    pub opencode: OpenCodeConnectorConfig,
    #[serde(default, rename = "claude-code")]
    pub claude_code: ClaudeCodeConnectorConfig,
    #[serde(default, rename = "codex")]
    pub codex: CodexConnectorConfig,
    #[serde(default, rename = "kilo-code")]
    pub kilo_code: KiloCodeConnectorConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeConnectorConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_opencode_command")]
    pub command: String,
    #[serde(default = "default_opencode_host")]
    pub serve_hostname: String,
    #[serde(default = "default_opencode_port")]
    pub serve_port: u16,
    /// Max seconds Coppice waits for an OpenCode session to reach idle before failing the run.
    /// Long shell commands (e.g. full `cargo test --workspace`) can exceed the default.
    #[serde(default = "default_opencode_run_timeout_secs")]
    pub run_timeout_secs: u64,
    #[serde(default)]
    pub model_providers: Vec<String>,
}

pub type OpenCodeProviderConfig = OpenCodeConnectorConfig;
pub type AgentProvidersConfig = AgentConnectorsConfig;

fn default_false() -> bool {
    false
}

fn default_opencode_command() -> String {
    "opencode".into()
}

fn default_opencode_host() -> String {
    "127.0.0.1".into()
}

fn default_opencode_port() -> u16 {
    4096
}

fn default_opencode_run_timeout_secs() -> u64 {
    1800
}

impl Default for OpenCodeConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            command: default_opencode_command(),
            serve_hostname: default_opencode_host(),
            serve_port: default_opencode_port(),
            run_timeout_secs: default_opencode_run_timeout_secs(),
            model_providers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeCodeConnectorConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_claude_code_run_timeout_secs")]
    pub run_timeout_secs: u64,
    #[serde(default)]
    pub model_providers: Vec<String>,
}

pub type ClaudeCodeProviderConfig = ClaudeCodeConnectorConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodexConnectorConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_codex_run_timeout_secs")]
    pub run_timeout_secs: u64,
    #[serde(default)]
    pub model_providers: Vec<String>,
}

pub type CodexProviderConfig = CodexConnectorConfig;

fn default_codex_run_timeout_secs() -> u64 {
    600
}

impl Default for CodexConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            run_timeout_secs: default_codex_run_timeout_secs(),
            model_providers: Vec::new(),
        }
    }
}

fn default_claude_code_run_timeout_secs() -> u64 {
    600
}

impl Default for ClaudeCodeConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            run_timeout_secs: default_claude_code_run_timeout_secs(),
            model_providers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KiloCodeConnectorConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_kilo_code_command")]
    pub command: String,
    #[serde(default = "default_kilo_code_run_timeout_secs")]
    pub run_timeout_secs: u64,
    #[serde(default)]
    pub model_providers: Vec<String>,
}

pub type KiloCodeProviderConfig = KiloCodeConnectorConfig;

fn default_kilo_code_command() -> String {
    "kilo".into()
}

fn default_kilo_code_run_timeout_secs() -> u64 {
    600
}

impl Default for KiloCodeConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            command: default_kilo_code_command(),
            run_timeout_secs: default_kilo_code_run_timeout_secs(),
            model_providers: Vec::new(),
        }
    }
}

impl AppConfig {
    /// Load config: defaults → `~/.config/coppice/config.toml` → `./config.toml` →
    /// `COPPICE_CONFIG` file → environment variables (last wins).
    pub fn load() -> Result<Self, Box<figment::Error>> {
        Self::load_figment(Self::base_figment())
    }

    /// Load defaults and environment only (no config files). Useful in unit tests.
    pub fn load_defaults() -> Result<Self, Box<figment::Error>> {
        Self::load_figment(Self::apply_env(Self::defaults_figment()))
    }

    pub fn local_config_path() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(LOCAL_CONFIG_FILE)
    }

    pub fn global_config_path() -> PathBuf {
        directories::BaseDirs::new()
            .map(|dirs| {
                dirs.config_dir()
                    .join(GLOBAL_CONFIG_DIR)
                    .join("config.toml")
            })
            .unwrap_or_else(|| PathBuf::from(".config/coppice/config.toml"))
    }

    pub fn server_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.server.port)
    }

    pub fn web_api_url(&self) -> String {
        self.web
            .api_url
            .clone()
            .unwrap_or_else(|| self.server_url())
    }

    fn load_figment(figment: Figment) -> Result<Self, Box<figment::Error>> {
        let config: Self = figment.extract().map_err(Box::new)?;
        config
            .knowledge
            .validate()
            .map_err(|message| Box::new(figment::Error::from(message)))?;
        Ok(config)
    }

    fn base_figment() -> Figment {
        let mut figment = Self::defaults_figment();

        let global = Self::global_config_path();
        if global.is_file() {
            figment = figment.merge(Toml::file(global));
        }

        let local = Self::local_config_path();
        if local.is_file() {
            figment = figment.merge(Toml::file(local));
        }

        if let Ok(path) = std::env::var("COPPICE_CONFIG") {
            figment = Self::merge_file(figment, Path::new(&path));
        }

        Self::apply_env(figment)
    }

    fn defaults_figment() -> Figment {
        Figment::new().merge(Serialized::defaults(Self::default_values()))
    }

    fn merge_file(figment: Figment, path: &Path) -> Figment {
        figment.merge(Toml::file(path))
    }

    fn apply_env(figment: Figment) -> Figment {
        figment
            .merge(Env::prefixed("COPPICE_").split("_"))
            .merge(Env::prefixed("COPPICE_").split("__"))
            .merge(
                Env::raw()
                    .only(&["DATABASE_URL"])
                    .map(|_| "database.url".into()),
            )
            .merge(
                Env::raw()
                    .only(&["SESSION_SECRET"])
                    .map(|_| "auth.session_secret".into()),
            )
            .merge(
                Env::raw()
                    .only(&["COPPICE_STORAGE__ARTIFACTS_DIR"])
                    .map(|_| "storage.artifacts_dir".into()),
            )
            .merge(
                Env::raw()
                    .only(&["COPPICE_STORAGE__MAX_UPLOAD_BYTES"])
                    .map(|_| "storage.max_upload_bytes".into()),
            )
            .merge(
                Env::raw()
                    .only(&["AGENT_DEFAULT_CONNECTOR", "AGENT_DEFAULT_PROVIDER"])
                    .map(|_| "agent.default_connector".into()),
            )
            .merge(
                Env::raw()
                    .only(&["WORKTREES_PATH"])
                    .map(|_| "agent.worktrees_path".into()),
            )
            .merge(
                Env::raw()
                    .only(&["AGENT_WORKER_COUNT"])
                    .map(|_| "agent.worker_count".into()),
            )
            .merge(
                Env::raw()
                    .only(&["WORKFLOW_AUTO_START_RUNS"])
                    .map(|_| "workflow.auto_start_runs".into()),
            )
    }

    fn default_values() -> Self {
        Self {
            server: ServerConfig { port: 5000 },
            database: DatabaseConfig {
                url: "postgres://coppice:coppice@localhost:5432/coppice".into(),
            },
            auth: AuthConfig {
                session_secret: "dev-secret-change-me".into(),
                bootstrap_password: "changeme".into(),
                cookie_secure: false,
            },
            storage: StorageConfig {
                artifacts_dir: "./data/artifacts".into(),
                max_upload_bytes: 10 * 1024 * 1024,
            },
            agent: AgentConfig {
                default_connector: "mock".into(),
                worktrees_path: "./data/worktrees".into(),
                worker_count: 2,
                health_check_interval_secs: default_health_check_interval(),
                connectors: AgentConnectorsConfig::default(),
            },
            web: WebConfig {
                port: 5001,
                static_dir: "./web/dist".into(),
                api_url: None,
            },
            workflow: WorkflowConfig::default(),
            knowledge: KnowledgeConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn loads_defaults_without_files() {
        let cfg = AppConfig::load_defaults().expect("defaults");
        assert_eq!(cfg.server.port, 5000);
    }

    #[test]
    fn opencode_connector_has_model_providers_default_empty() {
        let cfg = OpenCodeConnectorConfig::default();
        assert!(cfg.model_providers.is_empty());
    }

    #[test]
    fn deserializes_connectors_section() {
        let toml = r#"
        [agent]
        default_connector = "mock"
        worktrees_path = "./data/worktrees"
        worker_count = 2

        [agent.connectors.opencode]
        enabled = true
        model_providers = ["zai-coding-plan", "zai"]
    "#;
        #[derive(Deserialize)]
        struct Wrapper {
            agent: AgentConfig,
        }
        let wrapper: Wrapper = toml::from_str(toml).expect("parse");
        let cfg = wrapper.agent;
        assert_eq!(
            cfg.connectors.opencode.model_providers,
            vec!["zai-coding-plan", "zai"]
        );
    }

    #[test]
    fn deserializes_legacy_provider_keys() {
        let toml = r#"
        [agent]
        default_provider = "opencode"
        worktrees_path = "./data/worktrees"
        worker_count = 2

        [agent.providers.opencode]
        enabled = true
        model_providers = ["anthropic"]
    "#;
        #[derive(Deserialize)]
        struct Wrapper {
            agent: AgentConfig,
        }
        let wrapper: Wrapper = toml::from_str(toml).expect("parse");
        let cfg = wrapper.agent;
        assert_eq!(cfg.default_connector, "opencode");
        assert!(cfg.connectors.opencode.enabled);
        assert_eq!(cfg.connectors.opencode.model_providers, vec!["anthropic"]);
    }

    #[test]
    fn deserializes_claude_code_connector() {
        let toml = r#"
        [agent]
        default_connector = "claude-code"
        worktrees_path = "./data/worktrees"
        worker_count = 2

        [agent.connectors.claude-code]
        enabled = true
        run_timeout_secs = 900
        model_providers = ["sonnet", "opus"]
    "#;
        #[derive(Deserialize)]
        struct Wrapper {
            agent: AgentConfig,
        }
        let wrapper: Wrapper = toml::from_str(toml).expect("parse");
        let cfg = wrapper.agent;
        assert!(cfg.connectors.claude_code.enabled);
        assert_eq!(cfg.connectors.claude_code.run_timeout_secs, 900);
        assert_eq!(
            cfg.connectors.claude_code.model_providers,
            vec!["sonnet", "opus"]
        );
    }

    #[test]
    fn claude_code_connector_defaults() {
        let cfg = ClaudeCodeConnectorConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.run_timeout_secs, 600);
        assert!(cfg.model_providers.is_empty());
    }

    #[test]
    fn deserializes_codex_connector() {
        let toml = r#"
        [agent]
        default_connector = "codex"
        worktrees_path = "./data/worktrees"
        worker_count = 2

        [agent.connectors.codex]
        enabled = true
        run_timeout_secs = 900
        model_providers = ["openai", "azure"]
    "#;
        #[derive(Deserialize)]
        struct Wrapper {
            agent: AgentConfig,
        }
        let wrapper: Wrapper = toml::from_str(toml).expect("parse");
        let cfg = wrapper.agent;
        assert!(cfg.connectors.codex.enabled);
        assert_eq!(cfg.connectors.codex.run_timeout_secs, 900);
        assert_eq!(
            cfg.connectors.codex.model_providers,
            vec!["openai", "azure"]
        );
    }

    #[test]
    fn codex_connector_defaults() {
        let cfg = CodexConnectorConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.run_timeout_secs, 600);
        assert!(cfg.model_providers.is_empty());
    }

    #[test]
    fn deserializes_kilo_code_connector() {
        let toml = r#"
        [agent]
        default_connector = "kilo-code"
        worktrees_path = "./data/worktrees"
        worker_count = 2

        [agent.connectors.kilo-code]
        enabled = true
        command = "kilo"
        run_timeout_secs = 900
        model_providers = ["anthropic", "openai"]
    "#;
        #[derive(Deserialize)]
        struct Wrapper {
            agent: AgentConfig,
        }
        let wrapper: Wrapper = toml::from_str(toml).expect("parse");
        let cfg = wrapper.agent;
        assert!(cfg.connectors.kilo_code.enabled);
        assert_eq!(cfg.connectors.kilo_code.command, "kilo");
        assert_eq!(cfg.connectors.kilo_code.run_timeout_secs, 900);
        assert_eq!(
            cfg.connectors.kilo_code.model_providers,
            vec!["anthropic", "openai"]
        );
    }

    #[test]
    fn kilo_code_connector_defaults() {
        let cfg = KiloCodeConnectorConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.command, "kilo");
        assert_eq!(cfg.run_timeout_secs, 600);
        assert!(cfg.model_providers.is_empty());
    }

    #[test]
    fn agent_default_provider_env_maps_to_connector() {
        let _guard = ENV_LOCK.lock().expect("env lock");

        const KEY: &str = "AGENT_DEFAULT_PROVIDER";
        let previous = std::env::var(KEY).ok();
        std::env::set_var(KEY, "opencode");

        let cfg = AppConfig::load_defaults().expect("config should load");

        match previous {
            Some(value) => std::env::set_var(KEY, value),
            None => std::env::remove_var(KEY),
        }

        assert_eq!(cfg.agent.default_connector, "opencode");
    }

    #[test]
    fn auto_split_default_false() {
        let cfg = WorkflowConfig::default();
        assert!(!cfg.auto_split.default);
        assert!(!cfg.auto_split.effective("backlog"));
        assert!(!cfg.auto_split.effective("ready"));
        assert!(!cfg.auto_split.effective("in_progress"));
    }

    #[test]
    fn workflow_auto_assign_backlog_override() {
        let raw = r#"
        [workflow]
        auto_start_runs = false

        [workflow.auto_assign]
        default = true
        backlog = false
    "#;
        let cfg: AppConfig = toml::from_str(&format!(
            "{raw}\n[server]\nport=5000\n[database]\nurl=\"postgres://x\"\n[auth]\nsession_secret=\"s\"\nbootstrap_password=\"p\"\ncookie_secure=false\n[storage]\nartifacts_dir=\"/tmp\"\nmax_upload_bytes=1\n[agent]\ndefault_connector=\"mock\"\nworktrees_path=\"/tmp\"\nworker_count=1\n[web]\nport=5001\nstatic_dir=\"./web/dist\""
        )).expect("parse");
        assert!(!cfg.workflow.auto_assign.effective("backlog"));
        assert!(cfg.workflow.auto_assign.effective("ready"));
        assert!(cfg.workflow.auto_assign.effective("in_progress"));
    }

    #[test]
    fn storage_artifacts_dir_from_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");

        const KEY: &str = "COPPICE_STORAGE__ARTIFACTS_DIR";
        let previous = std::env::var(KEY).ok();
        std::env::set_var(KEY, "/tmp/coppice-test-artifacts");

        let cfg = AppConfig::load_defaults().expect("config should load");

        match previous {
            Some(value) => std::env::set_var(KEY, value),
            None => std::env::remove_var(KEY),
        }

        assert_eq!(cfg.storage.artifacts_dir, "/tmp/coppice-test-artifacts");
    }

    #[test]
    fn knowledge_defaults_are_fail_closed_and_bounded() {
        let cfg = KnowledgeConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.embedding.dimension, 1536);
        assert_eq!(cfg.embedding.provider, "mock");
        assert!(!cfg.auto_save.enabled);
        assert!(cfg.auto_save.allowed_types.is_empty());
        assert_eq!(cfg.retrieval.top_k, 8);
        assert!(cfg.retrieval.allowed_types.is_empty());
        assert_eq!(cfg.retrieval.max_page_size, 100);
        assert_eq!(cfg.context_budget.max_tokens, 24_000);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn knowledge_rejects_high_impact_auto_save_type() {
        let mut cfg = KnowledgeConfig::default();
        cfg.auto_save.enabled = true;
        cfg.auto_save.allowed_types = vec!["security_rule".into()];
        let error = cfg.validate().expect_err("security rules require humans");
        assert!(error.contains("security_rule"));
    }

    #[test]
    fn knowledge_accepts_explicit_low_risk_auto_save_type() {
        let mut cfg = KnowledgeConfig::default();
        cfg.auto_save.enabled = true;
        cfg.auto_save.allowed_types = vec!["test_command".into()];
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn knowledge_validates_optional_retrieval_type_allowlist() {
        let mut cfg = KnowledgeConfig::default();
        cfg.retrieval.allowed_types = vec!["test_command".into(), "bug_pattern".into()];
        assert!(cfg.validate().is_ok());

        cfg.retrieval.allowed_types = vec!["invented_type".into()];
        let error = cfg
            .validate()
            .expect_err("unknown retrieval types must fail closed");
        assert!(error.contains("invented_type"));
    }

    #[test]
    fn knowledge_validates_every_context_budget_allocation() {
        let mut cfg = KnowledgeConfig::default();
        cfg.context_budget.project_rules = cfg.context_budget.max_tokens + 1;
        assert!(cfg.validate().is_err());

        cfg.context_budget.project_rules = 0;
        cfg.context_budget.output_contract = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn knowledge_nested_values_load_from_double_underscore_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        const WORKERS: &str = "COPPICE_KNOWLEDGE__WORKER_COUNT";
        const TOP_K: &str = "COPPICE_KNOWLEDGE__RETRIEVAL__TOP_K";
        let previous_workers = std::env::var(WORKERS).ok();
        let previous_top_k = std::env::var(TOP_K).ok();
        std::env::set_var(WORKERS, "7");
        std::env::set_var(TOP_K, "3");

        let cfg = AppConfig::load_defaults().expect("knowledge env config should load");

        match previous_workers {
            Some(value) => std::env::set_var(WORKERS, value),
            None => std::env::remove_var(WORKERS),
        }
        match previous_top_k {
            Some(value) => std::env::set_var(TOP_K, value),
            None => std::env::remove_var(TOP_K),
        }
        assert_eq!(cfg.knowledge.worker_count, 7);
        assert_eq!(cfg.knowledge.retrieval.top_k, 3);
    }
}
