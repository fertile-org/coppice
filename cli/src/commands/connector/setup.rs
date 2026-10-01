use std::process::{Command, Stdio};

use clap::Args;

use coppice_connectors::{CLAUDE_CODE, CODEX, CURSOR, KILO_CODE, MOCK, OPENCODE};

use super::registry::{binary_on_path, parse_id};

#[derive(Args)]
pub struct SetupArgs {
    pub id: String,
}

pub fn run(args: SetupArgs) -> anyhow::Result<()> {
    let m = parse_id(&args.id)?;
    let id = m.id;
    if id == MOCK {
        println!("mock needs no setup");
        return Ok(());
    }

    if binary_on_path(m.binary).is_none() {
        anyhow::bail!(
            "`{}` not on PATH. Run: coppice connector install {id}",
            m.binary
        );
    }

    match id {
        CURSOR => {
            println!("Running `agent login` — copy any URL into a browser on your machine.");
            run_interactive(m.binary, &["login"])?;
        }
        CODEX => {
            println!("Running `codex login --device-auth`.");
            run_interactive(m.binary, &["login", "--device-auth"])?;
        }
        CLAUDE_CODE => {
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
        OPENCODE => {
            println!("Running `opencode auth login`.");
            run_interactive(m.binary, &["auth", "login"])?;
        }
        KILO_CODE => {
            println!("Kilo: try `kilo auth login` or open the TUI and use /connect.");
            // Best-effort; vendor UX varies.
            if run_interactive(m.binary, &["auth", "login"]).is_err() {
                println!(
                    "Interactive auth login unavailable. Run `{bin}` in a TTY and use /connect, or paste a vendor auth URL.",
                    bin = m.binary
                );
            }
        }
        _ => {}
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
