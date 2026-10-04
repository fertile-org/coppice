use std::collections::HashMap;
use std::path::Path;

use thiserror::Error;

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

/// Load all `*.md` files; map key = file stem (e.g. `pm.md` → `"pm"`).
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
        let content = std::fs::read_to_string(&path)?;
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

    #[test]
    fn load_pm_template_from_disk() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agent_templates");
        let templates = load(&dir).expect("load templates");
        let pm = templates.get("pm").expect("pm template");
        assert!(pm.contains("# SOUL"));
        assert!(pm.contains("## Mission"));
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
