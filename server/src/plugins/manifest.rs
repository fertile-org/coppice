use crate::plugins::builtin::BUILTIN_PLUGIN;
use crate::plugins::skills::parse_skill_file;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Component, Path};

const PLUGIN_JSON: &str = ".claude-plugin/plugin.json";
const MCP_JSON: &str = ".mcp.json";
const ESCAPES_ROOT: &str = "path escapes plugin root";
/// Kept sorted: `PluginManifest::unsupported` preserves this order.
const UNSUPPORTED_DIRS: [&str; 3] = ["agents", "commands", "hooks"];

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
    pub unsupported: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    pub name: String,
    pub kind: McpServerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpServerKind {
    Stdio,
    Http,
    Sse,
    Unknown,
}

#[derive(Deserialize)]
struct RawPluginJson {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    author: Option<RawAuthor>,
    skills: Option<OneOrMany>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawAuthor {
    Name(String),
    Object { name: Option<String> },
}

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
    if !manifest_path.is_file() {
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        check_name(&name)?;
        let skills_dir = if root.join("skills").is_dir() {
            "skills"
        } else {
            ""
        };
        return Ok(PluginManifest {
            name,
            version: "0.0.0".into(),
            description: String::new(),
            author: None,
            layout: PluginLayout::SkillsOnly,
            skills: sorted_skills(skills_in(&root, skills_dir)),
            mcp_servers: Vec::new(),
            unsupported: Vec::new(),
        });
    }

    let text =
        std::fs::read_to_string(&manifest_path).map_err(|e| format!("read {PLUGIN_JSON}: {e}"))?;
    let raw: RawPluginJson =
        serde_json::from_str(&text).map_err(|e| format!("invalid {PLUGIN_JSON}: {e}"))?;
    let name = raw
        .name
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("{PLUGIN_JSON}: `name` is required"))?;
    check_name(&name)?;
    let skill_dirs = match raw.skills {
        None => vec!["skills".to_string()],
        Some(OneOrMany::One(dir)) => vec![dir],
        Some(OneOrMany::Many(dirs)) => dirs,
    };
    let skills = skill_dirs
        .iter()
        .flat_map(|dir| skills_in(&root, dir))
        .collect();
    Ok(PluginManifest {
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
        skills: sorted_skills(skills),
        mcp_servers: mcp_servers(&root)?,
        unsupported: UNSUPPORTED_DIRS
            .iter()
            .filter(|d| root.join(d).exists())
            .map(|d| d.to_string())
            .collect(),
    })
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

fn sorted_skills(mut skills: Vec<SkillEntry>) -> Vec<SkillEntry> {
    skills.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });
    skills
}

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

fn mcp_servers(root: &Path) -> Result<Vec<McpServerEntry>, String> {
    let path = root.join(MCP_JSON);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read {MCP_JSON}: {e}"))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("invalid {MCP_JSON}: {e}"))?;
    let Some(servers) = value.get("mcpServers") else {
        return Ok(Vec::new());
    };
    let servers = servers
        .as_object()
        .ok_or_else(|| format!("{MCP_JSON}: `mcpServers` must be an object"))?;
    let mut entries: Vec<_> = servers
        .iter()
        .map(|(name, config)| McpServerEntry {
            name: name.clone(),
            kind: mcp_kind(config),
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn mcp_kind(config: &Value) -> McpServerKind {
    if config.get("command").is_some() {
        return McpServerKind::Stdio;
    }
    match config.get("type").map(Value::as_str) {
        Some(Some("http")) => McpServerKind::Http,
        Some(Some("sse")) => McpServerKind::Sse,
        None if config.get("url").is_some() => McpServerKind::Http,
        _ => McpServerKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(m.unsupported, vec!["commands", "hooks"]);
        let kind = |n: &str| m.mcp_servers.iter().find(|s| s.name == n).unwrap().kind;
        assert_eq!(m.mcp_servers.len(), 3);
        assert_eq!(kind("echo"), McpServerKind::Stdio);
        assert_eq!(kind("remote"), McpServerKind::Http);
        assert_eq!(kind("old"), McpServerKind::Sse);

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
        assert_eq!(m.unsupported, vec!["agents"]);
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
