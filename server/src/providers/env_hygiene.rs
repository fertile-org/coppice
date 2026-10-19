//! Environment markers an agent CLI must not inherit.
//!
//! When Coppice is started from a terminal that is itself inside Claude Code,
//! the agent it launches would see Claude Code's own session markers and act
//! as a nested or resumed session. Only those internal markers are removed.
//! Every other `CLAUDE_CODE_*` variable is a user setting (for example
//! `CLAUDE_CODE_USE_BEDROCK` or `CLAUDE_CODE_GIT_BASH_PATH`) and stays.

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
}
