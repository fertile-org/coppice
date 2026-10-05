//! Static descriptors for every agent connector Coppice knows about.

use serde::Serialize;

pub mod probe;

pub const MOCK: &str = "mock";
pub const CURSOR: &str = "cursor";
pub const CLAUDE_CODE: &str = "claude-code";
pub const CODEX: &str = "codex";
pub const KILO_CODE: &str = "kilo-code";
pub const OPENCODE: &str = "opencode";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectorDescriptor {
    /// Matches `[agent.connectors.<id>]` in config and `agents.connector`.
    pub id: &'static str,
    pub display_name: &'static str,
    /// `"mock"` for the built-in.
    pub binary: &'static str,
    pub install: InstallInfo,
    pub default_model_providers: &'static [&'static str],
    pub mcp_wiring: McpWiring,
    pub mcp_tool_names: ToolNameStyle,
    pub console: ConsoleKind,
    pub caps: Capabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallInfo {
    pub auth_hint: &'static str,
    /// Relative paths under $HOME that suggest auth is present.
    pub auth_paths: &'static [&'static str],
    /// Optional env vars that count as authenticated.
    pub auth_env: &'static [&'static str],
    /// Cheap local command run against the binary; empty means no probe.
    pub probe_args: &'static [&'static str],
    /// A successful probe proves auth (it needs credentials to succeed).
    pub probe_proves_auth: bool,
    /// Vendor install docs; empty for the built-in.
    pub docs_url: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum McpWiring {
    ClaudeJson,
    CursorHome,
    OpenCodeJson,
    KiloJson,
    CodexFlags,
    MockHttp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolNameStyle {
    /// `mcp__coppice__<tool>`
    McpDoubleUnderscore,
    /// `coppice-<tool>`
    Dash,
    /// `coppice_<tool>`
    Underscore,
    /// Separate `server` and `tool` fields.
    ServerToolFields,
    None,
}

/// A tool call routed through the Coppice gateway: a core tool (`plugin: None`)
/// or a plugin tool exposed as `<plugin>__<tool>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayTool {
    pub plugin: Option<String>,
    pub tool: String,
}

impl GatewayTool {
    /// Console label: `coppice · ticket_get` or `github · create_issue`.
    pub fn title(&self, server: &str) -> String {
        format!(
            "{} · {}",
            self.plugin.as_deref().unwrap_or(server),
            self.tool
        )
    }

    /// Core tool names never contain `__`, so the first one separates the plugin.
    fn from_exposed(exposed: &str) -> Option<Self> {
        if exposed.is_empty() {
            return None;
        }
        Some(match exposed.split_once("__") {
            Some((plugin, tool)) if !plugin.is_empty() && !tool.is_empty() => Self {
                plugin: Some(plugin.to_string()),
                tool: tool.to_string(),
            },
            _ => Self {
                plugin: None,
                tool: exposed.to_string(),
            },
        })
    }
}

/// Parse a connector's tool name for gateway `server` according to its style.
/// `ServerToolFields` connectors use [`gateway_tool_from_fields`] instead.
pub fn gateway_tool(style: ToolNameStyle, server: &str, name: &str) -> Option<GatewayTool> {
    let exposed = match style {
        ToolNameStyle::McpDoubleUnderscore => name
            .strip_prefix("mcp__")
            .and_then(|rest| rest.strip_prefix(server))
            .and_then(|rest| rest.strip_prefix("__")),
        ToolNameStyle::Dash => name
            .strip_prefix(server)
            .and_then(|rest| rest.strip_prefix('-')),
        ToolNameStyle::Underscore => name
            .strip_prefix(server)
            .and_then(|rest| rest.strip_prefix('_')),
        ToolNameStyle::ServerToolFields | ToolNameStyle::None => None,
    }?;
    GatewayTool::from_exposed(exposed)
}

/// Gateway tool for connectors that report `server` and `tool` separately.
pub fn gateway_tool_from_fields(
    server: &str,
    field_server: &str,
    tool: &str,
) -> Option<GatewayTool> {
    if field_server != server {
        return None;
    }
    GatewayTool::from_exposed(tool)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConsoleKind {
    OpenCodeSession,
    Structured,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Adapter enforces a read-only allowlist when asked.
    pub read_only_tools: bool,
    /// Chat turns may resume the CLI session.
    pub chat_resume: bool,
    /// Adapter reports a session id while running.
    pub session_events: bool,
    /// `work_on_ticket` runs may resume a prior session id.
    pub run_resume: bool,
    /// Keeps a per-run server process the watchdog tracks.
    pub run_server: bool,
}

const CONNECTORS: &[ConnectorDescriptor] = &[
    #[cfg(feature = "mock")]
    ConnectorDescriptor {
        id: MOCK,
        display_name: "Mock",
        binary: "mock",
        install: InstallInfo {
            auth_hint: "built-in; no setup",
            auth_paths: &[],
            auth_env: &[],
            probe_args: &[],
            probe_proves_auth: false,
            docs_url: "",
        },
        default_model_providers: &[],
        mcp_wiring: McpWiring::MockHttp,
        mcp_tool_names: ToolNameStyle::None,
        console: ConsoleKind::Plain,
        caps: Capabilities {
            read_only_tools: true,
            chat_resume: true,
            session_events: false,
            run_resume: false,
            run_server: false,
        },
    },
    ConnectorDescriptor {
        id: CURSOR,
        display_name: "Cursor",
        binary: "agent",
        install: InstallInfo {
            auth_hint: "agent login (copy URL)",
            auth_paths: &[".config/cursor/auth.json", ".cursor/auth.json"],
            auth_env: &[],
            probe_args: &["models"],
            probe_proves_auth: true,
            docs_url: "https://cursor.com/docs/cli/installation",
        },
        default_model_providers: &["cursor"],
        mcp_wiring: McpWiring::CursorHome,
        mcp_tool_names: ToolNameStyle::Dash,
        console: ConsoleKind::Structured,
        caps: Capabilities {
            read_only_tools: true,
            chat_resume: true,
            session_events: true,
            run_resume: true,
            run_server: false,
        },
    },
    ConnectorDescriptor {
        id: CLAUDE_CODE,
        display_name: "Claude Code",
        binary: "claude",
        install: InstallInfo {
            auth_hint: "ANTHROPIC_API_KEY or claude setup-token",
            auth_paths: &[".claude", ".config/claude"],
            auth_env: &["ANTHROPIC_API_KEY"],
            probe_args: &["--version"],
            probe_proves_auth: false,
            docs_url: "https://docs.anthropic.com/en/docs/claude-code/setup",
        },
        default_model_providers: &["sonnet", "opus", "haiku"],
        mcp_wiring: McpWiring::ClaudeJson,
        mcp_tool_names: ToolNameStyle::McpDoubleUnderscore,
        console: ConsoleKind::Structured,
        caps: Capabilities {
            read_only_tools: true,
            chat_resume: true,
            session_events: true,
            run_resume: true,
            run_server: false,
        },
    },
    ConnectorDescriptor {
        id: CODEX,
        display_name: "Codex",
        binary: "codex",
        install: InstallInfo {
            auth_hint: "codex login --device-auth",
            auth_paths: &[".codex"],
            auth_env: &["OPENAI_API_KEY"],
            probe_args: &["--version"],
            probe_proves_auth: false,
            docs_url: "https://developers.openai.com/codex/cli",
        },
        default_model_providers: &["openai"],
        mcp_wiring: McpWiring::CodexFlags,
        mcp_tool_names: ToolNameStyle::ServerToolFields,
        console: ConsoleKind::Structured,
        caps: Capabilities {
            read_only_tools: false,
            chat_resume: true,
            session_events: true,
            run_resume: false,
            run_server: false,
        },
    },
    ConnectorDescriptor {
        id: KILO_CODE,
        display_name: "Kilo Code",
        binary: "kilo",
        install: InstallInfo {
            auth_hint: "kilo auth / TUI /connect",
            auth_paths: &[".local/share/opencode", ".kilocode"],
            auth_env: &[],
            probe_args: &["--version"],
            probe_proves_auth: false,
            docs_url: "https://kilo.ai/docs/cli",
        },
        default_model_providers: &["anthropic"],
        mcp_wiring: McpWiring::KiloJson,
        // Unverified (no live CLI); follows its OpenCode fork. Only affects console labels.
        mcp_tool_names: ToolNameStyle::Underscore,
        console: ConsoleKind::Structured,
        caps: Capabilities {
            read_only_tools: false,
            chat_resume: false,
            session_events: true,
            run_resume: false,
            run_server: false,
        },
    },
    ConnectorDescriptor {
        id: OPENCODE,
        display_name: "OpenCode",
        binary: "opencode",
        install: InstallInfo {
            auth_hint: "opencode auth login",
            // After login; do NOT list `.opencode` (install tree).
            auth_paths: &[".local/share/opencode"],
            auth_env: &[],
            probe_args: &["auth", "list"],
            probe_proves_auth: true,
            docs_url: "https://opencode.ai/docs/",
        },
        default_model_providers: &[],
        mcp_wiring: McpWiring::OpenCodeJson,
        mcp_tool_names: ToolNameStyle::Underscore,
        console: ConsoleKind::OpenCodeSession,
        caps: Capabilities {
            read_only_tools: false,
            chat_resume: true,
            session_events: true,
            run_resume: false,
            run_server: true,
        },
    },
];

/// Every known connector, in display order.
pub fn all() -> &'static [ConnectorDescriptor] {
    CONNECTORS
}

pub fn get(id: &str) -> Option<&'static ConnectorDescriptor> {
    CONNECTORS.iter().find(|d| d.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_match_constants() {
        let ids: Vec<&str> = all().iter().map(|d| d.id).collect();
        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "duplicate connector ids: {ids:?}");
        #[cfg(feature = "mock")]
        let expected = vec![MOCK, CURSOR, CLAUDE_CODE, CODEX, KILO_CODE, OPENCODE];
        #[cfg(not(feature = "mock"))]
        let expected = vec![CURSOR, CLAUDE_CODE, CODEX, KILO_CODE, OPENCODE];
        assert_eq!(ids, expected);
        assert_eq!(
            [MOCK, CLAUDE_CODE, CURSOR, CODEX, KILO_CODE, OPENCODE],
            [
                "mock",
                "claude-code",
                "cursor",
                "codex",
                "kilo-code",
                "opencode"
            ]
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn assert_row(
        id: &str,
        wiring: McpWiring,
        names: ToolNameStyle,
        console: ConsoleKind,
        read_only_tools: bool,
        chat_resume: bool,
        session_events: bool,
        run_resume: bool,
        run_server: bool,
    ) {
        let d = get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(d.mcp_wiring, wiring, "{id} mcp_wiring");
        assert_eq!(d.mcp_tool_names, names, "{id} mcp_tool_names");
        assert_eq!(d.console, console, "{id} console");
        assert_eq!(
            d.caps,
            Capabilities {
                read_only_tools,
                chat_resume,
                session_events,
                run_resume,
                run_server,
            },
            "{id} caps"
        );
    }

    #[test]
    fn baseline_capabilities() {
        use ConsoleKind::*;
        use McpWiring::*;
        use ToolNameStyle::*;
        #[cfg(feature = "mock")]
        assert_row(
            MOCK,
            MockHttp,
            ToolNameStyle::None,
            Plain,
            true,
            true,
            false,
            false,
            false,
        );
        assert_row(
            CLAUDE_CODE,
            ClaudeJson,
            McpDoubleUnderscore,
            Structured,
            true,
            true,
            true,
            true,
            false,
        );
        assert_row(
            CURSOR, CursorHome, Dash, Structured, true, true, true, true, false,
        );
        assert_row(
            CODEX,
            CodexFlags,
            ServerToolFields,
            Structured,
            false,
            true,
            true,
            false,
            false,
        );
        assert_row(
            KILO_CODE, KiloJson, Underscore, Structured, false, false, true, false, false,
        );
        assert_row(
            OPENCODE,
            OpenCodeJson,
            Underscore,
            OpenCodeSession,
            false,
            true,
            true,
            false,
            true,
        );
    }

    #[test]
    fn install_info_moved_verbatim() {
        #[cfg(feature = "mock")]
        {
            let mock = get(MOCK).unwrap();
            assert_eq!(mock.binary, "mock");
            assert!(mock.default_model_providers.is_empty());
            assert_eq!(mock.install.auth_hint, "built-in; no setup");
            assert!(mock.install.auth_paths.is_empty());
            assert!(mock.install.auth_env.is_empty());
        }
        #[cfg(not(feature = "mock"))]
        assert!(get(MOCK).is_none());

        let cursor = get(CURSOR).unwrap();
        assert_eq!(cursor.binary, "agent");
        assert_eq!(cursor.default_model_providers, ["cursor"]);
        assert_eq!(cursor.install.auth_hint, "agent login (copy URL)");
        assert_eq!(
            cursor.install.auth_paths,
            [".config/cursor/auth.json", ".cursor/auth.json"]
        );
        assert!(cursor.install.auth_env.is_empty());

        let claude = get(CLAUDE_CODE).unwrap();
        assert_eq!(claude.binary, "claude");
        assert_eq!(claude.default_model_providers, ["sonnet", "opus", "haiku"]);
        assert_eq!(
            claude.install.auth_hint,
            "ANTHROPIC_API_KEY or claude setup-token"
        );
        assert_eq!(claude.install.auth_paths, [".claude", ".config/claude"]);
        assert_eq!(claude.install.auth_env, ["ANTHROPIC_API_KEY"]);

        let codex = get(CODEX).unwrap();
        assert_eq!(codex.binary, "codex");
        assert_eq!(codex.default_model_providers, ["openai"]);
        assert_eq!(codex.install.auth_hint, "codex login --device-auth");
        assert_eq!(codex.install.auth_paths, [".codex"]);
        assert_eq!(codex.install.auth_env, ["OPENAI_API_KEY"]);

        let kilo = get(KILO_CODE).unwrap();
        assert_eq!(kilo.binary, "kilo");
        assert_eq!(kilo.default_model_providers, ["anthropic"]);
        assert_eq!(kilo.install.auth_hint, "kilo auth / TUI /connect");
        assert_eq!(
            kilo.install.auth_paths,
            [".local/share/opencode", ".kilocode"]
        );
        assert!(kilo.install.auth_env.is_empty());

        let opencode = get(OPENCODE).unwrap();
        assert_eq!(opencode.binary, "opencode");
        assert!(opencode.default_model_providers.is_empty());
        assert_eq!(opencode.install.auth_hint, "opencode auth login");
        assert_eq!(opencode.install.auth_paths, [".local/share/opencode"]);
        assert!(opencode.install.auth_env.is_empty());
    }

    /// `web/src/lib/schemas/connector.ts` parses these exact strings.
    #[test]
    fn console_kind_wire_strings() {
        let wire = |k: ConsoleKind| serde_json::to_string(&k).unwrap();
        assert_eq!(wire(ConsoleKind::OpenCodeSession), r#""openCodeSession""#);
        assert_eq!(wire(ConsoleKind::Structured), r#""structured""#);
        assert_eq!(wire(ConsoleKind::Plain), r#""plain""#);
    }

    #[test]
    fn capabilities_wire_field_names() {
        let value = serde_json::to_value(Capabilities {
            read_only_tools: true,
            chat_resume: false,
            session_events: true,
            run_resume: false,
            run_server: true,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "readOnlyTools": true,
                "chatResume": false,
                "sessionEvents": true,
                "runResume": false,
                "runServer": true,
            })
        );
    }

    fn core(tool: &str) -> Option<GatewayTool> {
        Some(GatewayTool {
            plugin: None,
            tool: tool.to_string(),
        })
    }

    fn plugin(plugin: &str, tool: &str) -> Option<GatewayTool> {
        Some(GatewayTool {
            plugin: Some(plugin.to_string()),
            tool: tool.to_string(),
        })
    }

    #[test]
    fn gateway_tool_per_style() {
        use ToolNameStyle::*;
        assert_eq!(
            gateway_tool(McpDoubleUnderscore, "coppice", "mcp__coppice__ticket_get"),
            core("ticket_get")
        );
        assert_eq!(
            gateway_tool(
                McpDoubleUnderscore,
                "coppice",
                "mcp__coppice__github__create_issue"
            ),
            plugin("github", "create_issue")
        );
        assert_eq!(
            gateway_tool(McpDoubleUnderscore, "coppice", "mcp__other__x"),
            Option::None
        );
        assert_eq!(
            gateway_tool(Dash, "coppice", "coppice-ticket_get"),
            core("ticket_get")
        );
        assert_eq!(
            gateway_tool(Underscore, "coppice", "coppice_github__create_issue"),
            plugin("github", "create_issue")
        );
        assert_eq!(
            gateway_tool(Underscore, "coppice", "coppice_"),
            Option::None
        );
        for name in [
            "mcp__coppice__ticket_get",
            "coppice-ticket_get",
            "coppice_ticket_get",
        ] {
            assert_eq!(gateway_tool(None, "coppice", name), Option::None);
            assert_eq!(
                gateway_tool(ServerToolFields, "coppice", name),
                Option::None
            );
        }
        for style in [
            McpDoubleUnderscore,
            Dash,
            Underscore,
            ServerToolFields,
            None,
        ] {
            assert_eq!(gateway_tool(style, "coppice", "Bash"), Option::None);
        }
    }

    #[test]
    fn gateway_tool_from_fields_codex() {
        assert_eq!(
            gateway_tool_from_fields("coppice", "coppice", "github__create_issue"),
            plugin("github", "create_issue")
        );
        assert_eq!(
            gateway_tool_from_fields("coppice", "coppice", "ticket_get"),
            core("ticket_get")
        );
        assert_eq!(
            gateway_tool_from_fields("coppice", "other", "github__create_issue"),
            Option::None
        );
    }

    #[test]
    fn title_format() {
        assert_eq!(
            core("ticket_get").unwrap().title("coppice"),
            "coppice · ticket_get"
        );
        assert_eq!(
            plugin("github", "create_issue").unwrap().title("coppice"),
            "github · create_issue"
        );
    }

    #[test]
    fn get_unknown_is_none() {
        assert!(get("nope").is_none());
        assert!(get("").is_none());
        assert!(get("Cursor").is_none());
    }
}
