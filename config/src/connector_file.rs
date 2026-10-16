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
///
/// A document that holds a secret (a key such as `master_key` or `session_secret`,
/// or a URL with userinfo) is written `0600`. Anything else keeps the mode the
/// file already has, or `0644` when the file is new.
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
            file.set_permissions(std::fs::Permissions::from_mode(toml_file_mode(
                path, contents,
            )))?;
        }
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(unix)]
fn toml_file_mode(path: &Path, contents: &str) -> u32 {
    if toml_bears_secret(contents) {
        return 0o600;
    }
    std::fs::metadata(path)
        .ok()
        .map(|meta| {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o777
        })
        .unwrap_or(0o644)
}

#[cfg(unix)]
fn toml_bears_secret(contents: &str) -> bool {
    let Ok(doc) = contents.parse::<DocumentMut>() else {
        return false;
    };
    table_bears_secret(doc.as_table())
}

#[cfg(unix)]
fn is_secret_key(key: &str) -> bool {
    matches!(
        key,
        "session_secret"
            | "bootstrap_password"
            | "bootstrap_admin_password"
            | "master_key"
            | "password"
            | "secret"
            | "api_key"
            | "token"
            | "access_token"
            | "refresh_token"
    )
}

#[cfg(unix)]
fn table_bears_secret(table: &toml_edit::Table) -> bool {
    table
        .iter()
        .any(|(key, item)| is_secret_key(key) || item_bears_secret(item))
}

#[cfg(unix)]
fn item_bears_secret(item: &Item) -> bool {
    if let Some(table) = item.as_table() {
        return table_bears_secret(table);
    }
    if let Some(tables) = item.as_array_of_tables() {
        return tables.iter().any(table_bears_secret);
    }
    item.as_value().is_some_and(value_bears_secret)
}

#[cfg(unix)]
fn value_bears_secret(value: &toml_edit::Value) -> bool {
    match value {
        toml_edit::Value::String(text) => url_has_userinfo(text.value()),
        toml_edit::Value::InlineTable(table) => table
            .iter()
            .any(|(key, child)| is_secret_key(key) || value_bears_secret(child)),
        toml_edit::Value::Array(array) => array.iter().any(value_bears_secret),
        _ => false,
    }
}

#[cfg(unix)]
fn url_has_userinfo(value: &str) -> bool {
    let Some(scheme) = value.find("://") else {
        return false;
    };
    let rest = &value[scheme + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest[..authority_end].contains('@')
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

    #[cfg(unix)]
    #[test]
    fn atomic_write_is_private_for_secrets_and_keeps_other_modes() {
        use std::os::unix::fs::PermissionsExt;

        fn mode(path: &Path) -> u32 {
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777
        }

        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("config.toml");
        write_toml_atomic(&plain, "worker_count = 2\n").unwrap();
        assert_eq!(mode(&plain), 0o644);

        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o640)).unwrap();
        write_toml_atomic(&plain, "worker_count = 3\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&plain).unwrap(),
            "worker_count = 3\n"
        );
        assert_eq!(mode(&plain), 0o640);

        write_toml_atomic(&plain, "master_key = \"sekrit\"\n").unwrap();
        assert_eq!(mode(&plain), 0o600);

        let fresh = dir.path().join("fresh.toml");
        write_toml_atomic(
            &fresh,
            "[database]\nurl = \"postgres://user:s3cret@localhost/db\"\n",
        )
        .unwrap();
        assert_eq!(mode(&fresh), 0o600);

        let session = dir.path().join("session.toml");
        std::fs::write(&session, "port = 1\n").unwrap();
        std::fs::set_permissions(&session, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_toml_atomic(&session, "[auth]\nsession_secret = \"s\"\n").unwrap();
        assert_eq!(mode(&session), 0o600);
    }
}
