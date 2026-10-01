use clap::Args;
use coppice_config::AppConfig;

use coppice_connectors::{CLAUDE_CODE, CODEX, CURSOR, KILO_CODE, MOCK, OPENCODE};

use super::registry::{auth_present, binary_on_path, home_dir};

#[derive(Args)]
pub struct ListArgs {}

pub fn run(_args: ListArgs) -> anyhow::Result<()> {
    let config = AppConfig::load().ok();
    let home = home_dir();

    println!(
        "{:<14} {:<8} {:<10} {:<8} HINT",
        "ID", "ENABLED", "BINARY", "AUTH"
    );
    for meta in coppice_connectors::all() {
        if meta.id == MOCK {
            println!(
                "{:<14} {:<8} {:<10} {:<8} {}",
                meta.id, "yes", "n/a", "n/a", meta.install.auth_hint
            );
            continue;
        }

        let enabled = config
            .as_ref()
            .map(|c| match meta.id {
                CURSOR => c.agent.connectors.cursor.enabled,
                CLAUDE_CODE => c.agent.connectors.claude_code.enabled,
                CODEX => c.agent.connectors.codex.enabled,
                KILO_CODE => c.agent.connectors.kilo_code.enabled,
                OPENCODE => c.agent.connectors.opencode.enabled,
                _ => false,
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
            meta.id,
            if enabled { "yes" } else { "no" },
            binary,
            auth,
            meta.install.auth_hint
        );
    }

    println!();
    println!("HOME={}", home.display());
    println!("Next: coppice connector install <id> && coppice connector setup <id>");
    Ok(())
}
