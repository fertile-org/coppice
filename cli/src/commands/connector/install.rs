use std::process::{Command, Stdio};

use clap::Args;

use super::registry::{binary_on_path, home_dir, meta, parse_id, ConnectorId};

#[derive(Args)]
pub struct InstallArgs {
    pub id: String,
}

pub fn run(args: InstallArgs) -> anyhow::Result<()> {
    let id = parse_id(&args.id)?;
    if id == ConnectorId::Mock {
        println!("mock needs no install");
        return Ok(());
    }

    let home = home_dir();
    let local_bin = home.join(".local/bin");
    let opencode_bin = home.join(".opencode/bin");
    std::fs::create_dir_all(&local_bin)?;
    std::fs::create_dir_all(&opencode_bin)?;
    let path = std::env::var("PATH").unwrap_or_default();
    let prepended = format!("{}:{}:{path}", local_bin.display(), opencode_bin.display());
    std::env::set_var("PATH", &prepended);
    std::env::set_var("HOME", &home);

    match id {
        ConnectorId::Cursor => install_cursor(&home)?,
        ConnectorId::ClaudeCode => {
            defer_or_hint(
                id,
                "Install Claude Code into this HOME, e.g. follow https://docs.anthropic.com/en/docs/claude-code — binaries should land on $HOME/.local/bin.",
            )?;
        }
        ConnectorId::Codex => {
            defer_or_hint(
                id,
                "Install Codex into this HOME (npm/cargo/vendor script) so `codex` is on $HOME/.local/bin.",
            )?;
        }
        ConnectorId::KiloCode => {
            defer_or_hint(
                id,
                "Install `@kilocode/cli` into this HOME (e.g. npm install -g with prefix under $HOME).",
            )?;
        }
        ConnectorId::OpenCode => install_opencode(&home)?,
        ConnectorId::Mock => {}
    }

    let m = meta(id);
    match binary_on_path(m.binary) {
        Some(p) => println!("install ok: {} -> {}", m.binary, p.display()),
        None => {
            let msg = format!(
                "install finished but `{}` still not on PATH. Ensure PATH includes {} and {}.",
                m.binary,
                local_bin.display(),
                opencode_bin.display()
            );
            if matches!(id, ConnectorId::Cursor | ConnectorId::OpenCode) {
                anyhow::bail!(msg);
            }
            println!("{msg}");
        }
    }
    Ok(())
}

fn install_cursor(home: &std::path::Path) -> anyhow::Result<()> {
    if binary_on_path("agent").is_some() {
        println!("`agent` already on PATH; skipping download");
        return Ok(());
    }
    println!("Installing Cursor Agent CLI into HOME={} …", home.display());
    // Official installer; respects HOME for ~/.local/bin layout.
    let status = Command::new("sh")
        .arg("-c")
        .arg("curl -fsSL https://cursor.com/install | bash")
        .env("HOME", home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run cursor install (need curl?): {e}"))?;
    if !status.success() {
        anyhow::bail!("cursor install script exited with {status}");
    }
    Ok(())
}

fn install_opencode(home: &std::path::Path) -> anyhow::Result<()> {
    let opencode_bin = home.join(".opencode/bin");
    std::fs::create_dir_all(&opencode_bin)?;

    let path = std::env::var("PATH").unwrap_or_default();
    let prepended = format!(
        "{}:{}:{path}",
        home.join(".local/bin").display(),
        opencode_bin.display()
    );
    std::env::set_var("PATH", &prepended);

    if binary_on_path("opencode").is_some() {
        println!("`opencode` already on PATH; skipping download");
        return Ok(());
    }

    println!("Installing OpenCode into HOME={} …", home.display());
    let status = Command::new("sh")
        .arg("-c")
        .arg("curl -fsSL https://opencode.ai/install | bash")
        .env("HOME", home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run opencode install (need curl?): {e}"))?;
    if !status.success() {
        anyhow::bail!("opencode install script exited with {status}");
    }
    Ok(())
}

fn defer_or_hint(id: ConnectorId, hint: &str) -> anyhow::Result<()> {
    let m = meta(id);
    if binary_on_path(m.binary).is_some() {
        println!("`{}` already on PATH", m.binary);
        return Ok(());
    }
    println!("install for `{id}` is not automated yet.");
    println!("{hint}");
    println!("Then: coppice connector setup {id}");
    Ok(())
}
