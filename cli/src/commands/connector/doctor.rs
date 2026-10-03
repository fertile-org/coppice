use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use coppice_config::AppConfig;

use coppice_connectors::probe::{probe, ProbeEnv, ProbeOutcome};
use coppice_connectors::MOCK;

use super::registry::{binary_on_path, env_lookup, home_dir, parse_id, search_path};

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Args)]
pub struct DoctorArgs {
    pub id: String,
}

pub fn run(args: DoctorArgs) -> anyhow::Result<()> {
    run_to(args, &mut std::io::stdout())
}

fn run_to(args: DoctorArgs, out: &mut impl Write) -> anyhow::Result<()> {
    let m = parse_id(&args.id)?;
    let id = m.id;
    if id == MOCK {
        writeln!(out, "mock: ok (built-in)")?;
        return Ok(());
    }

    let home = home_dir();
    let mut failed = false;

    writeln!(out, "connector: {}", id)?;
    writeln!(out, "HOME: {}", home.display())?;

    let config = AppConfig::load().ok();
    let command = config.as_ref().and_then(|c| c.agent.connectors.command(id));
    let path = search_path(&home);
    let report = probe(
        m,
        &ProbeEnv {
            home: &home,
            path: &path,
            command_override: command,
            env_lookup: &env_lookup,
        },
        PROBE_TIMEOUT,
    );
    let auth_ok = !report.auth_env_set.is_empty() || !report.auth_paths_found.is_empty();
    let binary_ok = report.binary.is_some();

    match &report.binary {
        Some(path) => writeln!(out, "binary: ok ({})", path.display())?,
        None => {
            writeln!(
                out,
                "binary: MISSING (`{}` not on PATH)",
                command.unwrap_or(m.binary)
            )?;
            writeln!(
                out,
                "  next: coppice connector install {id}  (ensure PATH includes $HOME/.local/bin and $HOME/.opencode/bin)"
            )?;
            failed = true;
        }
    }

    if binary_ok {
        let probe_result = match report.probe {
            ProbeOutcome::Ok { .. } | ProbeOutcome::NotRun => Ok(()),
            ProbeOutcome::Failed { message } => Err(message),
            ProbeOutcome::TimedOut => Err(format!("timed out after {}s", PROBE_TIMEOUT.as_secs())),
        };
        match probe_result {
            Ok(()) => {
                writeln!(out, "models probe: ok")?;
                if auth_ok {
                    writeln!(out, "auth: ok ({})", m.install.auth_hint)?;
                } else if report.probe_proves_auth {
                    writeln!(out, "auth: ok (via models/auth probe)")?;
                } else {
                    writeln!(out, "auth: MISSING")?;
                    writeln!(out, "  next: coppice connector setup {id}")?;
                    writeln!(out, "  hint: {}", m.install.auth_hint)?;
                    failed = true;
                }
            }
            Err(e) => {
                if auth_ok {
                    writeln!(out, "auth: ok ({})", m.install.auth_hint)?;
                    writeln!(out, "models probe: FAIL ({e})")?;
                    failed = true;
                } else {
                    writeln!(out, "auth: MISSING")?;
                    writeln!(out, "  next: coppice connector setup {id}")?;
                    writeln!(out, "  hint: {}", m.install.auth_hint)?;
                    writeln!(out, "models probe: FAIL ({e})")?;
                    failed = true;
                }
            }
        }
    } else if auth_ok {
        writeln!(out, "auth: ok ({})", m.install.auth_hint)?;
    } else {
        writeln!(out, "auth: MISSING")?;
        writeln!(out, "  next: coppice connector setup {id}")?;
        writeln!(out, "  hint: {}", m.install.auth_hint)?;
        failed = true;
    }

    warn_if_makefile_without_make(out)?;

    if failed {
        anyhow::bail!("doctor failed for {id}");
    }
    writeln!(out, "doctor: ok")?;
    Ok(())
}

fn warn_if_makefile_without_make(out: &mut impl Write) -> std::io::Result<()> {
    if binary_on_path("make").is_some() {
        return Ok(());
    }
    if repo_root_with_makefile().is_some() {
        writeln!(out, "build-tools: WARN (Makefile present but make missing)")?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::super::registry::with_env_vars;
    use super::*;

    fn write_exec(path: &std::path::Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    struct Doctor {
        lines: Vec<String>,
        result: Result<(), String>,
    }

    /// Runs doctor with HOME=`home`, PATH=`home/bin` (scripts written there) and
    /// the given extra env; drops the environment-dependent build-tools line.
    fn doctor(
        id: &str,
        home: &std::path::Path,
        scripts: &[(&str, &str)],
        env: &[(&str, &str)],
    ) -> Doctor {
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let mut vars: Vec<(&str, Option<String>)> = vec![
            ("PATH", Some(bin.display().to_string())),
            ("HOME", Some(home.display().to_string())),
            ("ANTHROPIC_API_KEY", None),
            ("OPENAI_API_KEY", None),
            ("COPPICE_CONFIG", None),
        ];
        for (k, v) in env {
            vars.retain(|(key, _)| key != k);
            vars.push((k, Some((*v).to_string())));
        }
        let mut buf = Vec::new();
        let mut result = Ok(());
        with_env_vars(&vars, || {
            for (name, body) in scripts {
                write_exec(&bin.join(name), body);
            }
            result = run_to(DoctorArgs { id: id.into() }, &mut buf).map_err(|e| e.to_string());
        });
        let lines = String::from_utf8(buf)
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with("build-tools:"))
            .map(str::to_string)
            .collect();
        Doctor { lines, result }
    }

    fn header(id: &str, home: &std::path::Path) -> Vec<String> {
        vec![
            format!("connector: {id}"),
            format!("HOME: {}", home.display()),
        ]
    }

    fn expect(head: Vec<String>, rest: &[&str]) -> Vec<String> {
        head.into_iter()
            .chain(rest.iter().map(|s| (*s).to_string()))
            .collect()
    }

    const CURSOR_MISSING_BINARY: &[&str] = &[
        "binary: MISSING (`agent` not on PATH)",
        "  next: coppice connector install cursor  (ensure PATH includes $HOME/.local/bin and $HOME/.opencode/bin)",
    ];

    #[test]
    fn doctor_output_mock() {
        let dir = tempfile::tempdir().unwrap();
        let d = doctor("mock", dir.path(), &[], &[]);
        assert_eq!(d.lines, vec!["mock: ok (built-in)"]);
        assert_eq!(d.result, Ok(()));
    }

    #[test]
    fn doctor_output_binary_missing_auth_missing() {
        let dir = tempfile::tempdir().unwrap();
        let d = doctor("cursor", dir.path(), &[], &[]);
        let mut rest = CURSOR_MISSING_BINARY.to_vec();
        rest.extend([
            "auth: MISSING",
            "  next: coppice connector setup cursor",
            "  hint: agent login (copy URL)",
        ]);
        assert_eq!(d.lines, expect(header("cursor", dir.path()), &rest));
        assert_eq!(d.result, Err("doctor failed for cursor".into()));
    }

    #[test]
    fn doctor_output_binary_missing_auth_ok() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".cursor")).unwrap();
        std::fs::write(dir.path().join(".cursor/auth.json"), "{}").unwrap();
        let d = doctor("cursor", dir.path(), &[], &[]);
        let mut rest = CURSOR_MISSING_BINARY.to_vec();
        rest.push("auth: ok (agent login (copy URL))");
        assert_eq!(d.lines, expect(header("cursor", dir.path()), &rest));
        assert_eq!(d.result, Err("doctor failed for cursor".into()));
    }

    #[test]
    fn doctor_output_probe_proves_auth() {
        let dir = tempfile::tempdir().unwrap();
        let d = doctor(
            "cursor",
            dir.path(),
            &[(
                "agent",
                "[ \"$1\" = models ] && echo gpt-5 && exit 0; exit 1",
            )],
            &[],
        );
        let binary = format!("binary: ok ({})", dir.path().join("bin/agent").display());
        assert_eq!(
            d.lines,
            expect(
                header("cursor", dir.path()),
                &[
                    &binary,
                    "models probe: ok",
                    "auth: ok (via models/auth probe)",
                    "doctor: ok",
                ]
            )
        );
        assert_eq!(d.result, Ok(()));
    }

    #[test]
    fn doctor_output_version_ok_auth_missing() {
        let dir = tempfile::tempdir().unwrap();
        let d = doctor("claude-code", dir.path(), &[("claude", "echo 2.1.0")], &[]);
        let binary = format!("binary: ok ({})", dir.path().join("bin/claude").display());
        assert_eq!(
            d.lines,
            expect(
                header("claude-code", dir.path()),
                &[
                    &binary,
                    "models probe: ok",
                    "auth: MISSING",
                    "  next: coppice connector setup claude-code",
                    "  hint: ANTHROPIC_API_KEY or claude setup-token",
                ]
            )
        );
        assert_eq!(d.result, Err("doctor failed for claude-code".into()));
    }

    #[test]
    fn doctor_output_version_ok_auth_env() {
        let dir = tempfile::tempdir().unwrap();
        let d = doctor(
            "claude-code",
            dir.path(),
            &[("claude", "echo 2.1.0")],
            &[("ANTHROPIC_API_KEY", "sk-test")],
        );
        let binary = format!("binary: ok ({})", dir.path().join("bin/claude").display());
        assert_eq!(
            d.lines,
            expect(
                header("claude-code", dir.path()),
                &[
                    &binary,
                    "models probe: ok",
                    "auth: ok (ANTHROPIC_API_KEY or claude setup-token)",
                    "doctor: ok",
                ]
            )
        );
        assert_eq!(d.result, Ok(()));
    }

    #[test]
    fn doctor_output_probe_fails_auth_missing_uses_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let d = doctor("cursor", dir.path(), &[("agent", "echo no; exit 1")], &[]);
        let binary = format!("binary: ok ({})", dir.path().join("bin/agent").display());
        assert_eq!(
            d.lines,
            expect(
                header("cursor", dir.path()),
                &[
                    &binary,
                    "auth: MISSING",
                    "  next: coppice connector setup cursor",
                    "  hint: agent login (copy URL)",
                    "models probe: FAIL (no)",
                ]
            )
        );
        assert_eq!(d.result, Err("doctor failed for cursor".into()));
    }

    #[test]
    fn doctor_output_probe_fails_auth_ok_prefers_stderr() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".cursor")).unwrap();
        std::fs::write(dir.path().join(".cursor/auth.json"), "{}").unwrap();
        let d = doctor(
            "cursor",
            dir.path(),
            &[("agent", "echo out; echo boom >&2; exit 2")],
            &[],
        );
        let binary = format!("binary: ok ({})", dir.path().join("bin/agent").display());
        assert_eq!(
            d.lines,
            expect(
                header("cursor", dir.path()),
                &[
                    &binary,
                    "auth: ok (agent login (copy URL))",
                    "models probe: FAIL (boom)",
                ]
            )
        );
        assert_eq!(d.result, Err("doctor failed for cursor".into()));
    }

    #[test]
    fn doctor_output_probe_fails_without_output() {
        let dir = tempfile::tempdir().unwrap();
        let d = doctor("cursor", dir.path(), &[("agent", "exit 3")], &[]);
        assert_eq!(
            d.lines.last().unwrap(),
            "models probe: FAIL (command failed)"
        );
        assert_eq!(d.result, Err("doctor failed for cursor".into()));
    }

    #[test]
    fn doctor_uses_configured_command() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("coppice.toml");
        std::fs::write(
            &config,
            "[agent.connectors.cursor]\ncommand = \"my-agent\"\n",
        )
        .unwrap();
        let config = config.display().to_string();
        let env = [("COPPICE_CONFIG", config.as_str())];

        let d = doctor("cursor", dir.path(), &[("agent", "exit 0")], &env);
        assert_eq!(d.lines[2], "binary: MISSING (`my-agent` not on PATH)");

        let d = doctor("cursor", dir.path(), &[("my-agent", "echo gpt-5")], &env);
        let binary = format!("binary: ok ({})", dir.path().join("bin/my-agent").display());
        assert_eq!(d.lines[2], binary);
        assert_eq!(d.result, Ok(()));
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
            assert!(binary_on_path("make").is_none());
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
