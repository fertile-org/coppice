//! Builder format checker (PB-7). Returns plain-language errors; never secret values.

use crate::mcp::proxy::naming::{exposed_name, MAX_EXPOSED_LEN};
use crate::plugins::builtin::BUILTIN_PLUGIN;
use crate::plugins::capability::{
    AgentsCapability, CapabilityOutcome, CapabilityParser, CommandsCapability, HooksCapability,
    McpServerTransport, MARKETPLACE_JSON,
};
use crate::plugins::manifest::{is_valid_plugin_name, parse_plugin, PluginLayout};
use crate::plugins::portable_format::check_portable_format;
use std::collections::HashSet;
use std::path::{Component, Path};
use walkdir::WalkDir;

/// Soft cap so `<plugin>__<tool>` stays ≤ [`MAX_EXPOSED_LEN`] without hashing.
const MAX_PLUGIN_NAME_FOR_TOOLS: usize = 20;
const MAX_TOOL_NAME_HINT: usize = 28;

const GATEWAY_DENYLIST: &[&str] = &[
    "ticket_",
    "comment_post",
    "result_submit",
    "knowledge_",
    "open_repo",
    "skill_list",
    "skill_load",
    "design__",
];

const FORBIDDEN_DIRS: &[&str] = &["commands", "agents", "hooks"];

const COPPICE_CONCEPTS: &[&str] = &["Plan Review"];

const SKIP_DIR_NAMES: &[&str] = &[".git", ".agent"];

/// Scan `plugin_root` and return plain-language errors (empty = pass).
/// `installed_names` is used for clash / reserved `coppice` checks.
pub fn check_plugin(plugin_root: &Path, installed_names: &HashSet<String>) -> Vec<String> {
    let mut errors = Vec::new();

    if !plugin_root.is_dir() {
        errors.push("Plugin folder is missing.".into());
        return errors;
    }

    if !plugin_root.join("README.md").is_file() {
        errors.push("Add a short README.md that explains what the plugin does.".into());
    }

    // Structural portability first (Claude Code / Cursor layout); Coppice rules follow.
    errors.extend(check_portable_format(plugin_root));

    check_escapes(plugin_root, &mut errors);

    let manifest = match parse_plugin(plugin_root) {
        Ok(m) => m,
        Err(reason) => {
            errors.push(plain_parse_error(&reason));
            scan_text_files(plugin_root, &mut errors);
            check_forbidden_dirs(plugin_root, &mut errors);
            check_marketplace_npm(plugin_root, &mut errors);
            return dedupe(errors);
        }
    };

    if manifest.name == BUILTIN_PLUGIN {
        errors.push(format!(
            "The name `{BUILTIN_PLUGIN}` is reserved. Pick another name."
        ));
    } else if !is_valid_plugin_name(&manifest.name) {
        errors.push(format!(
            "Plugin name `{}` is invalid. Use letters, digits, `.`, `_`, or `-` (max 64).",
            manifest.name
        ));
    } else if installed_names.contains(&manifest.name) {
        errors.push(format!(
            "A plugin called {} already exists. Pick another name.",
            manifest.name
        ));
    }

    if manifest.name.len() > MAX_PLUGIN_NAME_FOR_TOOLS {
        errors.push(format!(
            "Keep the plugin name at {MAX_PLUGIN_NAME_FOR_TOOLS} characters or fewer so tool names stay short enough for Coppice."
        ));
    }

    if manifest.layout == PluginLayout::Plugin {
        if manifest.version.trim().is_empty() {
            errors.push("plugin.json needs a non-empty version.".into());
        }
        if manifest.description.trim().is_empty() {
            errors.push("plugin.json needs a non-empty description.".into());
        }
    }

    for skill in &manifest.skills {
        if let Some(err) = &skill.error {
            errors.push(plain_skill_error(&skill.name, err));
            continue;
        }
        let folder = Path::new(&skill.rel_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if folder != skill.name {
            errors.push(format!(
                "Skill folder `{folder}` must match frontmatter name `{}`.",
                skill.name
            ));
        }
        if skill.description.trim().is_empty() {
            errors.push(format!(
                "Skill `{}` is missing its description.",
                skill.name
            ));
        }
    }

    for server in &manifest.mcp_servers {
        if let Some(err) = &server.error {
            errors.push(format!(
                "Tool server `{}` is invalid: {}.",
                server.name,
                trim_period(err)
            ));
        }
        match &server.transport {
            McpServerTransport::Stdio { .. } | McpServerTransport::Http { .. } => {}
            McpServerTransport::Unsupported { kind } if kind == "sse" => {
                errors.push(format!(
                    "Tool server `{}` uses SSE, which Coppice does not support. Use stdio or http.",
                    server.name
                ));
            }
            McpServerTransport::Unsupported { kind } => {
                errors.push(format!(
                    "Tool server `{}` uses `{kind}`, which is not allowed. Use stdio or http.",
                    server.name
                ));
            }
        }
        let full = format!("{}__{}", manifest.name, server.name);
        if full.len() > MAX_EXPOSED_LEN
            || exposed_name(&manifest.name, &server.name).len() > MAX_EXPOSED_LEN
        {
            errors.push(format!(
                "Tool server `{}` makes `<plugin>__<tool>` longer than {MAX_EXPOSED_LEN} characters. Shorten the plugin or server name.",
                server.name
            ));
        }
        if server.name.len() > MAX_TOOL_NAME_HINT {
            errors.push(format!(
                "Tool server name `{}` is longer than {MAX_TOOL_NAME_HINT} characters.",
                server.name
            ));
        }
    }

    check_forbidden_dirs(plugin_root, &mut errors);
    check_marketplace_npm(plugin_root, &mut errors);
    scan_text_files(plugin_root, &mut errors);

    dedupe(errors)
}

fn check_forbidden_dirs(root: &Path, errors: &mut Vec<String>) {
    for key in FORBIDDEN_DIRS {
        match *key {
            "commands" => push_if_present::<CommandsCapability>(root, errors),
            "agents" => push_if_present::<AgentsCapability>(root, errors),
            "hooks" => push_if_present::<HooksCapability>(root, errors),
            _ => {}
        }
    }
}

fn push_if_present<P: CapabilityParser>(root: &Path, errors: &mut Vec<String>) {
    if matches!(
        P::parse(root, None, PluginLayout::Plugin),
        CapabilityOutcome::Unsupported(_)
    ) {
        errors.push(format!(
            "Remove the `{}` folder. Coppice does not support it in builder plugins.",
            P::KEY
        ));
    }
}

fn check_marketplace_npm(root: &Path, errors: &mut Vec<String>) {
    let path = root.join(MARKETPLACE_JSON);
    if !path.is_file() {
        return;
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        errors.push("Could not read .claude-plugin/marketplace.json.".into());
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        errors.push(".claude-plugin/marketplace.json is not valid JSON.".into());
        return;
    };
    let Some(plugins) = value.get("plugins").and_then(|p| p.as_array()) else {
        return;
    };
    for entry in plugins {
        let is_npm = entry
            .get("source")
            .and_then(|s| s.as_object())
            .and_then(|o| o.get("source"))
            .and_then(|s| s.as_str())
            == Some("npm");
        if is_npm {
            let name = entry
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unnamed");
            errors.push(format!(
                "Marketplace entry `{name}` uses an npm source, which is not allowed."
            ));
        }
    }
}

fn check_escapes(root: &Path, errors: &mut Vec<String>) {
    let Ok(canon_root) = std::fs::canonicalize(root) else {
        return;
    };
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !SKIP_DIR_NAMES.contains(&name.as_ref())
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if rel.components().any(|c| matches!(c, Component::ParentDir)) {
            errors.push("Plugin paths must stay inside the plugin folder (no `..`).".into());
            break;
        }
        if entry.file_type().is_symlink() {
            match std::fs::canonicalize(path) {
                Ok(target) if !target.starts_with(&canon_root) => {
                    errors.push(
                        "A symlink points outside the plugin folder. Remove it.".into(),
                    );
                }
                Err(_) => {
                    errors.push("A symlink could not be resolved; remove outbound links.".into());
                }
                Ok(_) => {}
            }
        }
    }
}

fn scan_text_files(root: &Path, errors: &mut Vec<String>) {
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !SKIP_DIR_NAMES.contains(&name.as_ref())
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let masked = mask_placeholders(&text);

        for needle in GATEWAY_DENYLIST {
            if masked.contains(needle) {
                errors.push(format!(
                    "Do not reference Coppice gateway tools (found `{needle}`)."
                ));
            }
        }

        if masked.contains("COPPICE_") {
            errors.push(
                "Do not rely on Coppice-only environment names (`COPPICE_*`).".into(),
            );
        }

        if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
            for concept in COPPICE_CONCEPTS {
                if masked.contains(concept) {
                    errors.push(format!(
                        "Skills must not mention Coppice-only concepts (`{concept}`)."
                    ));
                }
            }
            if masked.contains("mcp__") {
                errors.push(
                    "Skills must refer to tools by the server's own name, not `mcp__…`."
                        .into(),
                );
            }
        }

        if looks_like_email(&masked) {
            errors.push(
                "Replace literal email addresses with a `${KEY}` placeholder.".into(),
            );
        }
        if looks_like_home_path(&masked) {
            errors.push(
                "Replace home-directory paths with `${CLAUDE_PLUGIN_ROOT}` or a `${KEY}` placeholder."
                    .into(),
            );
        }
        if looks_like_token(&masked) {
            errors.push(
                "Replace literal tokens or keys with a `${KEY}` placeholder.".into(),
            );
        }
    }
}

/// Replace `${UPPER_SNAKE}` / `${KEY:-default}` so secret heuristics ignore them.
fn mask_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = text[i + 2..].find('}') {
                let body = &text[i + 2..i + 2 + end];
                let name = body.split(":-").next().unwrap_or(body);
                if !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
                    && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                {
                    out.push_str("${KEY}");
                    i += 2 + end + 1;
                    continue;
                }
            }
        }
        out.push(text[i..].chars().next().unwrap());
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    out
}

fn looks_like_email(text: &str) -> bool {
    text.chars().any(|c| c == '@')
        && text.split_whitespace().any(|word| {
            let Some((user, rest)) = word.split_once('@') else {
                return false;
            };
            let Some((host, tld)) = rest.rsplit_once('.') else {
                return false;
            };
            !user.is_empty()
                && !host.is_empty()
                && tld.len() >= 2
                && tld.chars().all(|c| c.is_ascii_alphabetic())
        })
}

fn looks_like_home_path(text: &str) -> bool {
    text.contains("/Users/") || text.contains("/home/")
}

fn looks_like_token(text: &str) -> bool {
    for prefix in ["ghp_", "sk-", "xoxb-", "xoxp-", "xoxa-", "xoxr-"] {
        if let Some(idx) = text.find(prefix) {
            let rest = &text[idx + prefix.len()..];
            let token: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if token.len() >= 20 {
                return true;
            }
        }
    }
    false
}

fn plain_parse_error(reason: &str) -> String {
    if reason.contains("reserved") {
        format!("The name `{BUILTIN_PLUGIN}` is reserved. Pick another name.")
    } else if reason.contains("invalid plugin name") {
        "Plugin name is invalid. Use letters, digits, `.`, `_`, or `-` (max 64).".into()
    } else if reason.contains("plugin.json") {
        format!("Could not read the plugin: {}.", trim_period(reason))
    } else {
        format!("Could not parse the plugin: {}.", trim_period(reason))
    }
}

fn plain_skill_error(name: &str, err: &str) -> String {
    if err.contains("description") {
        format!("Skill `{name}` is missing its description.")
    } else if err.contains("name") {
        format!("Skill `{name}` is missing its name.")
    } else if err.contains("frontmatter") {
        format!("Skill `{name}` needs YAML frontmatter with name and description.")
    } else if err.contains("escapes") {
        format!("Skill `{name}` path escapes the plugin folder.")
    } else {
        format!("Skill `{name}` is invalid: {}.", trim_period(err))
    }
}

fn trim_period(s: &str) -> &str {
    s.trim_end_matches('.')
}

fn dedupe(errors: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    errors
        .into_iter()
        .filter(|e| seen.insert(e.clone()))
        .collect()
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
    fn pass_minimal_is_clean() {
        let errors = check_plugin(&fixture("pass-minimal"), &HashSet::new());
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn fail_gateway_tools() {
        let errors = check_plugin(&fixture("fail-gateway"), &HashSet::new());
        assert!(
            errors.iter().any(|e| e.contains("gateway tools")),
            "{errors:?}"
        );
    }

    #[test]
    fn fail_forbidden_dirs() {
        let errors = check_plugin(&fixture("fail-forbidden-dirs"), &HashSet::new());
        for dir in ["commands", "agents", "hooks"] {
            assert!(
                errors.iter().any(|e| e.contains(dir)),
                "missing {dir}: {errors:?}"
            );
        }
    }

    #[test]
    fn fail_sse_transport() {
        let errors = check_plugin(&fixture("fail-sse"), &HashSet::new());
        assert!(
            errors.iter().any(|e| e.to_lowercase().contains("sse")),
            "{errors:?}"
        );
    }

    #[test]
    fn fail_npm_marketplace() {
        let errors = check_plugin(&fixture("fail-npm"), &HashSet::new());
        assert!(errors.iter().any(|e| e.contains("npm")), "{errors:?}");
    }

    #[test]
    fn fail_literal_secrets() {
        let errors = check_plugin(&fixture("fail-secrets"), &HashSet::new());
        assert!(
            errors.iter().any(|e| e.contains("email")),
            "email: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("home-directory") || e.contains("path")),
            "home path: {errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("token") || e.contains("key")),
            "token: {errors:?}"
        );
        let joined = errors.join("\n");
        assert!(
            !joined.contains("alice@example.com"),
            "must not echo email value"
        );
        assert!(!joined.contains("/Users/alice"), "must not echo home path");
        assert!(
            !joined.contains("ghp_abcdefghijklmnopqrstuvwxyz"),
            "must not echo token"
        );
    }

    #[test]
    fn name_clash_with_installed() {
        let mut installed = HashSet::new();
        installed.insert("pass-minimal".into());
        let errors = check_plugin(&fixture("pass-minimal"), &installed);
        assert!(
            errors.iter().any(|e| e.contains("already exists")),
            "{errors:?}"
        );
    }

    #[test]
    fn mask_placeholders_hides_values() {
        let masked = mask_placeholders("token ${API_KEY} and ${TOKEN:-x} stay");
        assert!(!masked.contains("API_KEY") || masked.contains("${KEY}"));
        assert_eq!(
            mask_placeholders("email ${EMAIL} ok"),
            "email ${KEY} ok"
        );
    }
}
