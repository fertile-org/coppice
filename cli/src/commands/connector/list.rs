use clap::Args;
use coppice_config::AppConfig;

use super::registry::{auth_present, binary_on_path, home_dir, CONNECTORS};

#[derive(Args)]
pub struct ListArgs {}

pub fn run(_args: ListArgs) -> anyhow::Result<()> {
    let config = AppConfig::load().ok();
    let home = home_dir();

    println!(
        "{:<14} {:<8} {:<10} {:<8} HINT",
        "ID", "ENABLED", "BINARY", "AUTH"
    );
    for meta in CONNECTORS {
        if meta.id == super::ConnectorId::Mock {
            println!(
                "{:<14} {:<8} {:<10} {:<8} {}",
                meta.id.as_str(),
                "yes",
                "n/a",
                "n/a",
                meta.auth_hint
            );
            continue;
        }

        let enabled = config
            .as_ref()
            .map(|c| match meta.id {
                super::ConnectorId::Cursor => c.agent.connectors.cursor.enabled,
                super::ConnectorId::ClaudeCode => c.agent.connectors.claude_code.enabled,
                super::ConnectorId::Codex => c.agent.connectors.codex.enabled,
                super::ConnectorId::KiloCode => c.agent.connectors.kilo_code.enabled,
                super::ConnectorId::OpenCode => c.agent.connectors.opencode.enabled,
                super::ConnectorId::Mock => true,
            })
            .unwrap_or(false);

        let binary = if binary_on_path(meta.binary).is_some() {
            "ok"
        } else {
            "missing"
        };
        let auth = if auth_present(meta, &home) {
            "ok"
        } else {
            "missing"
        };

        println!(
            "{:<14} {:<8} {:<10} {:<8} {}",
            meta.id.as_str(),
            if enabled { "yes" } else { "no" },
            binary,
            auth,
            meta.auth_hint
        );
    }

    println!();
    println!("HOME={}", home.display());
    println!("Next: coppice connector install <id> && coppice connector setup <id>");
    Ok(())
}
