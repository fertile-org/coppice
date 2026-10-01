use crate::plugins::builtin::BUILTIN_PLUGIN;
use crate::plugins::capability::{
    AgentsCapability, CapabilityOutcome, CapabilityParser, CommandsCapability, HooksCapability,
    McpServersCapability, SkillsCapability, PLUGIN_JSON,
};
pub use crate::plugins::capability::{McpServerEntry, McpServerTransport, UnsupportedPart};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub layout: PluginLayout,
    pub skills: Vec<SkillEntry>,
    pub mcp_servers: Vec<McpServerEntry>,
    pub unsupported: Vec<UnsupportedPart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginLayout {
    Plugin,
    SkillsOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub rel_path: String,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct RawPluginJson {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    author: Option<RawAuthor>,
    /// Type-checked here so a bad `skills` value invalidates the plugin;
    /// `SkillsCapability` reads it from the raw JSON.
    #[serde(rename = "skills")]
    _skills: Option<OneOrMany>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawAuthor {
    Name(String),
    Object { name: Option<String> },
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

pub fn is_plugin_root(root: &Path) -> bool {
    root.join(PLUGIN_JSON).is_file()
        || root.join("skills").is_dir()
        || std::fs::read_dir(root).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|e| e.path().join("SKILL.md").is_file())
        })
}

/// `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`
pub fn is_valid_plugin_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    name.len() <= 64
        && first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub fn parse_plugin(root: &Path) -> Result<PluginManifest, String> {
    let root =
        std::fs::canonicalize(root).map_err(|e| format!("plugin root {}: {e}", root.display()))?;
    let manifest_path = root.join(PLUGIN_JSON);
    let (mut manifest, plugin_json) = if !manifest_path.is_file() {
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        check_name(&name)?;
        let manifest = PluginManifest {
            name,
            version: "0.0.0".into(),
            description: String::new(),
            author: None,
            layout: PluginLayout::SkillsOnly,
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            unsupported: Vec::new(),
        };
        (manifest, None)
    } else {
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("read {PLUGIN_JSON}: {e}"))?;
        let raw: RawPluginJson =
            serde_json::from_str(&text).map_err(|e| format!("invalid {PLUGIN_JSON}: {e}"))?;
        let name = raw
            .name
            .filter(|n| !n.is_empty())
            .ok_or_else(|| format!("{PLUGIN_JSON}: `name` is required"))?;
        check_name(&name)?;
        let manifest = PluginManifest {
            name,
            version: raw.version.unwrap_or_else(|| "0.0.0".into()),
            description: raw.description.unwrap_or_default(),
            author: match raw.author {
                Some(RawAuthor::Name(name)) => Some(name),
                Some(RawAuthor::Object { name }) => name,
                None => None,
            }
            .filter(|a| !a.is_empty()),
            layout: PluginLayout::Plugin,
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            unsupported: Vec::new(),
        };
        let value: Value =
            serde_json::from_str(&text).map_err(|e| format!("invalid {PLUGIN_JSON}: {e}"))?;
        (manifest, Some(value))
    };

    let mut ctx = Capabilities {
        root: &root,
        plugin_json: plugin_json.as_ref(),
        layout: manifest.layout,
        unsupported: &mut manifest.unsupported,
    };
    let skills = ctx.parse::<SkillsCapability>()?;
    let mcp_servers = ctx.parse::<McpServersCapability>()?;
    ctx.parse::<AgentsCapability>()?;
    ctx.parse::<CommandsCapability>()?;
    ctx.parse::<HooksCapability>()?;
    manifest.skills = skills.unwrap_or_default();
    manifest.mcp_servers = mcp_servers.unwrap_or_default();
    Ok(manifest)
}

struct Capabilities<'a> {
    root: &'a Path,
    plugin_json: Option<&'a Value>,
    layout: PluginLayout,
    unsupported: &'a mut Vec<UnsupportedPart>,
}

impl Capabilities<'_> {
    fn parse<P: CapabilityParser>(&mut self) -> Result<Option<P::Output>, String> {
        match P::parse(self.root, self.plugin_json, self.layout) {
            CapabilityOutcome::Absent => Ok(None),
            CapabilityOutcome::Supported(output) => Ok(Some(output)),
            CapabilityOutcome::Unsupported(reason) => {
                self.unsupported.push(UnsupportedPart {
                    key: P::KEY.into(),
                    reason,
                });
                Ok(None)
            }
            CapabilityOutcome::Invalid(reason) => Err(reason),
        }
    }
}

fn check_name(name: &str) -> Result<(), String> {
    if name == BUILTIN_PLUGIN {
        Err(format!("plugin name `{name}` is reserved"))
    } else if !is_valid_plugin_name(name) {
        Err(format!("invalid plugin name `{name}`"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/plugins")
            .join(name)
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn skill<'a>(m: &'a PluginManifest, name: &str) -> &'a SkillEntry {
        m.skills
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("missing skill {name}: {:?}", m.skills))
    }

    #[test]
    fn parses_claude_plugin_layout() {
        let root = fixture("sample-plugin");
        assert!(is_plugin_root(&root));
        let m = parse_plugin(&root).unwrap();
        assert_eq!(m.name, "sample-plugin");
        assert_eq!(m.version, "1.2.0");
        assert_eq!(m.description, "Sample");
        assert_eq!(m.author.as_deref(), Some("Coppice"));
        assert_eq!(m.layout, PluginLayout::Plugin);
        assert_eq!(m.skills.len(), 2);
        let hello = skill(&m, "hello");
        assert_eq!(hello.error, None);
        assert_eq!(hello.rel_path, "skills/hello");
        assert_eq!(hello.description, "Says hello");
        assert!(skill(&m, "broken").error.is_some());
        assert_eq!(unsupported_keys(&m), vec!["commands", "hooks"]);
        let kind = |n: &str| {
            m.mcp_servers
                .iter()
                .find(|s| s.name == n)
                .unwrap()
                .api_kind()
                .to_string()
        };
        assert_eq!(m.mcp_servers.len(), 3);
        assert_eq!(kind("echo"), "stdio");
        assert_eq!(kind("remote"), "http");
        assert_eq!(kind("old"), "sse");
        let names: Vec<_> = m.mcp_servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["echo", "old", "remote"]);

        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["layout"], "plugin");
        assert!(json["mcpServers"].is_array());
        assert_eq!(serde_json::from_value::<PluginManifest>(json).unwrap(), m);
    }

    #[test]
    fn parses_skills_only_folder() {
        let root = fixture("skills-only");
        assert!(is_plugin_root(&root));
        let m = parse_plugin(&root).unwrap();
        assert_eq!(m.name, "skills-only");
        assert_eq!(m.version, "0.0.0");
        assert_eq!(m.description, "");
        assert_eq!(m.author, None);
        assert_eq!(m.layout, PluginLayout::SkillsOnly);
        assert_eq!(m.skills.len(), 1);
        let review = skill(&m, "review");
        assert_eq!(review.error, None);
        assert_eq!(review.rel_path, "review");
        assert_eq!(review.description, "Reviews code");
        assert!(m.unsupported.is_empty());
        assert!(m.mcp_servers.is_empty());
        assert_eq!(serde_json::to_value(&m).unwrap()["layout"], "skillsOnly");
    }

    #[test]
    fn parses_superpowers_shaped_plugin() {
        let m = parse_plugin(&fixture("superpowers-like")).unwrap();
        assert_eq!(m.name, "superpowers-like");
        assert_eq!(m.author.as_deref(), Some("Coppice"));
        assert_eq!(m.skills.len(), 2);
        assert!(m.skills.iter().all(|s| s.error.is_none()), "{:?}", m.skills);
        skill(&m, "brainstorming");
        skill(&m, "writing-plans");
        assert_eq!(unsupported_keys(&m), vec!["agents"]);
    }

    fn unsupported_keys(m: &PluginManifest) -> Vec<&str> {
        m.unsupported.iter().map(|u| u.key.as_str()).collect()
    }

    #[test]
    fn inline_plugin_json_mcp_servers() {
        let m = parse_plugin(&fixture("inline-mcp")).unwrap();
        assert_eq!(m.name, "inline-mcp");
        assert_eq!(
            m.mcp_servers,
            vec![McpServerEntry {
                name: "fs".into(),
                transport: McpServerTransport::Stdio {
                    command: "${CLAUDE_PLUGIN_ROOT}/bin/fs".into(),
                    args: vec!["--root".into(), "${ROOT}".into()],
                    env: BTreeMap::from([("TOKEN".into(), "${API_TOKEN}".into())]),
                },
                error: None,
            }]
        );
    }

    #[test]
    fn mcp_json_wins_over_inline_mcp_servers() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join(".claude-plugin/plugin.json"),
            r#"{"name":"p","mcpServers":{"inline":{"command":"a"}}}"#,
        );
        write(
            &dir.path().join(".mcp.json"),
            r#"{"mcpServers":{"file":{"command":"b"}}}"#,
        );
        let m = parse_plugin(dir.path()).unwrap();
        let names: Vec<_> = m.mcp_servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["file"]);
    }

    #[test]
    fn malformed_mcp_json_marks_plugin_invalid() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join(".claude-plugin/plugin.json"),
            r#"{"name":"p"}"#,
        );
        write(&dir.path().join(".mcp.json"), "{");
        let err = parse_plugin(dir.path()).unwrap_err();
        assert!(err.starts_with("invalid .mcp.json: "), "{err}");

        write(&dir.path().join(".mcp.json"), r#"{"mcpServers":[]}"#);
        let err = parse_plugin(dir.path()).unwrap_err();
        assert_eq!(err, ".mcp.json: `mcpServers` must be an object");

        std::fs::remove_file(dir.path().join(".mcp.json")).unwrap();
        write(
            &dir.path().join(".claude-plugin/plugin.json"),
            r#"{"name":"p","mcpServers":"nope"}"#,
        );
        let err = parse_plugin(dir.path()).unwrap_err();
        assert_eq!(
            err,
            ".claude-plugin/plugin.json: `mcpServers` must be an object"
        );
    }

    #[test]
    fn unsupported_dirs_reported() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join(".claude-plugin/plugin.json"),
            r#"{"name":"p"}"#,
        );
        for d in ["hooks", "commands", "agents"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        let m = parse_plugin(dir.path()).unwrap();
        let reason = "not supported yet".to_string();
        assert_eq!(
            m.unsupported,
            ["agents", "commands", "hooks"]
                .map(|key| UnsupportedPart {
                    key: key.into(),
                    reason: reason.clone(),
                })
                .to_vec()
        );
    }

    #[test]
    fn old_manifest_json_deserializes() {
        let old = serde_json::json!({
            "name": "p",
            "version": "1.0.0",
            "description": "",
            "author": null,
            "layout": "plugin",
            "skills": [{"name": "hello", "description": "Hi", "relPath": "skills/hello", "error": null}],
            "mcpServers": [{"name": "fs", "kind": "stdio"}],
            "unsupported": ["commands"],
        });
        let m: PluginManifest = serde_json::from_value(old).unwrap();
        assert_eq!(m.skills.len(), 1);
        assert_eq!(m.skills[0].rel_path, "skills/hello");
        assert_eq!(m.mcp_servers[0].name, "fs");
        assert_eq!(
            m.mcp_servers[0].transport,
            McpServerTransport::Unsupported {
                kind: "unknown".into()
            }
        );
        assert_eq!(m.mcp_servers[0].error, None);
        assert_eq!(m.unsupported[0].key, "commands");
        assert_eq!(m.unsupported[0].reason, "not supported yet");
    }

    #[test]
    fn new_manifest_json_round_trips() {
        let m = parse_plugin(&fixture("inline-mcp")).unwrap();
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["mcpServers"][0]["transport"]["type"], "stdio");
        assert_eq!(serde_json::from_value::<PluginManifest>(json).unwrap(), m);
    }

    #[test]
    fn invalid_plugin_json_is_plugin_level_error() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join(".claude-plugin/plugin.json"), "{");
        let err = parse_plugin(dir.path()).unwrap_err();
        assert!(err.contains("plugin.json"), "{err}");
    }

    #[test]
    fn reserved_or_bad_name_is_invalid() {
        for name in ["coppice", "bad name"] {
            let dir = tempfile::tempdir().unwrap();
            write(
                &dir.path().join(".claude-plugin/plugin.json"),
                &serde_json::json!({ "name": name }).to_string(),
            );
            assert!(parse_plugin(dir.path()).is_err(), "{name}");
        }
        assert!(is_valid_plugin_name("superpowers"));
        assert!(is_valid_plugin_name("a.b_c-9"));
        assert!(!is_valid_plugin_name(""));
        assert!(!is_valid_plugin_name("-lead"));
        assert!(!is_valid_plugin_name(&"a".repeat(65)));
        assert!(is_valid_plugin_name(&"a".repeat(64)));
    }

    #[test]
    fn skills_override_outside_root_is_invalid() {
        let outer = tempfile::tempdir().unwrap();
        write(
            &outer.path().join("outside/leak/SKILL.md"),
            "---\nname: leak\ndescription: Leaked\n---\nbody",
        );
        let root = outer.path().join("plugin");
        write(
            &root.join(".claude-plugin/plugin.json"),
            r#"{"name":"p","skills":["../../etc","../outside"]}"#,
        );
        let m = parse_plugin(&root).unwrap();
        assert_eq!(m.skills.len(), 2, "{:?}", m.skills);
        for s in &m.skills {
            assert_eq!(
                s.error.as_deref(),
                Some("path escapes plugin root"),
                "{s:?}"
            );
        }
        assert!(m.skills.iter().all(|s| s.name != "leak"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_skill_outside_root_is_invalid() {
        let outer = tempfile::tempdir().unwrap();
        let target = outer.path().join("outside/evil");
        write(
            &target.join("SKILL.md"),
            "---\nname: evil\ndescription: Evil\n---\nbody",
        );
        let root = outer.path().join("plugin");
        write(&root.join(".claude-plugin/plugin.json"), r#"{"name":"p"}"#);
        std::fs::create_dir_all(root.join("skills")).unwrap();
        std::os::unix::fs::symlink(&target, root.join("skills/evil")).unwrap();
        let m = parse_plugin(&root).unwrap();
        let evil = skill(&m, "evil");
        assert_eq!(evil.error.as_deref(), Some("path escapes plugin root"));
        assert_eq!(evil.description, "");
    }
}
