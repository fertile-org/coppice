use crate::plugins::manifest::{is_plugin_root, parse_plugin, PluginManifest};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Discovered {
    pub rel_path: String,
    pub name: String,
    pub result: Result<PluginManifest, String>,
}

/// Scans `dir` itself (rel_path `""`) and, only if it is not a plugin, each direct child.
pub fn discover(dir: &Path) -> std::io::Result<Vec<Discovered>> {
    if is_plugin_root(dir) {
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
        if path.is_dir() && is_plugin_root(&path) {
            found.push(discovered(&path, folder.to_string(), folder.to_string()));
        }
    }
    found.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(found)
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
            "inline-mcp",
            "m10-smoke",
            "mcp-fake",
            "mcp-fake-slow",
            "mcp-http",
            "sample-plugin",
            "skills-only",
            "superpowers-like",
        ];
        let rel: Vec<_> = found.iter().map(|d| d.rel_path.as_str()).collect();
        assert_eq!(rel, expected);
        let names: Vec<_> = found.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, expected);
        assert!(found.iter().all(|d| d.result.is_ok()));
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
