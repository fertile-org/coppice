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

impl AppConfig {
    pub fn load(config_path: Option<&str>) -> Result<Self, figment::Error> {
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
            );

        if let Some(path) = config_path {
            figment = figment.merge(Yaml::file(path));
        }

        figment.extract()
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
}
