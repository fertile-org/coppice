use clap::Args;
use coppice_config::AppConfig;

use coppice_connectors::MOCK;

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

        let enabled = match config.as_ref() {
            None => "no",
            Some(c) => match c.agent.connectors.enabled(meta.id) {
                Some(true) => "yes",
                Some(false) => "no",
                None => "unknown",
            },
        };

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
            meta.id, enabled, binary, auth, meta.install.auth_hint
        );
    }

    println!();
    println!("HOME={}", home.display());
    println!("Next: coppice connector install <id> && coppice connector setup <id>");
    Ok(())
}
