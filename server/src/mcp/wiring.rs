//! How each connector is told about the Coppice MCP gateway for one run. Every
//! per-run MCP file body and flag set is rendered here so the server name and
//! token env var live in one place. The token itself never appears in these
//! renderings: each CLI interpolates it from the process env (`McpAccess::env`).

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::json;

use super::grant::McpAccess;
use super::protocol::SERVER_NAME;

const TOKEN_ENV: &str = "COPPICE_MCP_TOKEN";
const OPENCODE_SCHEMA: &str = "https://opencode.ai/config.json";

/// Allowlist pattern for the gateway's tools under connectors that namespace
/// MCP tools as `mcp__<server>__<tool>` (claude-code).
pub fn claude_tool_pattern() -> String {
    format!("mcp__{SERVER_NAME}__*")
}

/// OpenCode's per-run config when the run has no gateway access.
pub fn opencode_base_json() -> String {
    json!({ "$schema": OPENCODE_SCHEMA }).to_string()
}

#[derive(Debug, Clone)]
pub struct McpServerSpec {
    pub name: &'static str,
    pub url: String,
    pub token_env: &'static str,
}

impl McpServerSpec {
    pub fn from_access(access: &McpAccess) -> Self {
        Self {
            name: SERVER_NAME,
            url: access.url.clone(),
            token_env: TOKEN_ENV,
        }
    }

    /// `<run dir>/mcp.json`, loaded with `--mcp-config … --strict-mcp-config`.
    pub fn claude_json(&self) -> String {
        json!({
            "mcpServers": {
                self.name: {
                    "type": "http",
                    "url": self.url,
                    "headers": { "Authorization": format!("Bearer ${{{}}}", self.token_env) },
                }
            }
        })
        .to_string()
    }

    /// `<run home>/.cursor/mcp.json`. Fields keep the verified layout order.
    pub fn cursor_mcp_json(&self) -> String {
        #[derive(Serialize)]
        struct McpFile<'a> {
            #[serde(rename = "mcpServers")]
            mcp_servers: BTreeMap<&'a str, McpServer<'a>>,
        }
        #[derive(Serialize)]
        struct McpServer<'a> {
            url: &'a str,
            headers: McpHeaders,
        }
        #[derive(Serialize)]
        struct McpHeaders {
            #[serde(rename = "Authorization")]
            authorization: String,
        }

        let server = McpServer {
            url: &self.url,
            headers: McpHeaders {
                authorization: format!("Bearer ${{env:{}}}", self.token_env),
            },
        };
        let file = McpFile {
            mcp_servers: BTreeMap::from([(self.name, server)]),
        };
        serde_json::to_string(&file).expect("cursor mcp.json serializes")
    }

    /// `<run config dir>/cli-config.json`. Without the allow rule `-p` denies
    /// every gateway call at the approval prompt.
    pub fn cursor_cli_config(&self) -> String {
        #[derive(Serialize)]
        struct CliConfigFile {
            version: u32,
            permissions: CliPermissions,
        }
        #[derive(Serialize)]
        struct CliPermissions {
            allow: Vec<String>,
            deny: Vec<String>,
        }

        let file = CliConfigFile {
            version: 1,
            permissions: CliPermissions {
                allow: vec![format!("Mcp({}:*)", self.name)],
                deny: Vec::new(),
            },
        };
        serde_json::to_string(&file).expect("cursor cli-config.json serializes")
    }

    /// The per-run `OPENCODE_CONFIG`.
    pub fn opencode_json(&self) -> String {
        json!({
            "$schema": OPENCODE_SCHEMA,
            "mcp": self.remote_server(),
        })
        .to_string()
    }

    /// `<run dir>/kilo-config.json`, pointed at by `KILO_CONFIG`.
    pub fn kilo_json(&self) -> String {
        json!({ "mcp": self.remote_server() }).to_string()
    }

    /// `-c` overrides adding the gateway for this codex process only.
    pub fn codex_args(&self) -> Vec<String> {
        vec![
            "-c".to_string(),
            format!("mcp_servers.{}.url=\"{}\"", self.name, self.url),
            "-c".to_string(),
            format!(
                "mcp_servers.{}.bearer_token_env_var=\"{}\"",
                self.name, self.token_env
            ),
        ]
    }

    /// OpenCode-family `remote` server entry with `{env:…}` interpolation.
    fn remote_server(&self) -> serde_json::Value {
        json!({
            self.name: {
                "type": "remote",
                "url": self.url,
                "enabled": true,
                "headers": { "Authorization": format!("Bearer {{env:{}}}", self.token_env) },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://127.0.0.1:5000/mcp";

    fn spec() -> McpServerSpec {
        McpServerSpec::from_access(&McpAccess {
            url: URL.into(),
            token: "super-secret-run-token".into(),
        })
    }

    #[test]
    fn claude_json_matches_baseline() {
        assert_eq!(
            spec().claude_json(),
            r#"{"mcpServers":{"coppice":{"headers":{"Authorization":"Bearer ${COPPICE_MCP_TOKEN}"},"type":"http","url":"http://127.0.0.1:5000/mcp"}}}"#
        );
    }

    #[test]
    fn cursor_mcp_json_matches_baseline() {
        assert_eq!(
            spec().cursor_mcp_json(),
            r#"{"mcpServers":{"coppice":{"url":"http://127.0.0.1:5000/mcp","headers":{"Authorization":"Bearer ${env:COPPICE_MCP_TOKEN}"}}}}"#
        );
    }

    #[test]
    fn cursor_cli_config_matches_baseline() {
        assert_eq!(
            spec().cursor_cli_config(),
            r#"{"version":1,"permissions":{"allow":["Mcp(coppice:*)"],"deny":[]}}"#
        );
    }

    #[test]
    fn opencode_json_matches_baseline() {
        assert_eq!(
            spec().opencode_json(),
            r#"{"$schema":"https://opencode.ai/config.json","mcp":{"coppice":{"enabled":true,"headers":{"Authorization":"Bearer {env:COPPICE_MCP_TOKEN}"},"type":"remote","url":"http://127.0.0.1:5000/mcp"}}}"#
        );
    }

    #[test]
    fn kilo_json_matches_baseline() {
        assert_eq!(
            spec().kilo_json(),
            r#"{"mcp":{"coppice":{"enabled":true,"headers":{"Authorization":"Bearer {env:COPPICE_MCP_TOKEN}"},"type":"remote","url":"http://127.0.0.1:5000/mcp"}}}"#
        );
    }

    #[test]
    fn codex_args_match_baseline() {
        assert_eq!(
            spec().codex_args(),
            vec![
                "-c".to_string(),
                r#"mcp_servers.coppice.url="http://127.0.0.1:5000/mcp""#.to_string(),
                "-c".to_string(),
                r#"mcp_servers.coppice.bearer_token_env_var="COPPICE_MCP_TOKEN""#.to_string(),
            ]
        );
    }

    #[test]
    fn claude_tool_pattern_matches_baseline() {
        assert_eq!(claude_tool_pattern(), "mcp__coppice__*");
    }

    #[test]
    fn opencode_base_json_matches_baseline() {
        assert_eq!(
            opencode_base_json(),
            r#"{"$schema":"https://opencode.ai/config.json"}"#
        );
    }

    #[test]
    fn renderers_never_embed_the_token() {
        let spec = spec();
        let rendered = [
            spec.claude_json(),
            spec.cursor_mcp_json(),
            spec.cursor_cli_config(),
            spec.opencode_json(),
            spec.kilo_json(),
            spec.codex_args().join(" "),
        ];
        for body in rendered {
            assert!(!body.contains("super-secret-run-token"), "{body}");
        }
    }
}
