//! Pure `${VAR}` resolution for plugin MCP entries. Errors name keys, never values.

use crate::plugins::capability::{McpServerEntry, McpServerTransport};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

/// `secrets` are the values substituted from settings or the server environment
/// (sorted, deduplicated) so transports can scrub them from errors and tool output.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum ResolvedTransport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        secrets: Vec<String>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
        secrets: Vec<String>,
    },
}

impl ResolvedTransport {
    pub fn kind(&self) -> &'static str {
        match self {
            ResolvedTransport::Stdio { .. } => "stdio",
            ResolvedTransport::Http { .. } => "http",
        }
    }

    pub fn secrets(&self) -> &[String] {
        match self {
            ResolvedTransport::Stdio { secrets, .. } | ResolvedTransport::Http { secrets, .. } => {
                secrets
            }
        }
    }
}

/// Kind plus env/header names only: values, command, args, and url may hold secrets.
impl fmt::Debug for ResolvedTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolvedTransport::Stdio { env, .. } => f
                .debug_struct("Stdio")
                .field("env", &env.keys().collect::<Vec<_>>())
                .finish_non_exhaustive(),
            ResolvedTransport::Http { headers, .. } => f
                .debug_struct("Http")
                .field("headers", &headers.keys().collect::<Vec<_>>())
                .finish_non_exhaustive(),
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
    placeholder_defaults(entries).into_keys().collect()
}

/// Each setting key, and whether every reference to it carries a `:-` default.
fn placeholder_defaults(entries: &[McpServerEntry]) -> BTreeMap<String, bool> {
    let mut keys = BTreeMap::new();
    let mut collect = |name: &str, default: Option<&str>| {
        if name != PLUGIN_ROOT {
            *keys.entry(name.to_string()).or_insert(true) &= default.is_some();
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

/// Where a setting key's value comes from when a server starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource {
    Setting,
    Env,
    Default,
    Missing,
}

impl SettingSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            SettingSource::Setting => "setting",
            SettingSource::Env => "env",
            SettingSource::Default => "default",
            SettingSource::Missing => "missing",
        }
    }
}

/// The source `resolve` would use for each setting key, from presence only:
/// a stored setting, else an allowed non-empty server env var, else a default
/// present at every reference.
pub fn setting_sources(
    entries: &[McpServerEntry],
    configured: &BTreeSet<String>,
    env: &dyn Fn(&str) -> Option<String>,
) -> BTreeMap<String, SettingSource> {
    placeholder_defaults(entries)
        .into_iter()
        .map(|(key, has_default)| {
            let source = if configured.contains(&key) {
                SettingSource::Setting
            } else if server_env_allowed(&key) && env(&key).is_some_and(|v| !v.is_empty()) {
                SettingSource::Env
            } else if has_default {
                SettingSource::Default
            } else {
                SettingSource::Missing
            };
            (key, source)
        })
        .collect()
}

pub fn resolve(
    transport: &McpServerTransport,
    ctx: &ResolveCtx,
) -> Result<ResolvedTransport, String> {
    let mut secrets = Vec::new();
    let mut lookup = |name: &str, default: Option<&str>| {
        if name == PLUGIN_ROOT {
            return Ok(ctx.plugin_root.to_string_lossy().into_owned());
        }
        let value = ctx
            .settings
            .get(name)
            .cloned()
            .filter(|v| !v.is_empty())
            .or_else(|| {
                server_env_allowed(name)
                    .then(|| (ctx.env)(name))
                    .flatten()
                    .filter(|v| !v.is_empty())
            });
        if let Some(value) = value {
            secrets.push(value.clone());
            return Ok(value);
        }
        default
            .map(str::to_string)
            .ok_or_else(|| format!("missing setting \"{name}\""))
    };
    let mut resolved = match transport {
        McpServerTransport::Stdio { command, args, env } => ResolvedTransport::Stdio {
            command: expand(command, &mut lookup)?,
            args: args
                .iter()
                .map(|a| expand(a, &mut lookup))
                .collect::<Result<_, _>>()?,
            env: expand_values(env, &mut lookup)?,
            secrets: Vec::new(),
        },
        McpServerTransport::Http { url, headers } => ResolvedTransport::Http {
            url: expand(url, &mut lookup)?,
            headers: expand_values(headers, &mut lookup)?,
            secrets: Vec::new(),
        },
        McpServerTransport::Unsupported { kind } => {
            return Err(format!("unsupported transport \"{kind}\""))
        }
    };
    secrets.sort();
    secrets.dedup();
    match &mut resolved {
        ResolvedTransport::Stdio { secrets: s, .. }
        | ResolvedTransport::Http { secrets: s, .. } => *s = secrets,
    }
    Ok(resolved)
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
                secrets: vec!["/home/x".into(), "tok".into()],
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
                secrets: vec!["/home/x".into(), "from-env".into()],
            }
        );
    }

    #[test]
    fn secrets_are_setting_and_env_values_only() {
        let env = |name: &str| (name == "FROM_ENV").then(|| "env-val".to_string());
        let transport = stdio(
            "${CLAUDE_PLUGIN_ROOT}/bin",
            &[
                "--token=${TOKEN}",
                "${PORT:-8080}",
                "${TOKEN}",
                "${FROM_ENV}",
            ],
            &[],
        );
        let resolved = resolve_with(&transport, &settings(&[("TOKEN", "set-val")]), &env).unwrap();
        assert_eq!(resolved.secrets(), ["env-val", "set-val"]);
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
    fn setting_sources_follow_resolution_order() {
        let entries = vec![McpServerEntry {
            name: "a".into(),
            transport: stdio(
                "${CLAUDE_PLUGIN_ROOT}/${SET}",
                &[
                    "${FROM_ENV}",
                    "${EMPTY_ENV}",
                    "${PORT:-8080}",
                    "${HALF:-x}",
                    "${HALF}",
                    "${NONE}",
                    "${DATABASE_URL}",
                    "${SET_AND_ENV}",
                ],
                &[],
            ),
            error: None,
        }];
        let configured: BTreeSet<String> = ["SET", "SET_AND_ENV"].map(String::from).into();
        let env = |name: &str| match name {
            "FROM_ENV" | "SET_AND_ENV" | "DATABASE_URL" | "PORT" => Some("v".to_string()),
            "EMPTY_ENV" => Some(String::new()),
            _ => None,
        };
        let sources = setting_sources(&entries, &configured, &env);
        let shown: Vec<_> = sources
            .iter()
            .map(|(k, s)| (k.as_str(), s.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("DATABASE_URL", "missing"),
                ("EMPTY_ENV", "missing"),
                ("FROM_ENV", "env"),
                ("HALF", "missing"),
                ("NONE", "missing"),
                ("PORT", "env"),
                ("SET", "setting"),
                ("SET_AND_ENV", "setting"),
            ]
        );
        let no_env = |_: &str| None;
        assert_eq!(
            setting_sources(&entries, &configured, &no_env).get("PORT"),
            Some(&SettingSource::Default)
        );
    }

    #[test]
    fn debug_shows_only_kind_and_names() {
        let stdio = ResolvedTransport::Stdio {
            command: "/bin/cmd-secret".into(),
            args: vec!["--token=arg-secret".into()],
            env: settings(&[("API_TOKEN", "env-secret")]),
            secrets: vec!["arg-secret".into(), "env-secret".into()],
        };
        let shown = format!("{stdio:?}");
        assert!(
            shown.contains("Stdio") && shown.contains("API_TOKEN"),
            "{shown}"
        );
        for secret in ["cmd-secret", "arg-secret", "env-secret"] {
            assert!(!shown.contains(secret), "{shown}");
        }

        let http = ResolvedTransport::Http {
            url: "https://u:url-pass@h/mcp?k=url-query".into(),
            headers: settings(&[("Authorization", "Bearer hdr-secret")]),
            secrets: vec!["hdr-secret".into()],
        };
        let shown = format!("{http:?}");
        assert!(
            shown.contains("Http") && shown.contains("Authorization"),
            "{shown}"
        );
        for secret in ["url-pass", "url-query", "hdr-secret"] {
            assert!(!shown.contains(secret), "{shown}");
        }
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
