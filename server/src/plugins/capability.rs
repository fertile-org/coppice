use crate::plugins::manifest::{PluginLayout, SkillEntry};
use crate::plugins::skill_walk::{walk, SKILL_CONTAINERS};
use crate::plugins::skills::parse_skill_file;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path};

pub(crate) const PLUGIN_JSON: &str = ".claude-plugin/plugin.json";
pub(crate) const MARKETPLACE_JSON: &str = ".claude-plugin/marketplace.json";
const MCP_JSON: &str = ".mcp.json";
const SKILL_MD: &str = "SKILL.md";
const ESCAPES_ROOT: &str = "path escapes plugin root";
const DUPLICATE_NAME: &str = "duplicate skill name";
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
        if layout == PluginLayout::SkillsOnly && root.join(SKILL_MD).is_file() {
            let name = root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let skills = load_skill(root, root, name, String::new());
            return CapabilityOutcome::Supported(skills.into_iter().collect());
        }
        let containers: Vec<(String, usize)> = match layout {
            PluginLayout::SkillsOnly => SKILL_CONTAINERS
                .iter()
                .map(|c| (c.to_string(), 3))
                .chain([(String::new(), 1)])
                .collect(),
            PluginLayout::Plugin => match plugin_json.and_then(|j| j.get(Self::KEY)) {
                None => vec![("skills".into(), 3)],
                Some(Value::String(dir)) => vec![(dir.clone(), 3)],
                Some(Value::Array(dirs)) => dirs
                    .iter()
                    .filter_map(|d| d.as_str().map(|d| (d.to_string(), 3)))
                    .collect(),
                Some(_) => Vec::new(),
            },
        };
        let mut skills: Vec<_> = containers
            .iter()
            .flat_map(|(dir, depth)| skills_in(root, dir, *depth))
            .collect();
        skills.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        let mut seen = HashSet::new();
        for skill in &mut skills {
            if !seen.insert(skill.name.clone()) && skill.error.is_none() {
                skill.error = Some(DUPLICATE_NAME.into());
            }
        }
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

/// Skills are dirs with `SKILL.md` up to `max_depth` levels below
/// `<root>/<rel_dir>`; `root` must be canonical.
fn skills_in(root: &Path, rel_dir: &str, max_depth: usize) -> Vec<SkillEntry> {
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
    let walk = walk(root, rel_dir, max_depth);
    let skills = walk.skills.into_iter().filter_map(|rel_path| {
        load_skill(
            root,
            &root.join(&rel_path),
            last_segment(&rel_path),
            rel_path,
        )
    });
    let escapes = walk
        .escaped
        .into_iter()
        .map(|rel_path| escaped(&last_segment(&rel_path), &rel_path));
    skills.chain(escapes).collect()
}

fn last_segment(rel_path: &str) -> String {
    rel_path.rsplit('/').next().unwrap_or_default().to_string()
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

    fn fixture(name: &str) -> std::path::PathBuf {
        std::fs::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../fixtures/plugins")
                .join(name),
        )
        .unwrap()
    }

    fn skills(root: &Path, plugin_json: Option<&Value>, layout: PluginLayout) -> Vec<SkillEntry> {
        let root = std::fs::canonicalize(root).unwrap();
        match SkillsCapability::parse(&root, plugin_json, layout) {
            CapabilityOutcome::Supported(skills) => skills,
            _ => panic!("expected supported skills"),
        }
    }

    fn write_skill(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: About {name}\n---\nbody"),
        )
        .unwrap();
    }

    #[test]
    fn skills_package_single_root_skill() {
        let skills = skills(&fixture("single-skill"), None, PluginLayout::SkillsOnly);
        assert_eq!(
            skills,
            vec![SkillEntry {
                name: "single-skill".into(),
                description: "A repository that is one skill".into(),
                rel_path: String::new(),
                error: None,
            }]
        );
    }

    #[test]
    fn skills_package_catalog_and_agent_dirs() {
        let skills = skills(&fixture("skills-catalog"), None, PluginLayout::SkillsOnly);
        let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["agent-one", "design", "pdf"]);
        let pdf = &skills[2];
        assert_eq!(pdf.rel_path, "skills/docs/pdf");
        assert_eq!(pdf.description, "Fill PDF forms. Use for PDFs.");
        assert_eq!(skills[0].rel_path, ".agents/skills/agent-one");
        assert!(skills.iter().all(|s| s.error.is_none()), "{skills:?}");
    }

    #[test]
    fn duplicate_skill_names_marked() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(&dir.path().join("skills/a/x"), "x");
        write_skill(&dir.path().join(".claude/skills/x"), "x");
        let skills = skills(dir.path(), None, PluginLayout::SkillsOnly);
        assert_eq!(skills.len(), 2, "{skills:?}");
        let by_path = |p: &str| skills.iter().find(|s| s.rel_path == p).unwrap();
        assert_eq!(by_path(".claude/skills/x").error, None);
        assert_eq!(
            by_path("skills/a/x").error.as_deref(),
            Some("duplicate skill name")
        );
    }

    #[test]
    fn plugin_layout_skills_use_walker() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(&dir.path().join("skills/cat/s"), "s");
        let plugin_json = serde_json::json!({"name": "p"});
        let skills = skills(dir.path(), Some(&plugin_json), PluginLayout::Plugin);
        assert_eq!(skills.len(), 1, "{skills:?}");
        assert_eq!(skills[0].name, "s");
        assert_eq!(skills[0].rel_path, "skills/cat/s");
        assert_eq!(skills[0].error, None);
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
