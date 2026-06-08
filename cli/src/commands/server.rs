use clap::Args;
use coppice_config::AppConfig;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Args)]
pub struct ServerStartArgs {}

pub fn run(_args: ServerStartArgs) -> anyhow::Result<()> {
    let config = AppConfig::load().map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
    let server_bin = resolve_server_binary()?;

    println!(
        "starting {} (port {})",
        server_bin.display(),
        config.server.port
    );

    let status = Command::new(&server_bin)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("coppice-server exited with {status}");
    }
}

fn resolve_server_binary() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("COPPICE_SERVER_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        anyhow::bail!("COPPICE_SERVER_BIN is not a file: {}", path.display());
    }

    let current = std::env::current_exe()?;
    let sibling = current
        .parent()
        .map(|dir| dir.join("coppice-server"))
        .filter(|path| path.is_file());
    if let Some(path) = sibling {
        return Ok(path);
    }

    which_server_binary()
}

fn which_server_binary() -> anyhow::Result<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("coppice-server");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "coppice-server not found. Set COPPICE_SERVER_BIN or install coppice-server next to the coppice CLI."
    )
}
