//! Environment markers an agent CLI must not inherit.
//!
//! When Coppice is started from a terminal that is itself inside Claude Code,
//! the agent it launches would see Claude Code's own session markers and act
//! as a nested or resumed session. Only those internal markers are removed.
//! Every other `CLAUDE_CODE_*` variable is a user setting (for example
//! `CLAUDE_CODE_USE_BEDROCK` or `CLAUDE_CODE_GIT_BASH_PATH`) and stays.
//!
//! The inherited `PWD` is replaced too: it names the server's own folder
//! (`$HOME` under the desktop app), and some CLIs trust it over the real
//! working directory.

const EXACT: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SSE_PORT",
];

/// `CLAUDECODE_*` (no underscore between the words) is internal only.
const PREFIX: &str = "CLAUDECODE_";

pub fn is_inherited_marker(key: &str) -> bool {
    EXACT.contains(&key) || key.starts_with(PREFIX)
}

/// Removes the markers this process has from `cmd`'s inherited environment.
pub fn scrub(cmd: &mut tokio::process::Command) {
    for (key, _) in std::env::vars_os() {
        if let Some(key) = key.to_str() {
            if is_inherited_marker(key) {
                cmd.env_remove(key);
            }
        }
    }
}

/// Runs `cmd` in `dir` with a `PWD` that says so. OpenCode 2.0.20 resolves
/// relative paths against `PWD`, so with the server's `PWD` it read
/// `.agent/context.md` from `$HOME` instead of the worktree.
pub fn work_in(cmd: &mut tokio::process::Command, dir: &std::path::Path) {
    cmd.current_dir(dir).env("PWD", dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_session_markers_and_keeps_user_settings() {
        for key in [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_EXECPATH",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CODE_SSE_PORT",
            "CLAUDECODE_ANYTHING",
        ] {
            assert!(is_inherited_marker(key), "{key}");
        }
        for key in [
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_GIT_BASH_PATH",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            "CLAUDE_CODE_TMPDIR",
            "ANTHROPIC_API_KEY",
            "PATH",
        ] {
            assert!(!is_inherited_marker(key), "{key}");
        }
    }

    #[tokio::test]
    async fn child_pwd_is_its_working_directory() {
        // `printenv`, not a shell: `sh` resets a stale `PWD` itself.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut cmd = tokio::process::Command::new("printenv");
        cmd.arg("PWD").env("PWD", "/somewhere/else");
        work_in(&mut cmd, dir.path());
        let output = cmd.output().await.expect("printenv");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            dir.path().display().to_string()
        );
    }
}
