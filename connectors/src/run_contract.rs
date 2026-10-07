//! Pinned launch flags for one connector.
//!
//! Launch code reads [`RunContract`] and does not hardcode these flags.
//! [`PermissionMode`] is its own field so a later milestone can replace it
//! with a sandbox flag. A version floor for the whole contract belongs here
//! too; this slice only gates individual flags.

/// Where the ticket prompt goes, if it is an argv token at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptPlace {
    /// Insert the prompt immediately after the first `-p`.
    AfterDashP,
    /// Append the prompt after the pinned flags (dynamic flags come after).
    Append,
    /// The prompt is not an argv token (stdin or an HTTP session).
    None,
}

/// One pinned flag (one or more argv tokens) and the CLI version it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedFlag {
    pub args: &'static [&'static str],
    /// Documentary minimum. Used to omit the flag only when [`Self::gate`] is set.
    pub min_version: Option<&'static str>,
    /// When true, pass the flag only if the detected version is at least
    /// [`Self::min_version`]. An unknown version omits a gated flag.
    pub gate: bool,
    pub reason: &'static str,
}

/// Approval / permission mode. Separate from [`RunContract::leading`] and
/// [`RunContract::trailing`] so a later milestone can swap it for a sandbox flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionMode {
    /// Argv for a write-capable run. Empty means this CLI has no such flag.
    pub write: &'static [&'static str],
    /// Argv for a read-only run. `None` means use [`Self::write`].
    pub read_only: Option<&'static [&'static str]>,
    pub min_version: Option<&'static str>,
    pub gate: bool,
    pub reason: &'static str,
}

impl PermissionMode {
    fn args(self, read_only: bool) -> &'static [&'static str] {
        if read_only {
            self.read_only.unwrap_or(self.write)
        } else {
            self.write
        }
    }
}

/// Flags Coppice passes explicitly, plus how the process relates to the worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunContract {
    pub permission_mode: PermissionMode,
    /// Flags before [`Self::permission_mode`].
    pub leading: &'static [PinnedFlag],
    /// Flags after [`Self::permission_mode`].
    pub trailing: &'static [PinnedFlag],
    /// When true, the child process cwd is the ticket worktree.
    pub process_cwd_is_worktree: bool,
    /// When true, a terminal result ends the run before process exit. A short
    /// grace period then stops a process that stays alive.
    pub result_before_exit: bool,
    pub prompt: PromptPlace,
    /// Argv that selects a native plan mode known not to write files or branches.
    /// `None` means planning stays prompt-based and must not invent a plan flag.
    pub plan_mode: Option<&'static [&'static str]>,
}

/// Values substituted into `{worktree}`, `{hostname}`, and `{port}` tokens.
#[derive(Debug, Clone, Copy)]
pub struct LaunchSubst<'a> {
    pub read_only: bool,
    /// When true, replace the permission-mode argv with [`RunContract::plan_mode`]
    /// if that mode is pinned. Otherwise the prompt carries the plan.
    pub plan: bool,
    pub worktree: &'a str,
    pub hostname: &'a str,
    pub port: &'a str,
    /// Detected CLI version. `None` means unknown, which omits gated flags.
    pub version: Option<&'a str>,
}

impl RunContract {
    /// Pinned argv only. Dynamic flags (model, resume, MCP) stay in the launcher.
    pub fn argv(&self, subst: &LaunchSubst<'_>) -> Vec<String> {
        let mut out = Vec::new();
        for flag in self.leading {
            push_flag(&mut out, flag, subst);
        }
        let (permission_args, min_version, gate) = self.permission_args(subst);
        push_tokens(&mut out, permission_args, subst, min_version, gate);
        for flag in self.trailing {
            push_flag(&mut out, flag, subst);
        }
        out
    }

    fn permission_args(self, subst: &LaunchSubst<'_>) -> (&'static [&'static str], Option<&'static str>, bool) {
        if subst.plan {
            if let Some(plan) = self.plan_mode {
                return (plan, None, false);
            }
        }
        (
            self.permission_mode.args(subst.read_only),
            self.permission_mode.min_version,
            self.permission_mode.gate,
        )
    }

    /// Place the prompt according to [`RunContract::prompt`].
    pub fn with_prompt(&self, argv: Vec<String>, prompt: &str) -> Vec<String> {
        match self.prompt {
            PromptPlace::None => argv,
            PromptPlace::Append => {
                let mut argv = argv;
                argv.push(prompt.to_string());
                argv
            }
            PromptPlace::AfterDashP => insert_after_dash_p(argv, prompt),
        }
    }
}

const fn flag(args: &'static [&'static str], reason: &'static str) -> PinnedFlag {
    PinnedFlag {
        args,
        min_version: None,
        gate: false,
        reason,
    }
}

const fn gated(
    args: &'static [&'static str],
    min_version: &'static str,
    reason: &'static str,
) -> PinnedFlag {
    PinnedFlag {
        args,
        min_version: Some(min_version),
        gate: true,
        reason,
    }
}

pub const MOCK: RunContract = RunContract {
    permission_mode: PermissionMode {
        write: &[],
        read_only: None,
        min_version: None,
        gate: false,
        reason: "Built-in fixture connector; no CLI flags.",
    },
    leading: &[],
    trailing: &[],
    process_cwd_is_worktree: false,
    result_before_exit: false,
    prompt: PromptPlace::None,
    plan_mode: None,
};

pub const CLAUDE_CODE: RunContract = RunContract {
    permission_mode: PermissionMode {
        write: &["--permission-mode", "bypassPermissions"],
        // Chat still narrows tools with a dynamic `--allowedTools` list.
        read_only: None,
        min_version: None,
        gate: false,
        reason: "Claude Code 2.1.284 (interactive) and 2.1.285 (`claude -p`) start in auto mode when no permission mode is set. Coppice already passed bypassPermissions; pinning it keeps that mode.",
    },
    leading: &[
        flag(&["-p"], "Headless print mode."),
        flag(
            &["--output-format", "stream-json"],
            "Structured stdout the runner parses.",
        ),
        flag(
            &["--verbose"],
            "Already passed with stream-json. Kept so the output shape does not depend on a CLI default.",
        ),
    ],
    trailing: &[],
    process_cwd_is_worktree: true,
    result_before_exit: true,
    prompt: PromptPlace::AfterDashP,
    // Plan mode is read-only: it replaces bypassPermissions on a planning run.
    plan_mode: Some(&["--permission-mode", "plan"]),
};

pub const CODEX: RunContract = RunContract {
    permission_mode: PermissionMode {
        write: &["--dangerously-bypass-approvals-and-sandbox"],
        read_only: None,
        min_version: None,
        gate: false,
        reason: "Today's approval bypass (it also disables the Codex sandbox). A later milestone swaps this field for a sandboxed flag.",
    },
    leading: &[
        gated(
            &["--no-daemon"],
            "0.156",
            "Skip the shared local background server. Added in Codex 0.156. Omitted when the detected version is unknown or older.",
        ),
        flag(&["exec"], "Non-interactive exec."),
        flag(&["--json"], "Structured stdout."),
    ],
    trailing: &[flag(
        &["-C", "{worktree}"],
        "Ticket worktree. The process cwd stays the server cwd; Codex takes its root from -C.",
    )],
    process_cwd_is_worktree: false,
    result_before_exit: true,
    prompt: PromptPlace::None,
    plan_mode: None,
};

pub const CURSOR: RunContract = RunContract {
    permission_mode: PermissionMode {
        write: &["--force"],
        read_only: Some(&["--mode", "ask"]),
        min_version: None,
        gate: false,
        reason: "Write runs use --force, which Coppice already passed. Read-only chat uses --mode ask and omits --force. --sandbox exists and is left for a later milestone.",
    },
    leading: &[
        flag(&["-p"], "Headless print mode."),
        flag(
            &["--trust"],
            "Workspace trust. Headless runs fail on an untrusted worktree without --trust or --force. This is not the permission-mode flag.",
        ),
    ],
    trailing: &[
        flag(
            &["--output-format", "stream-json"],
            "Structured stdout the runner parses.",
        ),
        flag(
            &["--workspace", "{worktree}"],
            "Ticket worktree. The process cwd is the worktree as well.",
        ),
    ],
    process_cwd_is_worktree: true,
    result_before_exit: true,
    prompt: PromptPlace::AfterDashP,
    plan_mode: None,
};

pub const KILO_CODE: RunContract = RunContract {
    permission_mode: PermissionMode {
        write: &["--auto"],
        read_only: None,
        min_version: None,
        gate: false,
        reason: "Auto-approves permissions for non-interactive kilo run. Read-only chat is refused before launch.",
    },
    leading: &[
        flag(&["run"], "Non-interactive run."),
        flag(&["--format", "json"], "Structured stdout."),
    ],
    trailing: &[flag(
        &["--dir", "{worktree}"],
        "Documented working directory for kilo run. The process cwd is the worktree as well. The version that added --dir is not published, so the flag is not gated.",
    )],
    process_cwd_is_worktree: true,
    result_before_exit: true,
    prompt: PromptPlace::Append,
    plan_mode: None,
};

pub const OPENCODE: RunContract = RunContract {
    permission_mode: PermissionMode {
        write: &[],
        read_only: None,
        min_version: None,
        gate: false,
        reason: "opencode serve has no documented permission-mode flag. The HTTP session carries the worktree.",
    },
    leading: &[
        flag(
            &["serve"],
            "Dedicated foreground server for this run, instead of the shared background client.",
        ),
        flag(
            &["--hostname", "{hostname}"],
            "Bind address for the per-run server.",
        ),
        flag(&["--port", "{port}"], "Free port chosen for this run."),
    ],
    trailing: &[],
    process_cwd_is_worktree: false,
    result_before_exit: false,
    prompt: PromptPlace::None,
    plan_mode: None,
};

fn push_flag(out: &mut Vec<String>, flag: &PinnedFlag, subst: &LaunchSubst<'_>) {
    push_tokens(out, flag.args, subst, flag.min_version, flag.gate);
}

fn push_tokens(
    out: &mut Vec<String>,
    args: &[&str],
    subst: &LaunchSubst<'_>,
    min_version: Option<&str>,
    gate: bool,
) {
    if args.is_empty() || !include_flag(min_version, gate, subst.version) {
        return;
    }
    for arg in args {
        out.push(expand(arg, subst));
    }
}

fn include_flag(min_version: Option<&str>, gate: bool, detected: Option<&str>) -> bool {
    if !gate {
        return true;
    }
    match (min_version, detected) {
        (Some(min), Some(found)) => version_at_least(found, min),
        _ => false,
    }
}

fn expand(arg: &str, subst: &LaunchSubst<'_>) -> String {
    arg.replace("{worktree}", subst.worktree)
        .replace("{hostname}", subst.hostname)
        .replace("{port}", subst.port)
}

fn insert_after_dash_p(argv: Vec<String>, prompt: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(argv.len() + 1);
    let mut inserted = false;
    for arg in argv {
        let is_p = arg == "-p";
        out.push(arg);
        if is_p && !inserted {
            out.push(prompt.to_string());
            inserted = true;
        }
    }
    if !inserted {
        out.push(prompt.to_string());
    }
    out
}

/// First version-shaped token in a `--version` line, such as `2.1.292`.
pub fn version_token(raw: &str) -> Option<&str> {
    for token in raw.split_whitespace() {
        let token = token.trim_matches(|c: char| matches!(c, ',' | '(' | ')' | '[' | ']'));
        let token = token
            .strip_prefix('v')
            .or_else(|| token.strip_prefix('V'))
            .unwrap_or(token);
        if !token.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let end = token
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
            .unwrap_or(token.len());
        let ver = token[..end].trim_end_matches('.');
        if ver.chars().any(|c| c.is_ascii_digit()) {
            return Some(ver);
        }
    }
    None
}

/// Numeric compare. Missing components count as zero. Non-numeric text is ignored.
pub fn version_at_least(detected: &str, minimum: &str) -> bool {
    let found = version_parts(detected);
    let min = version_parts(minimum);
    let n = found.len().max(min.len());
    for i in 0..n {
        let left = found.get(i).copied().unwrap_or(0);
        let right = min.get(i).copied().unwrap_or(0);
        if left > right {
            return true;
        }
        if left < right {
            return false;
        }
    }
    true
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subst<'a>(version: Option<&'a str>, read_only: bool) -> LaunchSubst<'a> {
        LaunchSubst {
            read_only,
            plan: false,
            worktree: "/wt",
            hostname: "127.0.0.1",
            port: "4096",
            version,
        }
    }

    #[test]
    fn version_token_reads_the_first_number() {
        assert_eq!(
            version_token("claude 2.1.292 (Claude Code)"),
            Some("2.1.292")
        );
        assert_eq!(version_token("codex-cli 0.156.0"), Some("0.156.0"));
        assert_eq!(version_token("v2.1.81"), Some("2.1.81"));
        assert_eq!(version_token("no version here"), None);
    }

    #[test]
    fn version_compare_is_numeric() {
        assert!(version_at_least("0.156.0", "0.156"));
        assert!(version_at_least("0.156", "0.156"));
        assert!(!version_at_least("0.155.9", "0.156"));
        assert!(version_at_least("2.1.292", "2.1.81"));
        assert!(!version_at_least("2.1.80", "2.1.81"));
        assert!(version_at_least("1", "0.156"));
    }

    #[test]
    fn claude_pins_permission_after_output_flags() {
        let argv =
            CLAUDE_CODE.with_prompt(CLAUDE_CODE.argv(&subst(Some("2.1.292"), false)), "PROMPT");
        assert_eq!(
            argv,
            [
                "-p",
                "PROMPT",
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-mode",
                "bypassPermissions",
            ]
        );
    }

    #[test]
    fn codex_gates_no_daemon_on_0_156() {
        let old = CODEX.argv(&subst(None, false));
        assert!(!old.iter().any(|a| a == "--no-daemon"));
        assert_eq!(
            old,
            [
                "exec",
                "--json",
                "--dangerously-bypass-approvals-and-sandbox",
                "-C",
                "/wt",
            ]
        );

        let older = CODEX.argv(&subst(Some("0.155.0"), false));
        assert!(!older.iter().any(|a| a == "--no-daemon"));

        let current = CODEX.argv(&subst(Some("0.156.0"), false));
        assert_eq!(current.first().map(String::as_str), Some("--no-daemon"));
        assert_eq!(current.get(1).map(String::as_str), Some("exec"));
        assert_eq!(
            CODEX
                .with_prompt(current, "PROMPT")
                .last()
                .map(String::as_str),
            Some("/wt")
        );
    }

    #[test]
    fn cursor_permission_mode_is_separate_from_trust() {
        let write = CURSOR.with_prompt(CURSOR.argv(&subst(None, false)), "PROMPT");
        assert_eq!(
            write,
            [
                "-p",
                "PROMPT",
                "--trust",
                "--force",
                "--output-format",
                "stream-json",
                "--workspace",
                "/wt",
            ]
        );
        let read_only = CURSOR.argv(&subst(None, true));
        assert!(read_only.windows(2).any(|w| w == ["--mode", "ask"]));
        assert!(!read_only.iter().any(|a| a == "--force"));
        assert!(read_only.iter().any(|a| a == "--trust"));
        assert!(!CURSOR.leading.iter().any(|f| f.args.contains(&"--force")));
        assert!(!CURSOR.trailing.iter().any(|f| f.args.contains(&"--force")));
    }

    #[test]
    fn kilo_appends_prompt_and_pins_dir() {
        let argv = KILO_CODE.with_prompt(KILO_CODE.argv(&subst(None, false)), "PROMPT");
        assert_eq!(
            argv,
            ["run", "--format", "json", "--auto", "--dir", "/wt", "PROMPT"]
        );
    }

    #[test]
    fn opencode_serve_has_no_permission_flag() {
        let argv = OPENCODE.argv(&subst(Some("1.2.3"), false));
        assert_eq!(argv, ["serve", "--hostname", "127.0.0.1", "--port", "4096"]);
        assert!(OPENCODE.permission_mode.write.is_empty());
    }

    #[test]
    fn only_claude_code_pins_a_native_plan_mode() {
        assert!(CODEX.plan_mode.is_none());
        assert!(CURSOR.plan_mode.is_none());
        assert!(KILO_CODE.plan_mode.is_none());
        assert!(OPENCODE.plan_mode.is_none());
        assert!(MOCK.plan_mode.is_none());

        let mut planned = subst(None, false);
        planned.plan = true;
        let argv = CLAUDE_CODE.argv(&planned);
        assert!(argv.windows(2).any(|w| w == ["--permission-mode", "plan"]));
        assert!(!argv.iter().any(|arg| arg == "bypassPermissions"));

        let write = CLAUDE_CODE.argv(&subst(None, false));
        assert!(write.windows(2).any(|w| w == ["--permission-mode", "bypassPermissions"]));
        assert!(!write.iter().any(|arg| arg == "plan"));
    }
}
