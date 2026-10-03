use std::io::Write;

use clap::Args;
use coppice_config::AppConfig;

use coppice_connectors::probe::resolve_binary;
use coppice_connectors::MOCK;

use super::registry::{auth_present, home_dir, search_path};

#[derive(Args)]
pub struct ListArgs {}

pub fn run(_args: ListArgs) -> anyhow::Result<()> {
    run_to(&mut std::io::stdout())
}

fn run_to(out: &mut impl Write) -> anyhow::Result<()> {
    let config = AppConfig::load().ok();
    let home = home_dir();
    let path = search_path(&home);

    writeln!(
        out,
        "{:<14} {:<8} {:<10} {:<8} HINT",
        "ID", "ENABLED", "BINARY", "AUTH"
    )?;
    for meta in coppice_connectors::all() {
        if meta.id == MOCK {
            writeln!(
                out,
                "{:<14} {:<8} {:<10} {:<8} {}",
                meta.id, "yes", "n/a", "n/a", meta.install.auth_hint
            )?;
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

        let name = config
            .as_ref()
            .and_then(|c| c.agent.connectors.command(meta.id))
            .unwrap_or(meta.binary);
        let binary = if resolve_binary(name, &path).is_some() {
            "ok"
        } else {
            "missing"
        };
        let auth = if auth_present(meta, &home) {
            "ok"
        } else {
            "missing"
        };

        writeln!(
            out,
            "{:<14} {:<8} {:<10} {:<8} {}",
            meta.id, enabled, binary, auth, meta.install.auth_hint
        )?;
    }

    writeln!(out)?;
    writeln!(out, "HOME={}", home.display())?;
    writeln!(
        out,
        "Next: coppice connector install <id> && coppice connector setup <id>"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::registry::with_env_vars;
    use super::*;

    #[test]
    fn list_output_rows() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("claude"), "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin.join("claude"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        std::fs::create_dir_all(dir.path().join(".codex")).unwrap();
        std::fs::write(dir.path().join(".codex/auth.json"), "{}").unwrap();
        let mut buf = Vec::new();
        let mut result = Ok(());
        with_env_vars(
            &[
                ("PATH", Some(bin.display().to_string())),
                ("HOME", Some(dir.path().display().to_string())),
                ("ANTHROPIC_API_KEY", None),
                ("OPENAI_API_KEY", None),
            ],
            || result = run_to(&mut buf),
        );
        result.unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "ID             ENABLED  BINARY     AUTH     HINT");
        for want in [
            "mock           yes      n/a        n/a      built-in; no setup",
            "cursor         no       missing    missing  agent login (copy URL)",
            "claude-code    no       ok         missing  ANTHROPIC_API_KEY or claude setup-token",
            "codex          no       missing    ok       codex login --device-auth",
        ] {
            assert!(lines.contains(&want), "missing row {want:?} in:\n{text}");
        }
        let n = lines.len();
        assert_eq!(lines[n - 3], "");
        assert_eq!(lines[n - 2], format!("HOME={}", dir.path().display()));
        assert_eq!(
            lines[n - 1],
            "Next: coppice connector install <id> && coppice connector setup <id>"
        );
    }
}
