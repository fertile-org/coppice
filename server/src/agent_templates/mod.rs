use std::collections::HashMap;
use std::path::Path;

use thiserror::Error;

const POSTURE: &str = include_str!("../../agent_templates/posture.md");
const POSTURE_PLACEHOLDER: &str = "{{posture}}";

#[derive(Debug, Error)]
pub enum AgentTemplateError {
    #[error("failed to read agent templates directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("agent template file name is not valid UTF-8: {0}")]
    InvalidFileName(std::path::PathBuf),
    #[error("missing agent template for preset key: {key}")]
    MissingPreset { key: String },
}

pub fn templates_dir() -> std::path::PathBuf {
    templates_dir_from(std::env::var("COPPICE_AGENT_TEMPLATES_DIR").ok().as_deref())
}

/// Packaged builds have no source tree, so the env override must win when set.
pub fn templates_dir_from(env: Option<&str>) -> std::path::PathBuf {
    match env {
        Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir),
        _ => std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agent_templates"),
    }
}

/// Load all agent `*.md` templates; map key = file stem (e.g. `pm.md` → `"pm"`).
///
/// `posture.md` is not a template key: its content is embedded via `include_str!`
/// and substituted for `{{posture}}` in every loaded file.
pub fn load(dir: &Path) -> Result<HashMap<String, String>, AgentTemplateError> {
    let mut out = HashMap::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let key = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| AgentTemplateError::InvalidFileName(path.clone()))?
            .to_string();
        if key == "posture" {
            continue;
        }
        let raw = std::fs::read_to_string(&path)?;
        let content = raw.replace(POSTURE_PLACEHOLDER, POSTURE);
        out.insert(key, content);
    }
    Ok(out)
}

pub async fn ensure_all_presets_have_templates(
    pool: &sqlx::PgPool,
    templates: &HashMap<String, String>,
) -> Result<(), AgentTemplateError> {
    let keys: Vec<String> = sqlx::query_scalar("SELECT key FROM agent_presets ORDER BY key")
        .fetch_all(pool)
        .await
        .map_err(|e| AgentTemplateError::Io(std::io::Error::other(e)))?;

    for key in keys {
        if !templates.contains_key(&key) {
            return Err(AgentTemplateError::MissingPreset { key });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn templates_dir_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agent_templates")
    }

    #[test]
    fn load_pm_template_from_disk() {
        let templates = load(&templates_dir_path()).expect("load templates");
        let pm = templates.get("pm").expect("pm template");
        assert!(pm.contains("# Operating posture"));
        assert!(
            pm.contains("Hard never (without explicit human approval)"),
            "expected hard-never list in composed prompt"
        );
        assert!(pm.contains("## Mission"));
        assert!(!pm.contains(POSTURE_PLACEHOLDER));
    }

    #[test]
    fn posture_is_single_sourced_and_not_a_template_key() {
        let templates = load(&templates_dir_path()).expect("load templates");
        assert!(
            !templates.contains_key("posture"),
            "posture.md must not be exposed as an agent-template key"
        );
        assert!(
            templates.len() >= 10,
            "expected all role templates, got {}",
            templates.len()
        );
        for (key, content) in &templates {
            assert!(
                content.contains("# Operating posture"),
                "{key} missing composed posture"
            );
            assert!(
                content.contains("Hard never (without explicit human approval)"),
                "{key} missing hard-never list"
            );
            assert!(
                content.contains("Escalate when ambiguity would change the solution"),
                "{key} missing escalation triggers"
            );
            assert!(
                !content.contains(POSTURE_PLACEHOLDER),
                "{key} still has unsubstituted placeholder"
            );
        }
        assert_eq!(
            POSTURE.trim(),
            include_str!("../../agent_templates/posture.md").trim()
        );
    }

    #[test]
    fn templates_dir_from_uses_env_override() {
        assert_eq!(templates_dir_from(Some("/x/t")), PathBuf::from("/x/t"));
    }

    #[test]
    fn templates_dir_from_falls_back_to_source_tree() {
        assert!(templates_dir_from(Some("")).ends_with("agent_templates"));
        assert!(templates_dir_from(None).ends_with("agent_templates"));
    }
}
