//! Every `config.toml` setting the Settings page shows in UI mode.
//!
//! A key missing from the file shows its default. Defaults come from
//! [`AppConfig`]'s own defaults, so they cannot drift from what the server
//! uses. Descriptions follow the settings reference
//! (`website/src/pages/docs/settings.md`).

use serde::Serialize;
use toml::Value;

use crate::AppConfig;

/// How a setting is edited.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SettingKind {
    Bool,
    /// Not set is its own state, such as Automatic.
    #[serde(rename_all = "camelCase")]
    OptionalBool {
        unset_label: &'static str,
    },
    Integer {
        min: Option<i64>,
        max: Option<i64>,
    },
    IntegerChoice {
        options: &'static [i64],
    },
    String,
    Choice {
        options: &'static [&'static str],
    },
    /// Free text when `options` is `None`.
    StringList {
        options: Option<&'static [&'static str]>,
    },
    /// A `[keybindings]` entry: one key, several, or `""` for none.
    Shortcut,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingSpec {
    /// Table path and key, such as `["agent", "connectors", "claude-code", "enabled"]`.
    pub path: Vec<String>,
    pub group: String,
    pub description: String,
    pub kind: SettingKind,
    /// The value when the key is missing. `None` when that is not one value;
    /// `default_label` says what happens instead.
    pub default: Option<Value>,
    pub default_label: Option<String>,
    /// Changing it needs a restart.
    pub restart: bool,
}

impl SettingSpec {
    pub fn key(&self) -> String {
        self.path.join(".")
    }

    /// `Err` with a sentence when `value` does not fit this setting.
    pub fn check(&self, value: &Value) -> Result<(), String> {
        let key = self.key();
        match (&self.kind, value) {
            (SettingKind::Bool | SettingKind::OptionalBool { .. }, Value::Boolean(_)) => Ok(()),
            (SettingKind::Integer { min, max }, Value::Integer(number)) => {
                if min.is_some_and(|min| *number < min) || max.is_some_and(|max| *number > max) {
                    Err(match (min, max) {
                        (Some(min), Some(max)) => format!("Set {key} to {min}–{max}."),
                        (Some(min), None) => format!("Set {key} to {min} or more."),
                        (None, Some(max)) => format!("Set {key} to {max} or less."),
                        (None, None) => unreachable!(),
                    })
                } else {
                    Ok(())
                }
            }
            (SettingKind::IntegerChoice { options }, Value::Integer(number)) => {
                if options.contains(number) {
                    Ok(())
                } else {
                    Err(format!("Set {key} to {}.", or_list(options)))
                }
            }
            (SettingKind::String, Value::String(_)) => Ok(()),
            (SettingKind::Choice { options }, Value::String(text)) => {
                if options.contains(&text.as_str()) {
                    Ok(())
                } else {
                    Err(format!("Set {key} to {}.", or_list(options)))
                }
            }
            (SettingKind::StringList { options }, Value::Array(items)) => {
                for item in items {
                    let Value::String(text) = item else {
                        return Err(format!("{key} is a list of text values."));
                    };
                    if let Some(options) = options {
                        if !options.contains(&text.as_str()) {
                            return Err(format!(
                                "{text} isn't one of {key}'s values: {}.",
                                options.join(", ")
                            ));
                        }
                    }
                }
                Ok(())
            }
            (SettingKind::Shortcut, Value::String(_)) => Ok(()),
            (SettingKind::Shortcut, Value::Array(items))
                if items.iter().all(|item| matches!(item, Value::String(_))) =>
            {
                Ok(())
            }
            _ => Err(format!("That isn't a valid value for {key}.")),
        }
    }
}

fn or_list<T: std::fmt::Display>(options: &[T]) -> String {
    let words: Vec<String> = options.iter().map(ToString::to_string).collect();
    match words.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{}, or {last}", rest.join(", ")),
        _ => words.join(""),
    }
}

/// Connector ids in the order Connectors lists them, with display names.
const CONNECTORS: &[(&str, &str)] = &[
    ("claude-code", "Claude Code"),
    ("codex", "Codex"),
    ("cursor", "Cursor"),
    ("opencode", "OpenCode"),
    ("kilo-code", "Kilo Code"),
    ("copilot", "GitHub Copilot"),
    ("qwen-code", "Qwen Code"),
    ("kimi", "Kimi Code"),
    ("grok", "Grok"),
];

const STATUSES: &[(&str, &str)] = &[
    ("backlog", "Backlog"),
    ("ready", "Ready"),
    ("in_progress", "In Progress"),
    ("in_review", "In Review"),
    ("in_qa", "In QA"),
    ("wait_for_final_review", "Wait for Human Review"),
    ("blocked", "Blocked"),
    ("done", "Done"),
];

const AUTO_SAVE_TYPES: &[&str] = &[
    "bug_pattern",
    "coding_convention",
    "dependency_note",
    "performance_note",
    "review_feedback",
    "test_command",
];

const KNOWLEDGE_TYPES: &[&str] = &[
    "coding_convention",
    "architecture_rule",
    "bug_pattern",
    "test_command",
    "review_feedback",
    "dependency_note",
    "api_contract",
    "workflow_rule",
    "human_preference",
    "operational_runbook",
    "security_rule",
    "performance_note",
];

const POSITIVE: SettingKind = SettingKind::Integer {
    min: Some(1),
    max: None,
};

struct Fixed {
    key: &'static str,
    group: &'static str,
    description: &'static str,
    kind: SettingKind,
    restart: bool,
}

const fn fixed(
    key: &'static str,
    group: &'static str,
    description: &'static str,
    kind: SettingKind,
) -> Fixed {
    Fixed {
        key,
        group,
        description,
        kind,
        restart: false,
    }
}

const fn needs_restart(fixed: Fixed) -> Fixed {
    Fixed {
        restart: true,
        ..fixed
    }
}

const FIXED_BEFORE_CONNECTORS: &[Fixed] = &[
    fixed(
        "logs.retention_days",
        "Logs",
        "How many days of logs to keep, including today. Older logs are deleted every hour, and the oldest go first once logs pass 256 MiB.",
        SettingKind::IntegerChoice {
            options: &[3, 7, 14, 30],
        },
    ),
    needs_restart(fixed(
        "logs.dir",
        "Logs",
        "Folder for logs. The desktop app uses its own folder and ignores this.",
        SettingKind::String,
    )),
    needs_restart(fixed(
        "storage.run_artifacts_retention_days",
        "Storage",
        "Days a finished run's console and terminal log are kept. Archiving a ticket removes its runs' files right away.",
        POSITIVE,
    )),
    needs_restart(fixed(
        "storage.run_console_max_bytes",
        "Storage",
        "Most bytes one run keeps for its console, and the same again for its terminal log. The oldest part goes first.",
        POSITIVE,
    )),
    fixed(
        "agent.worktrees_path",
        "Agent",
        "Folder where each ticket gets its own git worktree.",
        SettingKind::String,
    ),
    fixed(
        "agent.worker_count",
        "Agent",
        "Total cap on agent jobs across all connectors.",
        POSITIVE,
    ),
    fixed(
        "agent.health_check_interval_secs",
        "Agent",
        "Seconds between agent health checks. Below 10 runs as 10.",
        POSITIVE,
    ),
];

const FIXED_AFTER_CONNECTORS: &[Fixed] = &[
    fixed(
        "workflow.auto_start_runs",
        "Workflow",
        "Assigning an agent to a ticket with a repository starts a run. When off, you click Run agent.",
        SettingKind::Bool,
    ),
    fixed(
        "workflow.auto_assign.default",
        "Workflow",
        "Auto-assign for every status without its own setting: Coppice assigns the agent a run names. When off, the ticket shows Recommends instead.",
        SettingKind::Bool,
    ),
    fixed(
        "workflow.auto_split.default",
        "Workflow",
        "Auto-split for every status without its own setting: Coppice creates the child tickets a run proposes. When off, it saves a pending split for you to approve.",
        SettingKind::Bool,
    ),
    fixed(
        "knowledge.enabled",
        "Knowledge",
        "Turns knowledge compaction on or off.",
        SettingKind::Bool,
    ),
    fixed(
        "knowledge.poll_interval_ms",
        "Knowledge",
        "How often the compaction scheduler wakes, in milliseconds. Below 100 runs as 100.",
        POSITIVE,
    ),
    fixed(
        "knowledge.compaction.interval_secs",
        "Knowledge",
        "Seconds between compaction cycles.",
        POSITIVE,
    ),
    fixed(
        "knowledge.compaction.batch_max_tickets",
        "Knowledge",
        "Most tickets in one batch.",
        POSITIVE,
    ),
    fixed(
        "knowledge.compaction.batch_max_source_bytes",
        "Knowledge",
        "Most source bytes in one batch.",
        POSITIVE,
    ),
    fixed(
        "knowledge.compaction.max_candidates_per_batch",
        "Knowledge",
        "Most candidates one batch can propose.",
        POSITIVE,
    ),
    fixed(
        "knowledge.compaction.max_attempts",
        "Knowledge",
        "Most tries for one batch.",
        POSITIVE,
    ),
    fixed(
        "knowledge.auto_save.enabled",
        "Knowledge",
        "Saves low-risk candidates without asking you.",
        SettingKind::Bool,
    ),
    fixed(
        "knowledge.auto_save.allowed_types",
        "Knowledge",
        "Types auto-save may save.",
        SettingKind::StringList {
            options: Some(AUTO_SAVE_TYPES),
        },
    ),
    fixed(
        "knowledge.auto_save.minimum_confidence",
        "Knowledge",
        "Must stay high.",
        SettingKind::Choice {
            options: &["high"],
        },
    ),
    fixed(
        "knowledge.retrieval.top_k",
        "Knowledge",
        "How many entries one lookup returns.",
        SettingKind::Integer {
            min: Some(1),
            max: Some(20),
        },
    ),
    fixed(
        "knowledge.retrieval.allowed_types",
        "Knowledge",
        "Only return these types. Empty returns every type.",
        SettingKind::StringList {
            options: Some(KNOWLEDGE_TYPES),
        },
    ),
    fixed(
        "knowledge.retrieval.minimum_confidence",
        "Knowledge",
        "Lowest confidence a lookup returns.",
        SettingKind::Choice {
            options: &["low", "medium", "high"],
        },
    ),
    fixed(
        "knowledge.retrieval.default_page_size",
        "Knowledge",
        "Default page size.",
        POSITIVE,
    ),
    fixed(
        "knowledge.retrieval.max_page_size",
        "Knowledge",
        "Largest page size.",
        SettingKind::Integer {
            min: Some(1),
            max: Some(100),
        },
    ),
    fixed(
        "knowledge.retrieval.max_active_per_board",
        "Knowledge",
        "The most Knowledge items, pending or approved, one board can hold. Items for agents on that board count too.",
        POSITIVE,
    ),
    fixed(
        "knowledge.retrieval.max_active_workspace",
        "Knowledge",
        "The most Knowledge items, pending or approved, that apply to the whole workspace.",
        POSITIVE,
    ),
    fixed(
        "git.author_name",
        "Git",
        "Author name on commits Coppice makes.",
        SettingKind::String,
    ),
    fixed(
        "git.author_email",
        "Git",
        "Author email on those commits.",
        SettingKind::String,
    ),
    fixed(
        "git.push_enabled",
        "Git",
        "Lets Coppice push branches and open pull requests.",
        SettingKind::Bool,
    ),
    fixed(
        "plugins.dir",
        "Plugins",
        "Your default plugin folder. Add more folders on the Plugins page.",
        SettingKind::String,
    ),
    fixed(
        "plugins.allow_file_git_urls",
        "Plugins",
        "Lets plugin installs use file: git URLs.",
        SettingKind::Bool,
    ),
    fixed(
        "plugins.git_timeout_secs",
        "Plugins",
        "Seconds before a plugin clone or update fails.",
        POSITIVE,
    ),
    fixed(
        "plugins.mcp_start_timeout_secs",
        "Plugins",
        "Seconds to wait for a plugin's MCP server to start.",
        POSITIVE,
    ),
    fixed(
        "plugins.mcp_list_timeout_secs",
        "Plugins",
        "Seconds to wait for a plugin to list its tools.",
        POSITIVE,
    ),
    fixed(
        "plugins.mcp_idle_shutdown_secs",
        "Plugins",
        "Seconds an idle plugin MCP server stays up.",
        POSITIVE,
    ),
    fixed(
        "mcp.builtin_plugins_dir",
        "MCP",
        "Where Coppice writes its built-in skills on every start. Edits there are lost.",
        SettingKind::String,
    ),
    fixed(
        "mcp.call_timeout_secs",
        "MCP",
        "Seconds before one tool call fails.",
        POSITIVE,
    ),
    fixed(
        "mcp.max_output_bytes",
        "MCP",
        "Largest tool result, in bytes.",
        POSITIVE,
    ),
    fixed(
        "mcp.comment_post_limit",
        "MCP",
        "Most comments one run can post.",
        POSITIVE,
    ),
    fixed(
        "mcp.token_ttl_secs",
        "MCP",
        "How long a run's tool token lasts, in seconds.",
        POSITIVE,
    ),
];

/// Every setting, in page order. `desktop` is the desktop app, whose defaults
/// differ (`git.push_enabled` is on unless the file says otherwise).
pub fn catalog(desktop: bool) -> Vec<SettingSpec> {
    let defaults = default_values(desktop);
    let mut specs = Vec::new();
    let push_fixed = |specs: &mut Vec<SettingSpec>, entry: &Fixed| {
        let path: Vec<String> = entry.key.split('.').map(str::to_string).collect();
        specs.push(SettingSpec {
            default: lookup(&defaults, &path).cloned(),
            path,
            group: entry.group.to_string(),
            description: entry.description.to_string(),
            kind: entry.kind.clone(),
            default_label: None,
            restart: entry.restart,
        });
    };
    for entry in FIXED_BEFORE_CONNECTORS {
        push_fixed(&mut specs, entry);
    }
    for (id, name) in CONNECTORS {
        specs.extend(connector_settings(&defaults, id, name));
    }
    for entry in FIXED_AFTER_CONNECTORS {
        push_fixed(&mut specs, entry);
        if entry.key == "workflow.auto_split.default" {
            specs.extend(status_settings(&defaults));
        }
    }
    specs.extend(shortcut_settings());
    specs
}

fn default_values(desktop: bool) -> Value {
    let mut config = AppConfig::load_defaults().unwrap_or_else(|_| AppConfig::default_values());
    if desktop {
        config.git.push_enabled = true;
    }
    Value::try_from(&config).unwrap_or(Value::Table(Default::default()))
}

fn lookup<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |value, segment| value.as_table()?.get(segment))
}

fn connector_settings(defaults: &Value, id: &str, name: &str) -> Vec<SettingSpec> {
    let group = format!("Connectors · {name}");
    let base = ["agent", "connectors", id];
    let path = |key: &str| -> Vec<String> {
        base.iter()
            .copied()
            .chain([key])
            .map(str::to_string)
            .collect()
    };
    let mut specs = vec![SettingSpec {
        path: path("enabled"),
        group: group.clone(),
        description: format!(
            "Turns {name} on or off. Automatic is on once Coppice finds the CLI and, where it can check for free, sees you're signed in."
        ),
        kind: SettingKind::OptionalBool {
            unset_label: "Automatic",
        },
        default: None,
        default_label: Some("Automatic".into()),
        restart: false,
    }];
    let fields: &[(&str, &str, SettingKind, bool)] = &[
        (
            "command",
            "The CLI binary to run.",
            SettingKind::String,
            id == "opencode",
        ),
        (
            "serve_hostname",
            "Hostname for OpenCode's helper during a run.",
            SettingKind::String,
            true,
        ),
        (
            "model_providers",
            "Model providers for this connector. Filled in when you turn it on.",
            SettingKind::StringList { options: None },
            false,
        ),
        (
            "run_timeout_secs",
            "Seconds before a run on this connector fails.",
            POSITIVE,
            false,
        ),
        (
            "max_concurrent_runs",
            "How many jobs this connector may run at once. Extra jobs stay queued and start oldest-first.",
            POSITIVE,
            false,
        ),
    ];
    for (key, description, kind, restart) in fields {
        let path = path(key);
        // Only fields this connector has, such as `command` (not Claude Code).
        let Some(default) = lookup(defaults, &path) else {
            continue;
        };
        specs.push(SettingSpec {
            default: Some(default.clone()),
            path,
            group: group.clone(),
            description: description.to_string(),
            kind: kind.clone(),
            default_label: None,
            restart: *restart,
        });
    }
    specs
}

fn status_settings(defaults: &Value) -> Vec<SettingSpec> {
    let mut specs = Vec::new();
    for (table, what) in [("auto_assign", "Auto-assign"), ("auto_split", "Auto-split")] {
        let fallback = lookup(
            defaults,
            &["workflow".into(), table.into(), "default".into()],
        )
        .and_then(Value::as_bool)
        .unwrap_or(false);
        for (status, name) in STATUSES {
            let path: Vec<String> = ["workflow", table, status]
                .iter()
                .map(|s| s.to_string())
                .collect();
            let default = lookup(defaults, &path).cloned();
            specs.push(SettingSpec {
                default_label: default
                    .is_none()
                    .then(|| format!("Uses default ({})", if fallback { "on" } else { "off" })),
                default,
                path,
                group: "Workflow".into(),
                description: format!("{what} for tickets in {name}."),
                kind: SettingKind::OptionalBool {
                    unset_label: "Uses default",
                },
                restart: false,
            });
        }
    }
    specs
}

fn shortcut_settings() -> Vec<SettingSpec> {
    crate::keybindings::default_commands()
        .into_iter()
        .map(|(command, keys)| SettingSpec {
            description: command.clone(),
            path: vec![crate::keybindings::SECTION.into(), command],
            group: "Keyboard shortcuts".into(),
            kind: SettingKind::Shortcut,
            default: Some(Value::Array(keys.into_iter().map(Value::String).collect())),
            default_label: None,
            restart: false,
        })
        .collect()
}

/// The value `text` sets at `path`, if any.
pub fn file_value(document: &toml::Table, path: &[String]) -> Option<Value> {
    let (last, parents) = path.split_last()?;
    let mut table = document;
    for segment in parents {
        table = table.get(segment)?.as_table()?;
    }
    table.get(last).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_file::{set_setting, validate_config_text};

    /// A value unlike the default, so the test sees the key take effect.
    fn changed_value(spec: &SettingSpec) -> Value {
        match &spec.kind {
            SettingKind::Bool | SettingKind::OptionalBool { .. } => Value::Boolean(
                !spec
                    .default
                    .as_ref()
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
            SettingKind::Integer { max, .. } => {
                let default = spec
                    .default
                    .as_ref()
                    .and_then(Value::as_integer)
                    .unwrap_or(1);
                Value::Integer(if max.is_some_and(|max| default >= max) {
                    default - 1
                } else {
                    default + 1
                })
            }
            SettingKind::IntegerChoice { options } => {
                let default = spec.default.as_ref().and_then(Value::as_integer);
                Value::Integer(*options.iter().find(|o| Some(**o) != default).unwrap())
            }
            SettingKind::String => Value::String("/changed/by-test".into()),
            SettingKind::Choice { options } => Value::String(options[0].to_string()),
            SettingKind::StringList { options } => Value::Array(vec![Value::String(
                options.map_or("changed", |options| options[0]).to_string(),
            )]),
            SettingKind::Shortcut => Value::String(String::new()),
        }
    }

    #[test]
    fn every_setting_is_a_real_key_the_server_reads() {
        for spec in catalog(false) {
            let key = spec.key();
            let value = changed_value(&spec);
            spec.check(&value)
                .unwrap_or_else(|err| panic!("{key}: {err}"));
            let text = set_setting("", &spec.path, Some(&value)).expect("set");
            let parsed = validate_config_text(&text).unwrap_or_else(|err| panic!("{key}: {err:?}"));
            if spec.kind == SettingKind::Shortcut {
                let command = &spec.path[1];
                assert!(
                    parsed.keybindings.api().overrides.contains_key(command),
                    "{key}"
                );
                continue;
            }
            let read = Value::try_from(&parsed).unwrap();
            assert_eq!(
                lookup(&read, &spec.path),
                Some(&value),
                "{key} did not take effect"
            );
        }
    }

    #[test]
    fn defaults_come_from_the_config_defaults() {
        let specs = catalog(false);
        let get = |key: &str| specs.iter().find(|s| s.key() == key).unwrap().clone();
        assert_eq!(get("logs.retention_days").default, Some(Value::Integer(14)));
        assert_eq!(get("git.push_enabled").default, Some(Value::Boolean(false)));
        let enabled = get("agent.connectors.opencode.enabled");
        assert_eq!(enabled.default, None);
        assert_eq!(enabled.default_label.as_deref(), Some("Automatic"));
        assert!(get("agent.connectors.opencode.serve_hostname").restart);
        assert!(!specs
            .iter()
            .any(|s| s.key() == "agent.connectors.claude-code.command"));
        assert!(!specs.iter().any(|s| s.key().ends_with("serve_port")));
        let backlog = get("workflow.auto_assign.backlog");
        assert_eq!(backlog.default, Some(Value::Boolean(false)));
        let ready = get("workflow.auto_assign.ready");
        assert_eq!(ready.default_label.as_deref(), Some("Uses default (on)"));
        assert_eq!(
            get("keybindings.palette.open").default,
            Some(Value::Array(vec![Value::String("Mod+K".into())]))
        );
        let desktop = catalog(true);
        let push = desktop
            .iter()
            .find(|s| s.key() == "git.push_enabled")
            .unwrap();
        assert_eq!(push.default, Some(Value::Boolean(true)));
    }

    #[test]
    fn check_refuses_values_that_do_not_fit() {
        let specs = catalog(false);
        let get = |key: &str| specs.iter().find(|s| s.key() == key).unwrap().clone();
        assert_eq!(
            get("logs.retention_days").check(&Value::Integer(5)),
            Err("Set logs.retention_days to 3, 7, 14, or 30.".into())
        );
        assert_eq!(
            get("knowledge.retrieval.top_k").check(&Value::Integer(21)),
            Err("Set knowledge.retrieval.top_k to 1–20.".into())
        );
        assert!(get("agent.worker_count")
            .check(&Value::String("8".into()))
            .is_err());
        assert!(get("knowledge.retrieval.allowed_types")
            .check(&Value::Array(vec![Value::String("nope".into())]))
            .is_err());
        assert!(get("keybindings.palette.open")
            .check(&Value::Array(vec![Value::String("Mod+P".into())]))
            .is_ok());
    }
}
