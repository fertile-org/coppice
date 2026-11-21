//! Format-only portability check (PB-10). Structural parse only — no Coppice
//! denylist, secret heuristics, or install clash rules.

use crate::plugins::skills::parse_skill_file;
use serde_json::Value;
use std::path::Path;
use walkdir::WalkDir;

/// Structural errors for Claude Code / Cursor–style plugin folders (empty = pass).
pub fn check_portable_format(plugin_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    if !plugin_root.is_dir() {
        errors.push("Plugin folder is missing.".into());
        return errors;
    }

    check_plugin_json(plugin_root, &mut errors);
    check_mcp_json(plugin_root, &mut errors);
    check_skills(plugin_root, &mut errors);
    errors
}

fn check_plugin_json(root: &Path, errors: &mut Vec<String>) {
    let path = root.join(".claude-plugin/plugin.json");
    if !path.is_file() {
        errors.push("Missing .claude-plugin/plugin.json.".into());
        return;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            errors.push("Could not read .claude-plugin/plugin.json.".into());
            return;
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            errors.push(".claude-plugin/plugin.json is not valid JSON.".into());
            return;
        }
    };
    let Some(obj) = value.as_object() else {
        errors.push(".claude-plugin/plugin.json must be a JSON object.".into());
        return;
    };
    match obj.get("name").and_then(|v| v.as_str()).map(str::trim) {
        Some(name) if !name.is_empty() => {}
        _ => errors.push("plugin.json needs a non-empty string `name`.".into()),
    }
    if let Some(v) = obj.get("version") {
        if !v.is_string() {
            errors.push("plugin.json `version` must be a string when present.".into());
        }
    }
    if let Some(v) = obj.get("description") {
        if !v.is_string() {
            errors.push("plugin.json `description` must be a string when present.".into());
        }
    }
}

fn check_mcp_json(root: &Path, errors: &mut Vec<String>) {
    let path = root.join(".mcp.json");
    if !path.is_file() {
        return;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            errors.push("Could not read .mcp.json.".into());
            return;
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            errors.push(".mcp.json is not valid JSON.".into());
            return;
        }
    };
    let Some(servers) = value.get("mcpServers").and_then(|v| v.as_object()) else {
        return;
    };
    for (name, server) in servers {
        let Some(obj) = server.as_object() else {
            errors.push(format!("MCP server `{name}` must be a JSON object."));
            continue;
        };
        if let Some(ty) = obj.get("type").and_then(|v| v.as_str()) {
            if ty != "stdio" && ty != "http" {
                errors.push(format!(
                    "MCP server `{name}` type `{ty}` is not portable (use stdio or http)."
                ));
            }
        }
    }
}

fn check_skills(root: &Path, errors: &mut Vec<String>) {
    let skills = root.join("skills");
    if !skills.is_dir() {
        return;
    }
    for entry in WalkDir::new(&skills).into_iter().filter_map(|e| e.ok()) {
        if entry.file_name() != "SKILL.md" || !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .display()
            .to_string();
        let text = match std::fs::read_to_string(entry.path()) {
            Ok(t) => t,
            Err(_) => {
                errors.push(format!("Could not read `{rel}`."));
                continue;
            }
        };
        if let Err(err) = parse_skill_file(&text) {
            errors.push(format!("`{rel}`: {err:#}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/plugins/builder-check")
            .join(name)
    }

    #[test]
    fn pass_minimal_is_portable() {
        let errors = check_portable_format(&fixture("pass-minimal"));
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn bad_json_fails() {
        let errors = check_portable_format(&fixture("fail-portable-bad-json"));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("not valid JSON") || e.contains("plugin.json")),
            "{errors:?}"
        );
    }

    #[test]
    fn missing_frontmatter_fails() {
        let errors = check_portable_format(&fixture("fail-portable-no-frontmatter"));
        assert!(
            errors.iter().any(|e| e.contains("frontmatter") || e.contains("SKILL.md")),
            "{errors:?}"
        );
    }
}
