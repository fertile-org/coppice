use std::path::PathBuf;
use std::process::Command;

use clap::Args;

use coppice_connectors::{CLAUDE_CODE, CODEX, CURSOR, KILO_CODE, MOCK, OPENCODE};

use super::registry::{auth_present, binary_on_path, home_dir, parse_id};

#[derive(Args)]
pub struct DoctorArgs {
    pub id: String,
}

pub fn run(args: DoctorArgs) -> anyhow::Result<()> {
    let m = parse_id(&args.id)?;
    let id = m.id;
    if id == MOCK {
        println!("mock: ok (built-in)");
        return Ok(());
    }

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
            println!("binary: MISSING (`{}` not on PATH)", m.binary);
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
                    println!("auth: ok ({})", m.install.auth_hint);
                } else if probe_proves_auth(id) {
                    println!("auth: ok (via models/auth probe)");
                } else {
                    println!("auth: MISSING");
                    println!("  next: coppice connector setup {id}");
                    println!("  hint: {}", m.install.auth_hint);
                    failed = true;
                }
            }
            Err(e) => {
                if auth_ok {
                    println!("auth: ok ({})", m.install.auth_hint);
                    println!("models probe: FAIL ({e})");
                    failed = true;
                } else {
                    println!("auth: MISSING");
                    println!("  next: coppice connector setup {id}");
                    println!("  hint: {}", m.install.auth_hint);
                    println!("models probe: FAIL ({e})");
                    failed = true;
                }
            }
        }
    } else if auth_ok {
        println!("auth: ok ({})", m.install.auth_hint);
    } else {
        println!("auth: MISSING");
        println!("  next: coppice connector setup {id}");
        println!("  hint: {}", m.install.auth_hint);
        failed = true;
    }

    warn_if_makefile_without_make();

    if failed {
        anyhow::bail!("doctor failed for {id}");
    }
    println!("doctor: ok");
    Ok(())
}

fn warn_if_makefile_without_make() {
    if which::which("make").is_ok() {
        return;
    }
    if repo_root_with_makefile().is_some() {
        println!("build-tools: WARN (Makefile present but make missing)");
    }
}

fn repo_root_with_makefile() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    let mut dir = Some(start.as_path());
    while let Some(current) = dir {
        if current.join(".git").exists() {
            if current.join("Makefile").is_file() {
                return Some(current.to_path_buf());
            }
            return None;
        }
        dir = current.parent();
    }
    start.join("Makefile").is_file().then_some(start)
}

fn probe_proves_auth(id: &str) -> bool {
    matches!(id, CURSOR | OPENCODE)
}

fn probe_models(id: &str, binary: &str) -> anyhow::Result<()> {
    let mut cmd = match id {
        CURSOR => {
            let mut c = Command::new(binary);
            c.arg("models");
            c
        }
        CLAUDE_CODE => {
            let mut c = Command::new(binary);
            c.arg("--version");
            c
        }
        CODEX => {
            let mut c = Command::new(binary);
            c.arg("--version");
            c
        }
        KILO_CODE => {
            let mut c = Command::new(binary);
            c.arg("--version");
            c
        }
        OPENCODE => {
            let mut c = Command::new(binary);
            c.args(["auth", "list"]);
            c
        }
        _ => return Ok(()),
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
        anyhow::bail!(
            "{}",
            if msg.is_empty() {
                "command failed".into()
            } else {
                msg
            }
        );
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
            .map(|(key, _)| ((*key).to_string(), std::env::var(*key).ok()))
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
        assert!(probe_proves_auth(CURSOR));
        assert!(probe_proves_auth(OPENCODE));
        assert!(!probe_proves_auth(CLAUDE_CODE));
        assert!(!probe_proves_auth(CODEX));
        assert!(!probe_proves_auth(KILO_CODE));
    }

    #[test]
    fn doctor_fails_when_claude_version_ok_but_auth_missing() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let claude = bin.join("claude");
        std::fs::write(
            &claude,
            "#!/bin/sh\n[ \"$1\" = \"--version\" ] && exit 0\nexit 1\n",
        )
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

    #[test]
    fn doctor_warns_when_makefile_present_but_make_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Makefile"), "test:\n").unwrap();
        let empty_bin = dir.path().join("empty-bin");
        std::fs::create_dir_all(&empty_bin).unwrap();
        with_env_vars(&[("PATH", Some(empty_bin.display().to_string()))], || {
            let prev_cwd = std::env::current_dir().unwrap();
            std::env::set_current_dir(dir.path()).unwrap();
            assert!(repo_root_with_makefile().is_some());
            assert!(which::which("make").is_err());
            std::env::set_current_dir(prev_cwd).unwrap();
        });
    }

    #[test]
    fn repo_root_with_makefile_finds_git_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Makefile"), "test:\n").unwrap();
        std::fs::create_dir_all(dir.path().join("server")).unwrap();
        with_env_vars(&[], || {
            let prev_cwd = std::env::current_dir().unwrap();
            std::env::set_current_dir(dir.path().join("server")).unwrap();
            let found = repo_root_with_makefile();
            std::env::set_current_dir(prev_cwd).unwrap();
            assert_eq!(found.as_deref(), Some(dir.path()));
        });
    }
}
