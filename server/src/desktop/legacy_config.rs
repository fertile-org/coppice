//! One-time import from the old lowercase config path into the desktop file.
//!
//! Electron's userData directory is `~/.config/Coppice` (capital C). Older
//! builds read `~/.config/coppice/config.toml`. Only an allowlist is copied.
//! `enabled` switches and every secret-bearing section stay behind.

use std::io;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table};

use crate::config::AppConfig;

pub const LEGACY_IMPORT_NOTICE: &str = "Imported your git author, worker, knowledge and connector settings from `~/.config/coppice/config.toml`. Connectors stay as set in Connectors. The old file is now `config.toml.imported`.";

#[cfg(test)]
const DENIED_ROOTS: &[&str] = &["database", "auth", "secrets", "server", "storage", "web"];

/// Copy allowlisted settings from the legacy file into `desktop_config`, then
/// rename the legacy file to `config.toml.imported`. `Ok(None)` when there is
/// nothing to import.
pub fn import_legacy_config(desktop_config: &Path) -> io::Result<Option<String>> {
    let legacy = AppConfig::global_config_path();
    if !legacy.is_file() {
        return Ok(None);
    }
    if same_file(&legacy, desktop_config) {
        return Ok(None);
    }
    let legacy_text = std::fs::read_to_string(&legacy)?;
    let legacy_doc = legacy_text.parse::<DocumentMut>().map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("legacy config {}: {err}", legacy.display()),
        )
    })?;
    let desktop_text = if desktop_config.is_file() {
        std::fs::read_to_string(desktop_config)?
    } else {
        String::new()
    };
    let mut desktop_doc = desktop_text.parse::<DocumentMut>().map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("desktop config {}: {err}", desktop_config.display()),
        )
    })?;
    apply_allowlist(&legacy_doc, &mut desktop_doc);
    crate::desktop::bootstrap::write_config_preserving(desktop_config, &desktop_doc.to_string())?;
    let imported = imported_path(&legacy);
    std::fs::rename(&legacy, &imported)?;
    Ok(Some(LEGACY_IMPORT_NOTICE.to_string()))
}

fn imported_path(legacy: &Path) -> PathBuf {
    legacy.with_file_name("config.toml.imported")
}

fn same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn apply_allowlist(legacy: &DocumentMut, desktop: &mut DocumentMut) {
    if let Some(git) = legacy.get("git").and_then(Item::as_table) {
        let dest = table_mut(desktop, &["git"]);
        copy_keys(git, dest, &["author_name", "author_email"]);
    }
    if let Some(agent) = legacy.get("agent").and_then(Item::as_table) {
        if agent.get("worker_count").is_some() {
            let dest = table_mut(desktop, &["agent"]);
            copy_keys(agent, dest, &["worker_count"]);
        }
        if let Some(connectors) = agent.get("connectors").and_then(Item::as_table) {
            for (id, item) in connectors.iter() {
                let Some(src) = item.as_table() else {
                    continue;
                };
                let dest = table_mut(desktop, &["agent", "connectors", id]);
                for (key, value) in src.iter() {
                    if key == "enabled" {
                        continue;
                    }
                    dest.insert(key, value.clone());
                }
            }
        }
    }
    if let Some(knowledge) = legacy.get("knowledge") {
        desktop.insert("knowledge", knowledge.clone());
    }
}

fn copy_keys(src: &Table, dest: &mut Table, keys: &[&str]) {
    for key in keys {
        if let Some(value) = src.get(key) {
            dest.insert(key, value.clone());
        }
    }
}

fn table_mut<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> &'a mut Table {
    let mut item = doc.as_item_mut();
    for (index, key) in path.iter().enumerate() {
        let table = item.as_table_mut().expect("config root is a table");
        let next = table.entry(key).or_insert(Item::Table(Table::new()));
        if let Some(table) = next.as_table_mut() {
            if index + 1 < path.len() {
                table.set_implicit(true);
            }
        }
        item = next;
    }
    item.as_table_mut().expect("path is a table")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn imports_allowlist_once_and_skips_secrets_and_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let legacy_dir = home.join(".config").join("coppice");
        let desktop = dir.path().join("Coppice").join("config.toml");
        write(
            &desktop,
            "# keep\n[agent]\nworktrees_path = \"/data/worktrees\"\n",
        );
        write(
            &legacy_dir.join("config.toml"),
            r#"
[database]
url = "postgres://evil/db"

[auth]
session_secret = "stolen"

[secrets]
master_key = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"

[server]
port = 1

[storage]
artifacts_dir = "/tmp/evil"

[web]
port = 9

[git]
author_name = "Imported Ada"
author_email = "ada@import.example"
push_enabled = true

[agent]
worker_count = 7
default_connector = "mock"
worktrees_path = "/evil"

[agent.connectors.cursor]
enabled = false
command = "agent"
run_timeout_secs = 42

[knowledge.retrieval]
top_k = 4
"#,
        );

        let notice = with_config_home(&home, || import_legacy_config(&desktop).expect("import"));

        assert_eq!(notice.as_deref(), Some(LEGACY_IMPORT_NOTICE));
        assert!(!legacy_dir.join("config.toml").exists());
        assert!(legacy_dir.join("config.toml.imported").is_file());

        let text = std::fs::read_to_string(&desktop).unwrap();
        assert!(text.contains("# keep"), "{text}");
        assert!(text.contains("Imported Ada"), "{text}");
        assert!(text.contains("ada@import.example"), "{text}");
        assert!(text.contains("worker_count = 7"), "{text}");
        assert!(
            text.contains("worktrees_path = \"/data/worktrees\""),
            "{text}"
        );
        assert!(text.contains("command = \"agent\""), "{text}");
        assert!(text.contains("run_timeout_secs = 42"), "{text}");
        assert!(text.contains("top_k = 4"), "{text}");
        assert!(!text.contains("enabled"), "{text}");
        assert!(!text.contains("master_key"), "{text}");
        assert!(!text.contains("stolen"), "{text}");
        assert!(!text.contains("postgres://evil"), "{text}");
        assert!(!text.contains("artifacts_dir = \"/tmp/evil\""), "{text}");
        assert!(!text.contains("default_connector"), "{text}");
        assert!(!text.contains("push_enabled"), "{text}");
        for key in DENIED_ROOTS {
            assert!(!text.contains(&format!("[{key}]")), "{key} leaked:\n{text}");
        }

        let again = with_config_home(&home, || import_legacy_config(&desktop).expect("second"));
        assert_eq!(again, None);
    }

    fn with_config_home<T>(home: &Path, body: impl FnOnce() -> T) -> T {
        let prev_home = std::env::var("HOME").ok();
        let prev_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("HOME", home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
        let value = body();
        restore("HOME", prev_home);
        restore("XDG_CONFIG_HOME", prev_xdg);
        value
    }

    fn restore(key: &str, previous: Option<String>) {
        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
