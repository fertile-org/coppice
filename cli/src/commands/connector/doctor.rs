use std::process::Command;

use clap::Args;

use super::registry::{
    auth_present, binary_on_path, home_dir, meta, parse_id, ConnectorId,
};

#[derive(Args)]
pub struct DoctorArgs {
    pub id: String,
}

pub fn run(args: DoctorArgs) -> anyhow::Result<()> {
    let id = parse_id(&args.id)?;
    if id == ConnectorId::Mock {
        println!("mock: ok (built-in)");
        return Ok(());
    }

    let m = meta(id);
    let home = home_dir();
    let mut failed = false;

    println!("connector: {}", id);
    println!("HOME: {}", home.display());

    match binary_on_path(m.binary) {
        Some(path) => println!("binary: ok ({})", path.display()),
        None => {
            println!(
                "binary: MISSING (`{}` not on PATH)",
                m.binary
            );
            println!(
                "  next: coppice connector install {id}  (or install `{bin}` into $HOME/.local/bin)",
                id = id,
                bin = m.binary
            );
            failed = true;
        }
    }

    if auth_present(m, &home) {
        println!("auth: ok ({})", m.auth_hint);
    } else {
        println!("auth: MISSING");
        println!("  next: coppice connector setup {id}");
        println!("  hint: {}", m.auth_hint);
        failed = true;
    }

    if !failed {
        if let Err(e) = probe_models(id, m.binary) {
            println!("models probe: WARN ({e})");
            // Auth/binary present but models failed — still non-zero so operators notice.
            failed = true;
        } else {
            println!("models probe: ok");
        }
    }

    if failed {
        anyhow::bail!("doctor failed for {id}");
    }
    println!("doctor: ok");
    Ok(())
}

fn probe_models(id: ConnectorId, binary: &str) -> anyhow::Result<()> {
    let mut cmd = match id {
        ConnectorId::Cursor => {
            let mut c = Command::new(binary);
            c.arg("models");
            c
        }
        ConnectorId::ClaudeCode => {
            let mut c = Command::new(binary);
            c.arg("--version");
            c
        }
        ConnectorId::Codex => {
            let mut c = Command::new(binary);
            c.arg("--version");
            c
        }
        ConnectorId::KiloCode => {
            let mut c = Command::new(binary);
            c.arg("--version");
            c
        }
        ConnectorId::OpenCode => {
            let mut c = Command::new(binary);
            c.args(["auth", "list"]);
            c
        }
        ConnectorId::Mock => return Ok(()),
    };

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let msg = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        anyhow::bail!("{}", if msg.is_empty() { "command failed".into() } else { msg });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_fails_when_binary_missing() {
        // Use a fake PATH so `agent` cannot be found.
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("PATH", dir.path());
        std::env::set_var("HOME", dir.path());
        let err = run(DoctorArgs {
            id: "cursor".into(),
        });
        assert!(err.is_err());
    }
}
