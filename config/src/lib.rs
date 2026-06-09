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
    pub default_provider: String,
    pub worktrees_path: String,
    pub worker_count: u32,
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u32,
    #[serde(default)]
    pub providers: AgentProvidersConfig,
}

fn default_health_check_interval() -> u32 {
    60
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AgentProvidersConfig {
    #[serde(default)]
    pub opencode: OpenCodeProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeProviderConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_opencode_command")]
    pub command: String,
    #[serde(default = "default_opencode_host")]
    pub serve_hostname: String,
    #[serde(default = "default_opencode_port")]
    pub serve_port: u16,
    pub model: Option<String>,
    pub variant: Option<String>,
}

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

impl Default for OpenCodeProviderConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            command: default_opencode_command(),
            serve_hostname: default_opencode_host(),
            serve_port: default_opencode_port(),
            model: None,
            variant: None,
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
                    .only(&["AGENT_DEFAULT_PROVIDER"])
                    .map(|_| "agent.default_provider".into()),
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
                default_provider: "mock".into(),
                worktrees_path: "./data/worktrees".into(),
                worker_count: 2,
                health_check_interval_secs: default_health_check_interval(),
                providers: AgentProvidersConfig::default(),
            },
            web: WebConfig {
                port: 5173,
                static_dir: "./web/dist".into(),
                api_url: None,
            },
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
