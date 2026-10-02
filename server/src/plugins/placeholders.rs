//! Pure `${VAR}` resolution for plugin MCP entries. Errors name keys, never values.

use crate::plugins::capability::{McpServerEntry, McpServerTransport};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResolvedTransport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
    },
}

impl ResolvedTransport {
    pub fn kind(&self) -> &'static str {
        match self {
            ResolvedTransport::Stdio { .. } => "stdio",
            ResolvedTransport::Http { .. } => "http",
        }
    }
}

pub struct ResolveCtx<'a> {
    pub plugin_root: &'a Path,
    pub settings: &'a BTreeMap<String, String>,
    /// Raw environment lookup; blocked names are filtered by `resolve`.
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

const PLUGIN_ROOT: &str = "CLAUDE_PLUGIN_ROOT";

/// Server-environment names a plugin may read through `${VAR}`.
pub fn server_env_allowed(name: &str) -> bool {
    !name.starts_with("COPPICE_") && name != "DATABASE_URL" && name != "SECRETS_MASTER_KEY"
}

/// Setting keys referenced by the entries' command, args, env values, url, and header values.
pub fn placeholder_keys(entries: &[McpServerEntry]) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut collect = |name: &str, _default: Option<&str>| {
        if name != PLUGIN_ROOT {
            keys.insert(name.to_string());
        }
        Ok(String::new())
    };
    for entry in entries {
        for value in transport_values(&entry.transport) {
            let _ = expand(value, &mut collect);
        }
    }
    keys
}

pub fn resolve(
    transport: &McpServerTransport,
    ctx: &ResolveCtx,
) -> Result<ResolvedTransport, String> {
    let mut lookup = |name: &str, default: Option<&str>| {
        if name == PLUGIN_ROOT {
            return Ok(ctx.plugin_root.to_string_lossy().into_owned());
        }
        ctx.settings
            .get(name)
            .cloned()
            .filter(|v| !v.is_empty())
            .or_else(|| {
                server_env_allowed(name)
                    .then(|| (ctx.env)(name))
                    .flatten()
                    .filter(|v| !v.is_empty())
            })
            .or_else(|| default.map(str::to_string))
            .ok_or_else(|| format!("missing setting \"{name}\""))
    };
    match transport {
        McpServerTransport::Stdio { command, args, env } => Ok(ResolvedTransport::Stdio {
            command: expand(command, &mut lookup)?,
            args: args
                .iter()
                .map(|a| expand(a, &mut lookup))
                .collect::<Result<_, _>>()?,
            env: expand_values(env, &mut lookup)?,
        }),
        McpServerTransport::Http { url, headers } => Ok(ResolvedTransport::Http {
            url: expand(url, &mut lookup)?,
            headers: expand_values(headers, &mut lookup)?,
        }),
        McpServerTransport::Unsupported { kind } => {
            Err(format!("unsupported transport \"{kind}\""))
        }
    }
}

fn transport_values(transport: &McpServerTransport) -> Vec<&str> {
    match transport {
        McpServerTransport::Stdio { command, args, env } => std::iter::once(command)
            .chain(args)
            .chain(env.values())
            .map(String::as_str)
            .collect(),
        McpServerTransport::Http { url, headers } => std::iter::once(url)
            .chain(headers.values())
            .map(String::as_str)
            .collect(),
        McpServerTransport::Unsupported { .. } => Vec::new(),
    }
}

fn expand_values<F>(
    map: &BTreeMap<String, String>,
    lookup: &mut F,
) -> Result<BTreeMap<String, String>, String>
where
    F: FnMut(&str, Option<&str>) -> Result<String, String>,
{
    map.iter()
        .map(|(k, v)| Ok((k.clone(), expand(v, lookup)?)))
        .collect()
}

fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Replaces `${NAME}` / `${NAME:-default}` via `lookup`; anything else is kept verbatim.
fn expand<F>(input: &str, lookup: &mut F) -> Result<String, String>
where
    F: FnMut(&str, Option<&str>) -> Result<String, String>,
{
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return Ok(out);
        };
        let body = &after[..end];
        let (name, default) = match body.split_once(":-") {
            Some((name, default)) => (name, Some(default)),
            None => (body, None),
        };
        if is_name(name) {
            out.push_str(&lookup(name, default)?);
        } else {
            out.push_str(&rest[start..start + 2 + end + 1]);
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn stdio(command: &str, args: &[&str], env: &[(&str, &str)]) -> McpServerTransport {
        McpServerTransport::Stdio {
            command: command.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            env: settings(env),
        }
    }

    fn resolve_with(
        transport: &McpServerTransport,
        settings: &BTreeMap<String, String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<ResolvedTransport, String> {
        let ctx = ResolveCtx {
            plugin_root: Path::new("/p"),
            settings,
            env,
        };
        resolve(transport, &ctx)
    }

    fn command_of(resolved: ResolvedTransport) -> String {
        match resolved {
            ResolvedTransport::Stdio { command, .. } => command,
            other => panic!("expected stdio, got {other:?}"),
        }
    }

    #[test]
    fn resolves_plugin_root_settings_and_env() {
        let env = |name: &str| match name {
            "HOME_DIR" => Some("/home/x".to_string()),
            "API_TOKEN" => Some("from-env".to_string()),
            _ => None,
        };
        let transport = stdio(
            "${CLAUDE_PLUGIN_ROOT}/bin/fs",
            &["--home", "${HOME_DIR}"],
            &[("TOKEN", "${API_TOKEN}")],
        );
        let resolved = resolve_with(&transport, &settings(&[("API_TOKEN", "tok")]), &env).unwrap();
        assert_eq!(resolved.kind(), "stdio");
        assert_eq!(
            resolved,
            ResolvedTransport::Stdio {
                command: "/p/bin/fs".into(),
                args: vec!["--home".into(), "/home/x".into()],
                env: settings(&[("TOKEN", "tok")]),
            }
        );

        let http = McpServerTransport::Http {
            url: "https://h/${HOME_DIR}".into(),
            headers: settings(&[("Authorization", "Bearer ${API_TOKEN}")]),
        };
        let resolved = resolve_with(&http, &BTreeMap::new(), &env).unwrap();
        assert_eq!(resolved.kind(), "http");
        assert_eq!(
            resolved,
            ResolvedTransport::Http {
                url: "https://h//home/x".into(),
                headers: settings(&[("Authorization", "Bearer from-env")]),
            }
        );
    }

    #[test]
    fn blocked_env_names_are_missing() {
        let env = |_: &str| Some("leak".to_string());
        for name in [
            "COPPICE_SECRETS__MASTER_KEY",
            "DATABASE_URL",
            "SECRETS_MASTER_KEY",
        ] {
            assert!(!server_env_allowed(name), "{name}");
            let transport = stdio(&format!("${{{name}}}"), &[], &[]);
            assert_eq!(
                resolve_with(&transport, &BTreeMap::new(), &env),
                Err(format!("missing setting \"{name}\"")),
            );
        }
        assert!(server_env_allowed("HOME"));
        assert!(server_env_allowed("DATABASE_URL_X"));
    }

    #[test]
    fn default_value_syntax() {
        let env = |_: &str| None;
        let transport = stdio("${PORT:-8080}", &[], &[]);
        let resolved = resolve_with(&transport, &BTreeMap::new(), &env).unwrap();
        assert_eq!(command_of(resolved), "8080");
        let resolved = resolve_with(&transport, &settings(&[("PORT", "9")]), &env).unwrap();
        assert_eq!(command_of(resolved), "9");
    }

    #[test]
    fn unterminated_placeholder_kept_verbatim() {
        let env = |_: &str| None;
        let transport = stdio("a${b", &[], &[]);
        let resolved = resolve_with(&transport, &BTreeMap::new(), &env).unwrap();
        assert_eq!(command_of(resolved), "a${b");
    }

    #[test]
    fn unsupported_transport_errors() {
        let env = |_: &str| None;
        let transport = McpServerTransport::Unsupported { kind: "sse".into() };
        assert_eq!(
            resolve_with(&transport, &BTreeMap::new(), &env),
            Err("unsupported transport \"sse\"".to_string()),
        );
    }

    #[test]
    fn placeholder_keys_collects_all_fields() {
        let entries = vec![
            McpServerEntry {
                name: "a".into(),
                transport: stdio(
                    "${CLAUDE_PLUGIN_ROOT}/${CMD}",
                    &["${ARG}", "${X:-d}"],
                    &[("${NOT_A_KEY}", "${ENV_VAL}")],
                ),
                error: None,
            },
            McpServerEntry {
                name: "b".into(),
                transport: McpServerTransport::Http {
                    url: "https://${HOST}/mcp".into(),
                    headers: settings(&[("${NOT_HEADER_NAME}", "Bearer ${HDR}")]),
                },
                error: None,
            },
            McpServerEntry {
                name: "c".into(),
                transport: McpServerTransport::Unsupported { kind: "sse".into() },
                error: None,
            },
        ];
        let keys: Vec<_> = placeholder_keys(&entries).into_iter().collect();
        assert_eq!(keys, ["ARG", "CMD", "ENV_VAL", "HDR", "HOST", "X"]);
    }

    #[test]
    fn error_never_contains_values() {
        let env = |_: &str| Some("env-secret".to_string());
        let transport = stdio("${API_TOKEN}", &["${DATABASE_URL}"], &[]);
        let err =
            resolve_with(&transport, &settings(&[("API_TOKEN", "s3cr3t")]), &env).unwrap_err();
        assert_eq!(err, "missing setting \"DATABASE_URL\"");
        assert!(!err.contains("s3cr3t"));
        assert!(!err.contains("env-secret"));
    }
}
