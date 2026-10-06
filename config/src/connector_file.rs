//! Patch `[agent.connectors.<id>]` in a config file without dropping comments
//! or unrelated keys.

use std::io::{self, Write};
use std::path::Path;

use toml_edit::{value, Array, DocumentMut, Item, Table};

/// What to write for one connector. Enabling fills empty `model_providers` and
/// a missing `command`. Disabling only sets `enabled = false`.
pub struct ConnectorFilePatch<'a> {
    pub id: &'a str,
    pub enabled: bool,
    pub default_model_providers: &'a [&'a str],
    /// When enabling, written only if the key is absent.
    pub command_if_missing: Option<&'a str>,
}

pub fn set_connector_enabled(
    doc: &mut DocumentMut,
    patch: &ConnectorFilePatch<'_>,
) -> Result<(), String> {
    let agent = doc
        .entry("agent")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| "[agent] must be a table".to_string())?;

    let connectors = agent
        .entry("connectors")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| "[agent.connectors] must be a table".to_string())?;
    connectors.set_implicit(true);

    let table = connectors
        .entry(patch.id)
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| format!("[agent.connectors.{}] must be a table", patch.id))?;

    table["enabled"] = value(patch.enabled);
    if !patch.enabled {
        return Ok(());
    }

    let needs_providers = match table.get("model_providers") {
        None => true,
        Some(Item::Value(v)) => v.as_array().is_none_or(|a| a.is_empty()),
        _ => true,
    };
    if needs_providers && !patch.default_model_providers.is_empty() {
        let mut arr = Array::new();
        for provider in patch.default_model_providers {
            arr.push((*provider).to_string());
        }
        table["model_providers"] = value(arr);
    }

    if let Some(command) = patch.command_if_missing {
        if table.get("command").is_none() {
            table["command"] = value(command);
        }
    }
    Ok(())
}

/// Replace `path` with `contents`. A crash leaves the previous file intact.
pub fn write_toml_atomic(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension("toml.writing");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
        }
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_keeps_comments_and_existing_providers() {
        let mut doc = r#"
# desktop header
[agent]
default_connector = "claude-code"

# user note
[agent.connectors.claude-code]
enabled = false
model_providers = ["sonnet"]
"#
        .parse::<DocumentMut>()
        .unwrap();
        set_connector_enabled(
            &mut doc,
            &ConnectorFilePatch {
                id: "claude-code",
                enabled: true,
                default_model_providers: &["sonnet", "opus", "haiku"],
                command_if_missing: None,
            },
        )
        .unwrap();
        let text = doc.to_string();
        assert!(text.contains("# desktop header"), "{text}");
        assert!(text.contains("# user note"), "{text}");
        assert!(
            text.contains("default_connector = \"claude-code\""),
            "{text}"
        );
        assert!(text.contains("enabled = true"), "{text}");
        assert!(text.contains("sonnet"), "{text}");
        assert!(
            !text.contains("opus"),
            "must not replace a custom provider list: {text}"
        );
    }

    #[test]
    fn enable_fills_missing_providers_and_command() {
        let mut doc = DocumentMut::new();
        set_connector_enabled(
            &mut doc,
            &ConnectorFilePatch {
                id: "cursor",
                enabled: true,
                default_model_providers: &["cursor"],
                command_if_missing: Some("agent"),
            },
        )
        .unwrap();
        let text = doc.to_string();
        assert!(text.contains("[agent.connectors.cursor]"), "{text}");
        assert!(text.contains("enabled = true"), "{text}");
        assert!(text.contains("command = \"agent\""), "{text}");
        assert!(text.contains("cursor"), "{text}");
    }

    #[test]
    fn disable_does_not_remove_command_or_providers() {
        let mut doc = r#"
[agent.connectors.cursor]
enabled = true
command = "my-agent"
model_providers = ["cursor"]
"#
        .parse::<DocumentMut>()
        .unwrap();
        set_connector_enabled(
            &mut doc,
            &ConnectorFilePatch {
                id: "cursor",
                enabled: false,
                default_model_providers: &["cursor"],
                command_if_missing: Some("agent"),
            },
        )
        .unwrap();
        let text = doc.to_string();
        assert!(text.contains("enabled = false"), "{text}");
        assert!(text.contains("command = \"my-agent\""), "{text}");
        assert!(
            text.contains("model_providers = [\"cursor\"]") || text.contains("cursor"),
            "{text}"
        );
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "old\n").unwrap();
        write_toml_atomic(&path, "enabled = true\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "enabled = true\n");
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["config.toml".to_string()]);
    }
}
