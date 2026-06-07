use figment::{
    providers::{Env, Format, Serialized, Yaml},
    Figment,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
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
    pub static_dir: Option<String>,
}

impl AppConfig {
    pub fn load(config_path: Option<&str>) -> Result<Self, Box<figment::Error>> {
        let mut figment = Figment::new()
            .merge(Serialized::defaults(Self::default_values()))
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
                    .only(&["COPPICE_STORAGE__STATIC_DIR"])
                    .map(|_| "storage.static_dir".into()),
            )
            .merge(
                Env::raw()
                    .only(&["COPPICE_STORAGE__MAX_UPLOAD_BYTES"])
                    .map(|_| "storage.max_upload_bytes".into()),
            );

        if let Some(path) = config_path {
            figment = figment.merge(Yaml::file(path));
        }

        figment.extract().map_err(Box::new)
    }

    pub fn resolve_config_path() -> Option<String> {
        if let Ok(path) = std::env::var("COPPICE_CONFIG") {
            return Some(path);
        }

        let default = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../deploy/config/default.yaml");
        if default.exists() {
            return default.to_str().map(String::from);
        }

        None
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
                static_dir: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_defaults_without_file() {
        let cfg = AppConfig::load(None).expect("defaults");
        assert_eq!(cfg.server.port, 8080);
    }

    #[test]
    fn storage_artifacts_dir_from_env() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().expect("env lock");

        const KEY: &str = "COPPICE_STORAGE__ARTIFACTS_DIR";
        let previous = std::env::var(KEY).ok();
        std::env::set_var(KEY, "/tmp/coppice-test-artifacts");

        let cfg = AppConfig::load(None).expect("config should load");

        match previous {
            Some(value) => std::env::set_var(KEY, value),
            None => std::env::remove_var(KEY),
        }

        assert_eq!(cfg.storage.artifacts_dir, "/tmp/coppice-test-artifacts");
    }
}
