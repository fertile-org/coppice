use crate::plugins::builtin::BUILTIN_PLUGIN;
use crate::plugins::capability::{MARKETPLACE_JSON, PLUGIN_JSON};
use crate::plugins::discover::Discovered;
use crate::plugins::manifest::{
    is_valid_plugin_name, parse_plugin, ExternalSource, MarketplaceRef, PluginLayout,
    PluginManifest,
};
use crate::plugins::skill_walk::has_skills_package;
use serde_json::{Map, Value};
use std::path::{Component, Path};

const ESCAPES_REPO: &str = "source must stay inside the repository";

/// One row per `plugins` entry of `<folder>/.claude-plugin/marketplace.json`.
/// Rows are named after their entry; entries are never expanded as marketplaces.
pub fn expand_marketplace(folder: &Path, folder_rel: &str) -> Vec<Discovered> {
    let (market, entries) = match read_marketplace(folder) {
        Ok(parsed) => parsed,
        Err(reason) => {
            return vec![Discovered {
                rel_path: folder_rel.to_string(),
                name: folder_name(folder),
                result: Err(format!("invalid marketplace.json: {reason}")),
            }]
        }
    };
    entries
        .iter()
        .map(|entry| expand_entry(folder, folder_rel, &market, entry))
        .collect()
}

fn read_marketplace(folder: &Path) -> Result<(MarketplaceRef, Vec<Value>), String> {
    let text = std::fs::read_to_string(folder.join(MARKETPLACE_JSON)).map_err(|e| e.to_string())?;
    let mut value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .map_or_else(|| folder_name(folder), str::to_string);
    match value.get_mut("plugins").map(Value::take) {
        Some(Value::Array(entries)) => Ok((MarketplaceRef { name }, entries)),
        _ => Err("`plugins` must be an array".into()),
    }
}

fn folder_name(folder: &Path) -> String {
    std::fs::canonicalize(folder)
        .unwrap_or_else(|_| folder.to_path_buf())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

fn expand_entry(
    folder: &Path,
    folder_rel: &str,
    market: &MarketplaceRef,
    entry: &Value,
) -> Discovered {
    let name = entry
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let row = |rel_path: String, result: Result<PluginManifest, String>| Discovered {
        rel_path,
        name: name.clone(),
        result: result.map_err(|reason| format!("marketplace entry \"{name}\": {reason}")),
    };
    let remote_rel = format!("{folder_rel}#{name}");
    if name == BUILTIN_PLUGIN || !is_valid_plugin_name(&name) {
        return row(remote_rel, Err("invalid plugin name".into()));
    }
    match entry.get("source") {
        Some(Value::String(source)) => {
            let Some(rel) = normalize_source(source) else {
                return row(remote_rel, Err(ESCAPES_REPO.into()));
            };
            match in_repo_manifest(folder, &rel) {
                Ok(mut manifest) => {
                    manifest.name = name.clone();
                    manifest.marketplace = Some(market.clone());
                    Discovered {
                        rel_path: join_rel(folder_rel, &rel),
                        name: name.clone(),
                        result: Ok(manifest),
                    }
                }
                Err(reason) => row(remote_rel, Err(reason)),
            }
        }
        Some(Value::Object(source)) => {
            let description = entry
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            row(
                remote_rel,
                Ok(PluginManifest {
                    name: name.clone(),
                    version: "0.0.0".into(),
                    description,
                    author: None,
                    layout: PluginLayout::SkillsOnly,
                    skills: Vec::new(),
                    mcp_servers: Vec::new(),
                    unsupported: Vec::new(),
                    marketplace: Some(market.clone()),
                    external: Some(external_source(source)),
                }),
            )
        }
        _ => row(remote_rel, Err("source must be a path or an object".into())),
    }
}

/// `"./plugins/foo/"` → `"plugins/foo"`, `"./"` → `""`; `None` for absolute
/// paths or any `..` component.
fn normalize_source(source: &str) -> Option<String> {
    let mut parts = Vec::new();
    for component in Path::new(source).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.join("/"))
}

fn join_rel(folder_rel: &str, rel: &str) -> String {
    match (folder_rel.is_empty(), rel.is_empty()) {
        (true, _) => rel.to_string(),
        (false, true) => folder_rel.to_string(),
        (false, false) => format!("{folder_rel}/{rel}"),
    }
}

fn in_repo_manifest(folder: &Path, rel: &str) -> Result<PluginManifest, String> {
    let target = folder.join(rel);
    if !target.is_dir() {
        return Err("source path does not exist".into());
    }
    match (
        std::fs::canonicalize(folder),
        std::fs::canonicalize(&target),
    ) {
        (Ok(root), Ok(target)) if target.starts_with(&root) => {}
        _ => return Err(ESCAPES_REPO.into()),
    }
    if !target.join(PLUGIN_JSON).is_file() && !has_skills_package(&target) {
        return Err("no plugin or skills found at source".into());
    }
    parse_plugin(&target)
}

fn external_source(source: &Map<String, Value>) -> ExternalSource {
    let field = |key: &str| source.get(key).and_then(Value::as_str);
    let kind = field("source").unwrap_or("unknown");
    match kind {
        "github" => ExternalSource {
            kind: kind.into(),
            url: field("repo").map(github_url),
        },
        "git-subdir" => ExternalSource {
            kind: match field("path") {
                Some(path) => format!("git-subdir:{path}"),
                None => kind.into(),
            },
            url: field("url").map(|url| {
                if is_github_shorthand(url) {
                    github_url(url)
                } else {
                    url.to_string()
                }
            }),
        },
        "url" => ExternalSource {
            kind: kind.into(),
            url: field("url").map(str::to_string),
        },
        _ => ExternalSource {
            kind: kind.into(),
            url: None,
        },
    }
}

fn github_url(repo: &str) -> String {
    format!("https://github.com/{repo}.git")
}

/// `owner/repo` with nothing that marks a URL, ssh remote, or path.
fn is_github_shorthand(url: &str) -> bool {
    let mut parts = url.split('/');
    let valid = |part: Option<&str>| {
        part.is_some_and(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
    };
    valid(parts.next()) && valid(parts.next()) && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::capability::{MARKETPLACE_JSON, PLUGIN_JSON};
    use crate::plugins::discover::discover;
    use crate::plugins::manifest::{ExternalSource, PluginLayout};
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

    fn write_skill(dir: &Path, name: &str) {
        write(
            &dir.join("SKILL.md"),
            &format!("---\nname: {name}\ndescription: About {name}\n---\nbody"),
        );
    }

    fn market(dir: &Path, plugins: serde_json::Value) {
        write(
            &dir.join(MARKETPLACE_JSON),
            &serde_json::json!({ "name": "m", "plugins": plugins }).to_string(),
        );
    }

    #[test]
    fn marketplace_fixture_rows() {
        let rows = expand_marketplace(&fixture("marketplace-repo"), "marketplace-repo");
        assert_eq!(rows.len(), 5, "{rows:?}");
        let by = |rel: &str| {
            rows.iter()
                .find(|d| d.rel_path == rel)
                .unwrap_or_else(|| panic!("missing row {rel}: {rows:?}"))
        };
        let alpha = by("marketplace-repo/plugins/alpha")
            .result
            .as_ref()
            .unwrap();
        assert_eq!(alpha.name, "alpha");
        assert_eq!(alpha.marketplace.as_ref().unwrap().name, "acme-market");
        assert_eq!(alpha.skills.len(), 1);
        assert_eq!(alpha.external, None);
        let beta = by("marketplace-repo/plugins/beta");
        assert_eq!(beta.name, "beta-skills");
        let beta = beta.result.as_ref().unwrap();
        assert_eq!(beta.name, "beta-skills");
        assert_eq!(beta.layout, PluginLayout::SkillsOnly);
        assert_eq!(beta.marketplace.as_ref().unwrap().name, "acme-market");
        let remote = by("marketplace-repo#remote-one");
        assert_eq!(remote.name, "remote-one");
        let remote = remote.result.as_ref().unwrap();
        assert_eq!(
            remote.external,
            Some(ExternalSource {
                kind: "github".into(),
                url: Some("https://github.com/acme/remote-one.git".into()),
            })
        );
        assert_eq!(remote.description, "Hosted elsewhere");
        assert_eq!(remote.version, "0.0.0");
        assert_eq!(remote.marketplace.as_ref().unwrap().name, "acme-market");
        assert_eq!(
            by("marketplace-repo#escape").result.as_ref().unwrap_err(),
            "marketplace entry \"escape\": source must stay inside the repository"
        );
        assert_eq!(
            by("marketplace-repo#ghost").result.as_ref().unwrap_err(),
            "marketplace entry \"ghost\": source path does not exist"
        );
    }

    #[test]
    fn invalid_marketplace_json_is_one_invalid_row() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("broken");
        write(&folder.join(MARKETPLACE_JSON), "{");
        let rows = expand_marketplace(&folder, "broken");
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].rel_path, "broken");
        assert_eq!(rows[0].name, "broken");
        let err = rows[0].result.as_ref().unwrap_err();
        assert!(err.starts_with("invalid marketplace.json: "), "{err}");

        write(&folder.join(MARKETPLACE_JSON), r#"{"plugins":{}}"#);
        let rows = expand_marketplace(&folder, "broken");
        assert_eq!(rows.len(), 1, "{rows:?}");
        let err = rows[0].result.as_ref().unwrap_err();
        assert!(err.starts_with("invalid marketplace.json: "), "{err}");
    }

    #[test]
    fn marketplace_root_entry_yields_single_row() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("m");
        write(&folder.join(PLUGIN_JSON), r#"{"name":"m"}"#);
        market(
            &folder,
            serde_json::json!([{ "name": "m", "source": "./" }]),
        );
        let found = discover(dir.path()).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rel_path, "m");
        let manifest = found[0].result.as_ref().unwrap();
        assert_eq!(manifest.layout, PluginLayout::Plugin);
        assert_eq!(manifest.marketplace.as_ref().unwrap().name, "m");
    }

    #[test]
    fn plugin_dir_itself_as_marketplace_uses_bare_rel_paths() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(&dir.path().join("p/skills/s"), "s");
        market(
            dir.path(),
            serde_json::json!([
                { "name": "p", "source": "./p/" },
                { "name": "r", "source": { "source": "url", "url": "https://x/r.git" } },
            ]),
        );
        let found = discover(dir.path()).unwrap();
        let rel: Vec<_> = found.iter().map(|d| d.rel_path.as_str()).collect();
        assert_eq!(rel, ["#r", "p"], "{found:?}");
        assert!(found.iter().all(|d| d.result.is_ok()), "{found:?}");
        assert_eq!(
            found[0].result.as_ref().unwrap().external,
            Some(ExternalSource {
                kind: "url".into(),
                url: Some("https://x/r.git".into()),
            })
        );
    }

    #[test]
    fn duplicate_entry_sources_keep_first() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("m");
        write_skill(&folder.join("p/skills/s"), "s");
        market(
            &folder,
            serde_json::json!([
                { "name": "first", "source": "./p" },
                { "name": "second", "source": "p/" },
            ]),
        );
        let found = discover(dir.path()).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rel_path, "m/p");
        assert_eq!(found[0].name, "first");
    }

    #[test]
    fn git_subdir_and_npm_sources() {
        let dir = tempfile::tempdir().unwrap();
        market(
            dir.path(),
            serde_json::json!([
                { "name": "sub", "source": { "source": "git-subdir", "url": "acme/mono", "path": "tools/x" } },
                { "name": "full", "source": { "source": "git-subdir", "url": "https://git.example/mono.git", "path": "y" } },
                { "name": "pkg", "source": { "source": "npm", "package": "@acme/pkg" } },
                { "name": "odd", "source": { "repo": "acme/odd" } },
            ]),
        );
        let rows = expand_marketplace(dir.path(), "");
        let external = |rel: &str| {
            rows.iter()
                .find(|d| d.rel_path == rel)
                .unwrap_or_else(|| panic!("missing row {rel}: {rows:?}"))
                .result
                .as_ref()
                .unwrap()
                .external
                .clone()
                .unwrap()
        };
        let sub = external("#sub");
        assert_eq!(sub.url.as_deref(), Some("https://github.com/acme/mono.git"));
        assert_eq!(sub.kind, "git-subdir:tools/x");
        let full = external("#full");
        assert_eq!(full.url.as_deref(), Some("https://git.example/mono.git"));
        assert_eq!(full.kind, "git-subdir:y");
        let pkg = external("#pkg");
        assert_eq!(pkg.url, None);
        assert_eq!(pkg.kind, "npm");
        assert_eq!(external("#odd").kind, "unknown");
    }

    #[test]
    fn invalid_entries_are_invalid_rows() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("m");
        std::fs::create_dir_all(folder.join("empty")).unwrap();
        write(&folder.join("file"), "x");
        market(
            &folder,
            serde_json::json!([
                { "name": "bad name", "source": "./empty" },
                { "name": "abs", "source": "/etc" },
                { "name": "file", "source": "./file" },
                { "name": "empty", "source": "./empty" },
                { "name": "nosrc" },
            ]),
        );
        let rows = expand_marketplace(&folder, "m");
        let err = |rel: &str| {
            rows.iter()
                .find(|d| d.rel_path == rel)
                .unwrap_or_else(|| panic!("missing row {rel}: {rows:?}"))
                .result
                .clone()
                .unwrap_err()
        };
        assert_eq!(
            err("m#bad name"),
            "marketplace entry \"bad name\": invalid plugin name"
        );
        assert_eq!(
            err("m#abs"),
            "marketplace entry \"abs\": source must stay inside the repository"
        );
        assert_eq!(
            err("m#file"),
            "marketplace entry \"file\": source path does not exist"
        );
        assert_eq!(
            err("m#empty"),
            "marketplace entry \"empty\": no plugin or skills found at source"
        );
        assert_eq!(
            err("m#nosrc"),
            "marketplace entry \"nosrc\": source must be a path or an object"
        );
    }

    #[test]
    fn marketplace_entry_is_not_expanded_again() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("m");
        let inner = folder.join("inner");
        market(&inner, serde_json::json!([]));
        market(
            &folder,
            serde_json::json!([{ "name": "inner", "source": "./inner" }]),
        );
        let rows = expand_marketplace(&folder, "m");
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(
            rows[0].result.as_ref().unwrap_err(),
            "marketplace entry \"inner\": no plugin or skills found at source"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_source_outside_repo_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside");
        write_skill(&outside.join("skills/s"), "s");
        let folder = dir.path().join("m");
        market(
            &folder,
            serde_json::json!([{ "name": "link", "source": "./link" }]),
        );
        std::os::unix::fs::symlink(&outside, folder.join("link")).unwrap();
        let rows = expand_marketplace(&folder, "m");
        assert_eq!(
            rows[0].result.as_ref().unwrap_err(),
            "marketplace entry \"link\": source must stay inside the repository"
        );
    }
}
