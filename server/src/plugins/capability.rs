use crate::plugins::manifest::{PluginLayout, SkillEntry};
use crate::plugins::skills::parse_skill_file;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub(crate) const PLUGIN_JSON: &str = ".claude-plugin/plugin.json";
const MCP_JSON: &str = ".mcp.json";
const ESCAPES_ROOT: &str = "path escapes plugin root";
const NOT_SUPPORTED_YET: &str = "not supported yet";

pub enum CapabilityOutcome<T> {
    Absent,
    Supported(T),
    Unsupported(String),
    /// The capability is malformed badly enough that the whole plugin is invalid.
    Invalid(String),
}

pub trait CapabilityParser {
    type Output;
    const KEY: &'static str;
    /// `root` must be canonical.
    fn parse(
        root: &Path,
        plugin_json: Option<&Value>,
        layout: PluginLayout,
    ) -> CapabilityOutcome<Self::Output>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    pub name: String,
    #[serde(default = "unknown_transport")]
    pub transport: McpServerTransport,
    #[serde(default)]
    pub error: Option<String>,
}

impl McpServerEntry {
    pub fn api_kind(&self) -> &str {
        match &self.transport {
            McpServerTransport::Stdio { .. } => "stdio",
            McpServerTransport::Http { .. } => "http",
            McpServerTransport::Unsupported { kind } => kind,
        }
    }
}

/// Values keep their `${…}` placeholders verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum McpServerTransport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
    },
    Unsupported {
        kind: String,
    },
}

fn unknown_transport() -> McpServerTransport {
    McpServerTransport::Unsupported {
        kind: "unknown".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "RawUnsupportedPart")]
pub struct UnsupportedPart {
    pub key: String,
    pub reason: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawUnsupportedPart {
    Key(String),
    Part { key: String, reason: String },
}

impl From<RawUnsupportedPart> for UnsupportedPart {
    fn from(raw: RawUnsupportedPart) -> Self {
        match raw {
            RawUnsupportedPart::Key(key) => UnsupportedPart {
                key,
                reason: NOT_SUPPORTED_YET.into(),
            },
            RawUnsupportedPart::Part { key, reason } => UnsupportedPart { key, reason },
        }
    }
}

pub struct SkillsCapability;

impl CapabilityParser for SkillsCapability {
    type Output = Vec<SkillEntry>;
    const KEY: &'static str = "skills";

    fn parse(
        root: &Path,
        plugin_json: Option<&Value>,
        layout: PluginLayout,
    ) -> CapabilityOutcome<Vec<SkillEntry>> {
        let skill_dirs = match layout {
            PluginLayout::SkillsOnly if root.join("skills").is_dir() => vec!["skills".into()],
            PluginLayout::SkillsOnly => vec![String::new()],
            PluginLayout::Plugin => match plugin_json.and_then(|j| j.get(Self::KEY)) {
                None => vec!["skills".into()],
                Some(Value::String(dir)) => vec![dir.clone()],
                Some(Value::Array(dirs)) => dirs
                    .iter()
                    .filter_map(|d| d.as_str().map(str::to_string))
                    .collect(),
                Some(_) => Vec::new(),
            },
        };
        let mut skills: Vec<_> = skill_dirs
            .iter()
            .flat_map(|dir| skills_in(root, dir))
            .collect();
        skills.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.rel_path.cmp(&b.rel_path))
        });
        CapabilityOutcome::Supported(skills)
    }
}

pub struct McpServersCapability;

impl CapabilityParser for McpServersCapability {
    type Output = Vec<McpServerEntry>;
    const KEY: &'static str = "mcpServers";

    fn parse(
        root: &Path,
        plugin_json: Option<&Value>,
        layout: PluginLayout,
    ) -> CapabilityOutcome<Vec<McpServerEntry>> {
        if layout == PluginLayout::SkillsOnly {
            return CapabilityOutcome::Absent;
        }
        let path = root.join(MCP_JSON);
        let (source, file_value);
        let servers = if path.is_file() {
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(e) => return CapabilityOutcome::Invalid(format!("read {MCP_JSON}: {e}")),
            };
            file_value = match serde_json::from_str::<Value>(&text) {
                Ok(value) => value,
                Err(e) => return CapabilityOutcome::Invalid(format!("invalid {MCP_JSON}: {e}")),
            };
            source = MCP_JSON;
            file_value.get(Self::KEY)
        } else {
            source = PLUGIN_JSON;
            plugin_json.and_then(|j| j.get(Self::KEY))
        };
        let Some(servers) = servers else {
            return CapabilityOutcome::Absent;
        };
        let Some(servers) = servers.as_object() else {
            return CapabilityOutcome::Invalid(format!(
                "{source}: `{}` must be an object",
                Self::KEY
            ));
        };
        let mut entries: Vec<_> = servers
            .iter()
            .map(|(name, config)| mcp_entry(name, config))
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        CapabilityOutcome::Supported(entries)
    }
}

fn mcp_entry(name: &str, config: &Value) -> McpServerEntry {
    let result = match config.as_object() {
        Some(config) => mcp_transport(config),
        None => Err("entry must be an object".into()),
    };
    let (transport, error) = match result {
        Ok(transport) => (transport, None),
        Err(reason) => (unknown_transport(), Some(reason)),
    };
    McpServerEntry {
        name: name.to_string(),
        transport,
        error,
    }
}

fn mcp_transport(config: &Map<String, Value>) -> Result<McpServerTransport, String> {
    let kind = match config.get("type") {
        None => None,
        Some(Value::String(kind)) => Some(kind.as_str()),
        Some(_) => return Err("`type` must be a string".into()),
    };
    let is_stdio = match kind {
        Some("stdio") => true,
        Some("http") => false,
        Some(other) if other.trim().is_empty() => return Ok(unknown_transport()),
        Some(other) => {
            return Ok(McpServerTransport::Unsupported { kind: other.into() });
        }
        None if config.contains_key("command") => true,
        None if config.contains_key("url") => false,
        None => return Ok(unknown_transport()),
    };
    if is_stdio {
        Ok(McpServerTransport::Stdio {
            command: string_field(config, "command")?,
            args: match config.get("args") {
                None => Vec::new(),
                Some(args) => args
                    .as_array()
                    .and_then(|args| {
                        args.iter()
                            .map(|a| a.as_str().map(str::to_string))
                            .collect()
                    })
                    .ok_or("`args` must be an array of strings")?,
            },
            env: string_map(config, "env")?,
        })
    } else {
        Ok(McpServerTransport::Http {
            url: string_field(config, "url")?,
            headers: string_map(config, "headers")?,
        })
    }
}

fn string_field(config: &Map<String, Value>, key: &str) -> Result<String, String> {
    config
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("`{key}` must be a string"))
}

fn string_map(config: &Map<String, Value>, key: &str) -> Result<BTreeMap<String, String>, String> {
    let Some(value) = config.get(key) else {
        return Ok(BTreeMap::new());
    };
    value
        .as_object()
        .and_then(|map| {
            map.iter()
                .map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect()
        })
        .ok_or_else(|| format!("`{key}` must be an object of strings"))
}

/// Reports `Unsupported` when `<root>/<KEY>` exists in a plugin layout.
fn unsupported_dir(root: &Path, key: &str, layout: PluginLayout) -> CapabilityOutcome<()> {
    if layout == PluginLayout::Plugin && root.join(key).exists() {
        CapabilityOutcome::Unsupported(NOT_SUPPORTED_YET.into())
    } else {
        CapabilityOutcome::Absent
    }
}

macro_rules! unsupported_dir_capability {
    ($name:ident, $key:literal) => {
        pub struct $name;

        impl CapabilityParser for $name {
            type Output = ();
            const KEY: &'static str = $key;

            fn parse(
                root: &Path,
                _plugin_json: Option<&Value>,
                layout: PluginLayout,
            ) -> CapabilityOutcome<()> {
                unsupported_dir(root, Self::KEY, layout)
            }
        }
    };
}

unsupported_dir_capability!(CommandsCapability, "commands");
unsupported_dir_capability!(AgentsCapability, "agents");
unsupported_dir_capability!(HooksCapability, "hooks");

fn escaped(name: &str, rel_path: &str) -> SkillEntry {
    SkillEntry {
        name: name.to_string(),
        description: String::new(),
        rel_path: rel_path.to_string(),
        error: Some(ESCAPES_ROOT.into()),
    }
}

/// Skills are `<root>/<rel_dir>/<name>/SKILL.md`; `root` must be canonical.
fn skills_in(root: &Path, rel_dir: &str) -> Vec<SkillEntry> {
    let rel_dir = rel_dir.trim_end_matches('/');
    let dir = root.join(rel_dir);
    let canonical = match std::fs::canonicalize(&dir) {
        Ok(canonical) => canonical,
        Err(_)
            if Path::new(rel_dir).is_absolute()
                || Path::new(rel_dir)
                    .components()
                    .any(|c| c == Component::ParentDir) =>
        {
            return vec![escaped(rel_dir, rel_dir)];
        }
        Err(_) => return Vec::new(),
    };
    if !canonical.starts_with(root) {
        return vec![escaped(rel_dir, rel_dir)];
    }
    let Ok(entries) = std::fs::read_dir(&canonical) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let rel_path = if rel_dir.is_empty() {
                name.clone()
            } else {
                format!("{rel_dir}/{name}")
            };
            load_skill(root, &canonical.join(&name), name, rel_path)
        })
        .collect()
}

fn load_skill(root: &Path, dir: &Path, name: String, rel_path: String) -> Option<SkillEntry> {
    let canonical = std::fs::canonicalize(dir).ok()?;
    if !canonical.is_dir() {
        return None;
    }
    if !canonical.starts_with(root) {
        return Some(escaped(&name, &rel_path));
    }
    let skill_file = std::fs::canonicalize(canonical.join("SKILL.md")).ok()?;
    if !skill_file.starts_with(root) {
        return Some(escaped(&name, &rel_path));
    }
    let parsed = std::fs::read_to_string(&skill_file)
        .map_err(anyhow::Error::from)
        .and_then(|text| parse_skill_file(&text));
    let (description, error) = match parsed {
        Ok((frontmatter, _)) => (frontmatter.description, None),
        Err(err) => (String::new(), Some(format!("{err:#}"))),
    };
    Some(SkillEntry {
        name,
        description,
        rel_path,
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_mcp(mcp_json: &str) -> Vec<McpServerEntry> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MCP_JSON), mcp_json).unwrap();
        match McpServersCapability::parse(dir.path(), None, PluginLayout::Plugin) {
            CapabilityOutcome::Supported(entries) => entries,
            _ => panic!("expected supported mcpServers"),
        }
    }

    fn transport(mcp_json: &str) -> McpServerTransport {
        let entries = parse_mcp(mcp_json);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].error, None);
        entries[0].transport.clone()
    }

    #[test]
    fn mcp_stdio_entry_keeps_full_spec() {
        let t = transport(
            r#"{"mcpServers":{"fs":{"command":"${CLAUDE_PLUGIN_ROOT}/bin/fs","args":["--root","${ROOT}"],"env":{"TOKEN":"${API_TOKEN}"}}}}"#,
        );
        assert_eq!(
            t,
            McpServerTransport::Stdio {
                command: "${CLAUDE_PLUGIN_ROOT}/bin/fs".into(),
                args: vec!["--root".into(), "${ROOT}".into()],
                env: BTreeMap::from([("TOKEN".into(), "${API_TOKEN}".into())]),
            }
        );
    }

    #[test]
    fn mcp_http_entry_keeps_url_and_headers() {
        let t = transport(
            r#"{"mcpServers":{"x":{"type":"http","url":"https://x/mcp","headers":{"Authorization":"Bearer ${KEY}"}}}}"#,
        );
        assert_eq!(
            t,
            McpServerTransport::Http {
                url: "https://x/mcp".into(),
                headers: BTreeMap::from([("Authorization".into(), "Bearer ${KEY}".into())]),
            }
        );
        assert_eq!(
            transport(r#"{"mcpServers":{"x":{"url":"https://x/mcp"}}}"#),
            McpServerTransport::Http {
                url: "https://x/mcp".into(),
                headers: BTreeMap::new(),
            }
        );
    }

    #[test]
    fn mcp_sse_and_unknown_are_unsupported() {
        let t = transport(r#"{"mcpServers":{"x":{"type":"sse","url":"https://x/sse"}}}"#);
        assert_eq!(t, McpServerTransport::Unsupported { kind: "sse".into() });
        let t = transport(r#"{"mcpServers":{"x":{"description":"nothing"}}}"#);
        assert_eq!(
            t,
            McpServerTransport::Unsupported {
                kind: "unknown".into()
            }
        );
    }

    #[test]
    fn mcp_blank_type_is_unknown() {
        for ty in ["", "   "] {
            let t = transport(&format!(
                r#"{{"mcpServers":{{"x":{{"type":"{ty}","command":"x"}}}}}}"#
            ));
            assert_eq!(t, unknown_transport(), "type {ty:?}");
        }
    }

    #[test]
    fn mcp_malformed_entries_keep_plugin_valid() {
        let entries = parse_mcp(
            r#"{"mcpServers":{
                "a":"nope",
                "b":{"command":7},
                "c":{"command":"x","args":[1]},
                "d":{"command":"x","env":{"K":1}},
                "e":{"type":"http","headers":{}},
                "f":{"url":"https://x","headers":{"H":true}}
            }}"#,
        );
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c", "d", "e", "f"]);
        for entry in &entries {
            assert_eq!(entry.transport, unknown_transport(), "{entry:?}");
            assert_eq!(entry.api_kind(), "unknown");
            assert!(entry.error.is_some(), "{entry:?}");
        }
        assert_eq!(
            entries[1].error.as_deref(),
            Some("`command` must be a string")
        );
    }

    #[test]
    fn mcp_absent_without_config() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            McpServersCapability::parse(dir.path(), None, PluginLayout::Plugin),
            CapabilityOutcome::Absent
        ));
    }

    #[test]
    fn unsupported_dir_only_in_plugin_layout() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("hooks")).unwrap();
        assert!(matches!(
            HooksCapability::parse(dir.path(), None, PluginLayout::Plugin),
            CapabilityOutcome::Unsupported(reason) if reason == NOT_SUPPORTED_YET
        ));
        assert!(matches!(
            HooksCapability::parse(dir.path(), None, PluginLayout::SkillsOnly),
            CapabilityOutcome::Absent
        ));
        assert!(matches!(
            AgentsCapability::parse(dir.path(), None, PluginLayout::Plugin),
            CapabilityOutcome::Absent
        ));
    }
}
