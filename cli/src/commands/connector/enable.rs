use std::path::{Path, PathBuf};

use clap::Args;
use toml_edit::{value, Array, DocumentMut, Item, Table};

use super::registry::{meta, parse_id, ConnectorId};

#[derive(Args)]
pub struct EnableArgs {
    /// Connector id (cursor, claude-code, codex, kilo-code, opencode)
    pub id: String,
    /// Config file to patch (default: COPPICE_CONFIG, else ./config.toml, else global)
    #[arg(long)]
    pub config: Option<PathBuf>,
}

pub fn run(args: EnableArgs) -> anyhow::Result<()> {
    let id = parse_id(&args.id)?;
    if id == ConnectorId::Mock {
        println!("mock is always available; nothing to enable");
        return Ok(());
    }

    let path = resolve_config_path(args.config.as_deref())?;
    let text = if path.is_file() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };

    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| anyhow::anyhow!("invalid TOML in {}: {e}", path.display()))?;

    enable_in_doc(&mut doc, id)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, doc.to_string())?;
    println!("enabled {} in {}", id, path.display());
    println!("Restart the server (or recreate the Compose service) to pick up config changes.");
    Ok(())
}

pub fn resolve_config_path(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    if let Ok(p) = std::env::var("COPPICE_CONFIG") {
        return Ok(PathBuf::from(p));
    }
    let local = coppice_config::AppConfig::local_config_path();
    if local.is_file() {
        return Ok(local);
    }
    // Docker deploy layout when run from repo / typical operator cwd.
    let deploy = PathBuf::from("deploy/config/config.toml");
    if deploy.is_file() {
        return Ok(deploy);
    }
    Ok(local)
}

pub fn enable_in_doc(doc: &mut DocumentMut, id: ConnectorId) -> anyhow::Result<()> {
    let m = meta(id);
    let agent = doc
        .entry("agent")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("[agent] must be a table"))?;

    let connectors = agent
        .entry("connectors")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("[agent.connectors] must be a table"))?;
    connectors.set_implicit(true);

    let table = connectors
        .entry(id.config_key())
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("connector table must be a table"))?;

    table["enabled"] = value(true);

    let needs_providers = match table.get("model_providers") {
        None => true,
        Some(Item::Value(v)) => v
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(true),
        _ => true,
    };
    if needs_providers && !m.default_model_providers.is_empty() {
        let mut arr = Array::new();
        for p in m.default_model_providers {
            arr.push(p.to_string());
        }
        table["model_providers"] = value(arr);
    }

    if matches!(
        id,
        ConnectorId::Cursor | ConnectorId::KiloCode | ConnectorId::OpenCode
    ) && table.get("command").is_none()
    {
        table["command"] = value(m.binary);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_creates_cursor_section() {
        let mut doc = DocumentMut::new();
        enable_in_doc(&mut doc, ConnectorId::Cursor).unwrap();
        let text = doc.to_string();
        assert!(text.contains("[agent.connectors.cursor]"));
        assert!(text.contains("enabled = true"));
        assert!(text.contains("\"cursor\""));
        assert!(text.contains("command = \"agent\""));
    }

    #[test]
    fn enable_is_idempotent() {
        let mut doc = r#"
[agent.connectors.cursor]
enabled = false
command = "agent"
model_providers = ["cursor"]
"#
        .parse::<DocumentMut>()
        .unwrap();
        enable_in_doc(&mut doc, ConnectorId::Cursor).unwrap();
        enable_in_doc(&mut doc, ConnectorId::Cursor).unwrap();
        let table = doc["agent"]["connectors"]["cursor"].as_table().unwrap();
        assert_eq!(table["enabled"].as_bool(), Some(true));
        assert_eq!(
            table["model_providers"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>(),
            vec!["cursor"]
        );
    }

    #[test]
    fn enable_claude_sets_providers() {
        let mut doc = DocumentMut::new();
        enable_in_doc(&mut doc, ConnectorId::ClaudeCode).unwrap();
        let text = doc.to_string();
        assert!(text.contains("[agent.connectors.claude-code]"));
        assert!(text.contains("sonnet"));
    }
}
