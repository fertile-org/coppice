use std::process::{Command, Stdio};

use clap::Args;

use super::registry::{binary_on_path, meta, parse_id, ConnectorId};

#[derive(Args)]
pub struct SetupArgs {
    pub id: String,
}

pub fn run(args: SetupArgs) -> anyhow::Result<()> {
    let id = parse_id(&args.id)?;
    if id == ConnectorId::Mock {
        println!("mock needs no setup");
        return Ok(());
    }

    let m = meta(id);
    if binary_on_path(m.binary).is_none() {
        anyhow::bail!(
            "`{}` not on PATH. Run: coppice connector install {id}",
            m.binary
        );
    }

    match id {
        ConnectorId::Cursor => {
            println!("Running `agent login` — copy any URL into a browser on your machine.");
            run_interactive(m.binary, &["login"])?;
        }
        ConnectorId::Codex => {
            println!("Running `codex login --device-auth`.");
            run_interactive(m.binary, &["login", "--device-auth"])?;
        }
        ConnectorId::ClaudeCode => {
            println!("Claude in Docker: prefer ANTHROPIC_API_KEY or setup-token (browser OAuth is unreliable).");
            if std::env::var_os("ANTHROPIC_API_KEY").is_some_and(|v| !v.is_empty()) {
                println!("ANTHROPIC_API_KEY is set; skipping interactive login.");
                return Ok(());
            }
            println!("Trying `claude setup-token` (paste token when prompted).");
            match run_interactive(m.binary, &["setup-token"]) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("setup-token failed ({e}); trying `claude login`.");
                    run_interactive(m.binary, &["login"])?;
                }
            }
        }
        ConnectorId::OpenCode => {
            println!("Running `opencode auth login`.");
            run_interactive(m.binary, &["auth", "login"])?;
        }
        ConnectorId::KiloCode => {
            println!("Kilo: try `kilo auth login` or open the TUI and use /connect.");
            // Best-effort; vendor UX varies.
            if run_interactive(m.binary, &["auth", "login"]).is_err() {
                println!(
                    "Interactive auth login unavailable. Run `{bin}` in a TTY and use /connect, or paste a vendor auth URL.",
                    bin = m.binary
                );
            }
        }
        ConnectorId::Mock => {}
    }

    println!("setup finished for {id}. Run: coppice connector doctor {id}");
    Ok(())
}

fn run_interactive(bin: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(bin)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if !status.success() {
        anyhow::bail!("{bin} {:?} exited with {status}", args);
    }
    Ok(())
}
