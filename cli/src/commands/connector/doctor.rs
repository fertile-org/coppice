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

    let auth_ok = auth_present(m, &home);
    let binary_path = binary_on_path(m.binary);
    let binary_ok = binary_path.is_some();

    match binary_path {
        Some(path) => println!("binary: ok ({})", path.display()),
        None => {
            println!(
                "binary: MISSING (`{}` not on PATH)",
                m.binary
            );
            println!(
                "  next: coppice connector install {id}  (ensure PATH includes $HOME/.local/bin and $HOME/.opencode/bin)"
            );
            failed = true;
        }
    }

    if binary_ok {
        match probe_models(id, m.binary) {
            Ok(()) => {
                println!("models probe: ok");
                if auth_ok {
                    println!("auth: ok ({})", m.auth_hint);
                } else if probe_proves_auth(id) {
                    println!("auth: ok (via models/auth probe)");
                } else {
                    println!("auth: MISSING");
                    println!("  next: coppice connector setup {id}");
                    println!("  hint: {}", m.auth_hint);
                    failed = true;
                }
            }
            Err(e) => {
                if auth_ok {
                    println!("auth: ok ({})", m.auth_hint);
                    println!("models probe: FAIL ({e})");
                    failed = true;
                } else {
                    println!("auth: MISSING");
                    println!("  next: coppice connector setup {id}");
                    println!("  hint: {}", m.auth_hint);
                    println!("models probe: FAIL ({e})");
                    failed = true;
                }
            }
        }
    } else if auth_ok {
        println!("auth: ok ({})", m.auth_hint);
    } else {
        println!("auth: MISSING");
        println!("  next: coppice connector setup {id}");
        println!("  hint: {}", m.auth_hint);
        failed = true;
    }

    if failed {
        anyhow::bail!("doctor failed for {id}");
    }
    println!("doctor: ok");
    Ok(())
}

fn probe_proves_auth(id: ConnectorId) -> bool {
    matches!(id, ConnectorId::Cursor | ConnectorId::OpenCode)
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
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env_vars(vars: &[(&str, Option<String>)], f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let saved: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(key, _)| {
                (
                    (*key).to_string(),
                    std::env::var(*key).ok(),
                )
            })
            .collect();
        for (key, val) in vars {
            match val {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        f();
        for (key, prev) in saved {
            match prev {
                Some(v) => std::env::set_var(&key, v),
                None => std::env::remove_var(&key),
            }
        }
    }

    #[test]
    fn probe_proves_auth_only_for_cursor_and_opencode() {
        assert!(probe_proves_auth(ConnectorId::Cursor));
        assert!(probe_proves_auth(ConnectorId::OpenCode));
        assert!(!probe_proves_auth(ConnectorId::ClaudeCode));
        assert!(!probe_proves_auth(ConnectorId::Codex));
        assert!(!probe_proves_auth(ConnectorId::KiloCode));
    }

    #[test]
    fn doctor_fails_when_claude_version_ok_but_auth_missing() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let claude = bin.join("claude");
        std::fs::write(&claude, "#!/bin/sh\n[ \"$1\" = \"--version\" ] && exit 0\nexit 1\n")
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&claude).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&claude, perms).unwrap();
        }
        with_env_vars(
            &[
                ("PATH", Some(bin.display().to_string())),
                ("HOME", Some(dir.path().display().to_string())),
                ("ANTHROPIC_API_KEY", None),
                ("OPENAI_API_KEY", None),
            ],
            || {
                let err = run(DoctorArgs {
                    id: "claude-code".into(),
                });
                assert!(err.is_err());
            },
        );
    }

    #[test]
    fn doctor_fails_when_binary_missing() {
        let dir = tempfile::tempdir().unwrap();
        with_env_vars(
            &[
                ("PATH", Some(dir.path().display().to_string())),
                ("HOME", Some(dir.path().display().to_string())),
                ("ANTHROPIC_API_KEY", None),
                ("OPENAI_API_KEY", None),
            ],
            || {
                let err = run(DoctorArgs {
                    id: "cursor".into(),
                });
                assert!(err.is_err());
            },
        );
    }

    #[test]
    fn doctor_fails_when_auth_missing_even_if_binary_exists() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let agent = bin.join("agent");
        std::fs::write(&agent, "#!/bin/sh\necho no\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&agent).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&agent, perms).unwrap();
        }
        with_env_vars(
            &[
                ("PATH", Some(bin.display().to_string())),
                ("HOME", Some(dir.path().display().to_string())),
                ("ANTHROPIC_API_KEY", None),
                ("OPENAI_API_KEY", None),
            ],
            || {
                let err = run(DoctorArgs {
                    id: "cursor".into(),
                });
                assert!(err.is_err());
            },
        );
    }
}
