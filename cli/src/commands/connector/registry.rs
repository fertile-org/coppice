use std::path::{Path, PathBuf};

/// Connector IDs that match `[agent.connectors.<id>]` in config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorId {
    Mock,
    Cursor,
    ClaudeCode,
    Codex,
    KiloCode,
    OpenCode,
}

impl ConnectorId {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mock" => Some(Self::Mock),
            "cursor" => Some(Self::Cursor),
            "claude-code" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "kilo-code" => Some(Self::KiloCode),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Cursor => "cursor",
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::KiloCode => "kilo-code",
            Self::OpenCode => "opencode",
        }
    }

    /// TOML table key under `[agent.connectors.]`.
    pub fn config_key(self) -> &'static str {
        self.as_str()
    }
}

impl std::fmt::Display for ConnectorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct ConnectorMeta {
    pub id: ConnectorId,
    pub binary: &'static str,
    pub default_model_providers: &'static [&'static str],
    pub auth_hint: &'static str,
    /// Relative paths under $HOME that suggest auth is present.
    pub auth_paths: &'static [&'static str],
    /// Optional env vars that count as authenticated.
    pub auth_env: &'static [&'static str],
}

pub const CONNECTORS: &[ConnectorMeta] = &[
    ConnectorMeta {
        id: ConnectorId::Mock,
        binary: "mock",
        default_model_providers: &[],
        auth_hint: "built-in; no setup",
        auth_paths: &[],
        auth_env: &[],
    },
    ConnectorMeta {
        id: ConnectorId::Cursor,
        binary: "agent",
        default_model_providers: &["cursor"],
        auth_hint: "agent login (copy URL)",
        auth_paths: &[".config/cursor/auth.json", ".cursor/auth.json"],
        auth_env: &[],
    },
    ConnectorMeta {
        id: ConnectorId::ClaudeCode,
        binary: "claude",
        default_model_providers: &["sonnet", "opus", "haiku"],
        auth_hint: "ANTHROPIC_API_KEY or claude setup-token",
        auth_paths: &[".claude", ".config/claude"],
        auth_env: &["ANTHROPIC_API_KEY"],
    },
    ConnectorMeta {
        id: ConnectorId::Codex,
        binary: "codex",
        default_model_providers: &["openai"],
        auth_hint: "codex login --device-auth",
        auth_paths: &[".codex"],
        auth_env: &["OPENAI_API_KEY"],
    },
    ConnectorMeta {
        id: ConnectorId::KiloCode,
        binary: "kilo",
        default_model_providers: &["anthropic"],
        auth_hint: "kilo auth / TUI /connect",
        auth_paths: &[".local/share/opencode", ".kilocode"],
        auth_env: &[],
    },
    ConnectorMeta {
        id: ConnectorId::OpenCode,
        binary: "opencode",
        default_model_providers: &[],
        auth_hint: "opencode auth login",
        auth_paths: &[".local/share/opencode", ".opencode"],
        auth_env: &[],
    },
];

pub fn meta(id: ConnectorId) -> &'static ConnectorMeta {
    CONNECTORS
        .iter()
        .find(|c| c.id == id)
        .expect("connector registry incomplete")
}

pub fn parse_id(s: &str) -> anyhow::Result<ConnectorId> {
    ConnectorId::parse(s).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown connector `{s}`; expected one of: {}",
            CONNECTORS
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("/home/coppice"))
}

pub fn auth_present(meta: &ConnectorMeta, home: &Path) -> bool {
    for key in meta.auth_env {
        if std::env::var_os(key).is_some_and(|v| !v.is_empty()) {
            return true;
        }
    }
    for rel in meta.auth_paths {
        let p = home.join(rel);
        if p.exists() {
            return true;
        }
    }
    false
}

pub fn binary_on_path(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}
