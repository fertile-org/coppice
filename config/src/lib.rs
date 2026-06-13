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

#[derive(Debug, Clone, Deserialize, Serialize)]
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

impl Default for AutoSplitConfig {
    fn default() -> Self {
        Self {
            default: false,
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
    600
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
            .map(|dirs| dirs.config_dir().join(GLOBAL_CONFIG_DIR).join("config.toml"))
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
        figment.extract().map_err(Box::new)
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
            server: ServerConfig { port: 8080 },
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
                port: 5173,
                static_dir: "./web/dist".into(),
                api_url: None,
            },
            workflow: WorkflowConfig::default(),
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
        assert_eq!(cfg.server.port, 8080);
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
            "{raw}\n[server]\nport=8080\n[database]\nurl=\"postgres://x\"\n[auth]\nsession_secret=\"s\"\nbootstrap_password=\"p\"\ncookie_secure=false\n[storage]\nartifacts_dir=\"/tmp\"\nmax_upload_bytes=1\n[agent]\ndefault_connector=\"mock\"\nworktrees_path=\"/tmp\"\nworker_count=1\n[web]\nport=5173\nstatic_dir=\"./web/dist\""
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
}
