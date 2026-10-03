use crate::plugins::capability::MARKETPLACE_JSON;
use crate::plugins::manifest::{is_plugin_dir_root, is_plugin_root, parse_plugin, PluginManifest};
use crate::plugins::marketplace::expand_marketplace;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Discovered {
    pub rel_path: String,
    pub name: String,
    pub result: Result<PluginManifest, String>,
}

/// Scans `dir` itself (rel_path `""`) and, only if it is not a plugin, each direct child.
/// `dir` is judged by [`is_plugin_dir_root`], children by [`is_plugin_root`].
/// A folder with `marketplace.json` yields its entries instead of itself.
pub fn discover(dir: &Path) -> std::io::Result<Vec<Discovered>> {
    if is_plugin_dir_root(dir) {
        if dir.join(MARKETPLACE_JSON).is_file() {
            return Ok(dedupe(expand_marketplace(dir, "")));
        }
        let folder = std::fs::canonicalize(dir)?
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        return Ok(vec![discovered(dir, String::new(), folder)]);
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(folder) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !path.is_dir() || !is_plugin_root(&path) {
            continue;
        }
        if path.join(MARKETPLACE_JSON).is_file() {
            found.extend(expand_marketplace(&path, folder));
        } else {
            found.push(discovered(&path, folder.to_string(), folder.to_string()));
        }
    }
    Ok(dedupe(found))
}

/// Keeps the first row per `rel_path`, then sorts by it.
fn dedupe(found: Vec<Discovered>) -> Vec<Discovered> {
    let mut seen = HashSet::new();
    let mut kept: Vec<_> = found
        .into_iter()
        .filter(|d| {
            let first = seen.insert(d.rel_path.clone());
            if !first {
                tracing::warn!(rel_path = %d.rel_path, name = %d.name, "duplicate plugin path; keeping the first");
            }
            first
        })
        .collect();
    kept.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    kept
}

fn discovered(path: &Path, rel_path: String, folder: String) -> Discovered {
    let result = parse_plugin(path);
    let name = result.as_ref().map_or(folder, |m| m.name.clone());
    Discovered {
        rel_path,
        name,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/plugins")
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn discover_depth0_plugin_skips_children() {
        let found = discover(&fixtures().join("sample-plugin")).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rel_path, "");
        assert_eq!(found[0].name, "sample-plugin");
        assert!(found[0].result.is_ok());
    }

    #[test]
    fn discover_depth1_children() {
        let found = discover(&fixtures()).unwrap();
        let expected = [
            ("inline-mcp", "inline-mcp", true),
            ("m10-smoke", "m10-smoke", true),
            ("marketplace-repo#escape", "escape", false),
            ("marketplace-repo#ghost", "ghost", false),
            ("marketplace-repo#remote-one", "remote-one", true),
            ("marketplace-repo/plugins/alpha", "alpha", true),
            ("marketplace-repo/plugins/beta", "beta-skills", true),
            ("mcp-fake", "mcp-fake", true),
            ("mcp-fake-slow", "mcp-fake-slow", true),
            ("mcp-http", "mcp-http", true),
            ("sample-plugin", "sample-plugin", true),
            ("single-skill", "single-skill", true),
            ("skills-catalog", "skills-catalog", true),
            ("skills-only", "skills-only", true),
            ("superpowers-like", "superpowers-like", true),
        ];
        let actual: Vec<_> = found
            .iter()
            .map(|d| (d.rel_path.as_str(), d.name.as_str(), d.result.is_ok()))
            .collect();
        assert_eq!(actual, expected, "{found:?}");
    }

    #[test]
    fn discover_expands_marketplace_child() {
        let dir = tempfile::tempdir().unwrap();
        let market = dir.path().join("marketplace-repo");
        copy_tree(&fixtures().join("marketplace-repo"), &market);
        write(
            &dir.path().join("other/.claude-plugin/plugin.json"),
            r#"{"name":"other"}"#,
        );
        let found = discover(dir.path()).unwrap();
        let rel: Vec<_> = found.iter().map(|d| d.rel_path.as_str()).collect();
        assert_eq!(
            rel,
            [
                "marketplace-repo#escape",
                "marketplace-repo#ghost",
                "marketplace-repo#remote-one",
                "marketplace-repo/plugins/alpha",
                "marketplace-repo/plugins/beta",
                "other",
            ]
        );
        assert!(found.iter().all(|d| d.rel_path != "marketplace-repo"));
    }

    #[test]
    fn discover_plugin_dir_marketplace_skips_itself() {
        let dir = tempfile::tempdir().unwrap();
        copy_tree(&fixtures().join("marketplace-repo"), dir.path());
        let found = discover(dir.path()).unwrap();
        let rel: Vec<_> = found.iter().map(|d| d.rel_path.as_str()).collect();
        assert_eq!(
            rel,
            [
                "#escape",
                "#ghost",
                "#remote-one",
                "plugins/alpha",
                "plugins/beta"
            ]
        );
    }

    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }

    #[test]
    fn discover_single_skill_child_does_not_make_dir_a_plugin() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("a/SKILL.md"),
            "---\nname: a\ndescription: A\n---\nbody",
        );
        write(
            &dir.path().join("b/.claude-plugin/plugin.json"),
            r#"{"name":"b"}"#,
        );
        let found = discover(dir.path()).unwrap();
        let rel: Vec<_> = found.iter().map(|d| d.rel_path.as_str()).collect();
        assert_eq!(rel, ["a", "b"]);
        assert!(found.iter().all(|d| d.result.is_ok()), "{found:?}");
    }

    #[test]
    fn discover_ignores_deeper_nesting_and_files() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("a/b/.claude-plugin/plugin.json"),
            r#"{"name":"deep"}"#,
        );
        write(&dir.path().join("loose.txt"), "not a plugin");
        assert!(discover(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn discover_names_failed_plugin_after_folder() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("broken/.claude-plugin/plugin.json"), "{");
        let found = discover(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "broken");
        assert!(found[0].result.is_err());
    }
}
