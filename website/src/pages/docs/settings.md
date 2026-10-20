---
layout: ../../layouts/Docs.astro
title: Settings reference
description: "Every field you can change in config.toml, with its type and default."
---

# Settings reference

Coppice keeps its settings in one file, `config.toml`:

| OS | Path |
| --- | --- |
| Linux | `~/.config/Coppice/config.toml` |
| macOS | `~/Library/Application Support/Coppice/config.toml` |

On Linux, if `XDG_CONFIG_HOME` is set, the file is at `$XDG_CONFIG_HOME/Coppice/config.toml`.

## Edit it in the app
Open **Settings** in the sidebar. The page shows the file as text, and your comments are kept.

- **Save** checks the file before writing it. A bad value isn't saved.
- **Restore last good version** undoes a bad change.

Some changes apply right away. Others show "Saved. Restart Coppice to apply."

You can also edit the file in any text editor. Your edits survive upgrades.

## Updates
Open **Settings → Updates** to see your version and choose **Check for updates**. **Automatic updates** is on by default. Turn it off, and Coppice stops checking for new versions in the background.

This switch isn't in `config.toml`. Coppice saves it as `automaticUpdates` in `update-settings.json`, in the same folder.

## Replace keys

If you shared a backup, make new keys and a new admin password. Saved tokens keep working, and everyone is signed out.

Open **System → Backup → Replace keys**, enter the new admin password, and confirm.

## Logs
Admins can open **System → Logs** to filter and live-tail server, request, agent run and database logs. Each run has a **View logs** link.

**Export logs** saves the lines that match your current filters as `coppice-logs.zip`. Coppice removes the tokens and keys it knows about, but check the file before you share it.

**Report a problem** opens a new GitHub issue with your Coppice version filled in. Say what happened and what you expected, then attach the zip from **Export logs**.

## `[logs]`

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `retention_days` | integer | `14` | How many days of logs to keep, including today: 3, 7, 14, or 30. The default is 14. Coppice deletes older logs every hour, and changes apply without a restart. If logs pass 256 MiB, Coppice deletes the oldest first, even before `retention_days` is up. |

Each day's log is split into files of up to 32 MiB, and every file counts toward the 256 MiB limit, today's too. Coppice never deletes the log file or the database log it's writing. Database logs count toward the limit, and Coppice deletes them once they pass `retention_days`.

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `dir` | string | `./data/logs` | Folder for logs. The desktop app uses its own folder and ignores this. Changing it needs a restart. |

## `[storage]`

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `run_artifacts_retention_days` | integer | `30` | Days a finished run's console and terminal log are kept. Archiving a ticket removes its runs' files right away. Below 1 runs as 1. Changing it needs a restart. |
| `run_console_max_bytes` | integer | `67108864` | Most bytes one run keeps for its console, and the same again for its terminal log. The oldest part goes first. Changing it needs a restart. |

## Connectors
Signed-in CLIs are on by default. You only need a line to turn one off:

```toml
[agent.connectors.claude-code]
enabled = false
```

The switch in **System → Connectors** writes the same line.

Kilo Code, GitHub Copilot, Qwen Code, Kimi Code and Grok stay off until you turn them on. Coppice can't check their sign-in for free.

## `[agent]`

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `worktrees_path` | string | `<data-dir>/worktrees` | Folder where each ticket gets its own git worktree. |
| `worker_count` | integer | `2` | How many agent jobs run at once. Below 1 runs as 1. |
| `health_check_interval_secs` | integer | `60` | Seconds between agent health checks. Below 10 runs as 10. |

`<data-dir>` is the folder that holds `config.toml`: `~/.config/Coppice` on Linux and WSL, and `~/Library/Application Support/Coppice` on macOS.

## `[agent.connectors.<id>]`

Ids: `claude-code`, `codex`, `cursor`, `opencode`, `kilo-code`, `copilot`, `qwen-code`, `kimi`, `grok`. Each table is optional.

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `enabled` | bool | not set: on if the CLI is signed in | Turns the connector on or off. |
| `model_providers` | list of strings | `[]` | Model providers for this connector. Filled in when you turn it on. |
| `run_timeout_secs` | integer | `1800` for OpenCode, `600` for others | Seconds before a run on this connector fails. |
| `command` | string | `cursor-agent` (Cursor), `codex`, `opencode`, `kilo`, `copilot`, `qwen`, `kimi`, `grok` | The CLI binary to run. Claude Code always runs `claude` and has no `command`. |

OpenCode only:

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `serve_hostname` | string | `127.0.0.1` | Hostname for OpenCode's helper during a run. Needs a restart. |

Changing OpenCode's `command` also needs a restart.

## `[workflow]`

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `auto_start_runs` | bool | `false` | When true, assigning an agent to a ticket with a repository starts a run. When false, you click **Run agent**. |
| `auto_assign.default` | bool | `true` | Turns auto-assign on for every status without its own setting. |
| `auto_assign.<status>` | bool | `backlog`: `false`. Others: uses `default` | Turns auto-assign on or off for one status. |
| `auto_split.default` | bool | `false` | Turns auto-split on for every status without its own setting. |
| `auto_split.<status>` | bool | uses `default` | Turns auto-split on or off for one status. |

Status keys: `backlog`, `ready`, `in_progress`, `in_review`, `in_qa`, `wait_for_final_review` (Wait for Human Review), `blocked`, `done`.

When auto-assign is on for that status, Coppice assigns the agent named by the run. When it's off, the ticket shows **Recommends:** instead.

When auto-split is on for that status, Coppice creates the child tickets the run proposes. When it's off, it saves a **Pending split recommendation** until you click **Approve splits**.

## `[knowledge]`

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Turns knowledge compaction on or off. |
| `poll_interval_ms` | integer | `500` | How often the compaction scheduler wakes. Below 100 runs as 100. |
| `compaction.interval_secs` | integer | `1800` | Seconds between compaction cycles. |
| `compaction.batch_max_tickets` | integer | `10` | Most tickets in one batch. |
| `compaction.batch_max_source_bytes` | integer | `200000` | Most source bytes in one batch. |
| `compaction.max_candidates_per_batch` | integer | `20` | Most candidates one batch can propose. |
| `compaction.max_attempts` | integer | `3` | Most tries for one batch. |
| `auto_save.enabled` | bool | `false` | Saves low-risk candidates without asking you. |
| `auto_save.allowed_types` | list of strings | `[]` | Types auto-save may save: `bug_pattern`, `coding_convention`, `dependency_note`, `performance_note`, `review_feedback`, `test_command`. |
| `auto_save.minimum_confidence` | string | `high` | Must stay `high`. |
| `retrieval.top_k` | integer | `8` | How many entries one lookup returns. 1 to 20. |
| `retrieval.allowed_types` | list of strings | `[]` (all types) | Only return these types: `coding_convention`, `architecture_rule`, `bug_pattern`, `test_command`, `review_feedback`, `dependency_note`, `api_contract`, `workflow_rule`, `human_preference`, `operational_runbook`, `security_rule`, `performance_note`. |
| `retrieval.minimum_confidence` | string | `medium` | `low`, `medium` or `high`. |
| `retrieval.default_page_size` | integer | `25` | Default page size. |
| `retrieval.max_page_size` | integer | `100` | Largest page size. 100 at most. |
| `retrieval.max_active_per_board` | integer | `1000` | The most Knowledge items, pending or approved, that one board can hold. Items for agents on that board count too. |
| `retrieval.max_active_workspace` | integer | `10000` | The most Knowledge items, pending or approved, that apply to the whole workspace. |

Every count and interval above must be greater than 0.

## `[git]`

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `author_name` | string | `Coppice` | Author name on commits Coppice makes. |
| `author_email` | string | `coppice@localhost` | Author email on those commits. |
| `push_enabled` | bool | `false` | Lets Coppice push branches and open pull requests. |

## `[plugins]`

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `dir` | string | `<data-dir>/plugins` | Your default plugin folder. Add more folders on the Plugins page. |
| `allow_file_git_urls` | bool | `false` | Lets plugin installs use `file:` git URLs. |
| `git_timeout_secs` | integer | `300` | Seconds before a plugin clone or update fails. |
| `mcp_start_timeout_secs` | integer | `20` | Seconds to wait for a plugin's MCP server to start. |
| `mcp_list_timeout_secs` | integer | `10` | Seconds to wait for a plugin to list its tools. |
| `mcp_idle_shutdown_secs` | integer | `600` | Seconds an idle plugin MCP server stays up. |

## `[mcp]`

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `builtin_plugins_dir` | string | `<data-dir>/builtin-plugins` | Where Coppice writes its built-in skills on every start. Edits there are lost. |
| `call_timeout_secs` | integer | `60` | Seconds before one tool call fails. |
| `max_output_bytes` | integer | `32768` | Largest tool result, in bytes. |
| `comment_post_limit` | integer | `5` | Most comments one run can post. |
| `token_ttl_secs` | integer | `14400` | How long a run's tool token lasts, in seconds. |

## `[keybindings]`

Each key is a command id from [Keyboard shortcuts](/docs/shortcuts). Your keys replace that command's defaults, and an empty value turns the shortcut off.

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `"<command id>"` | string or list of strings | the built-in shortcut | Replaces that command's keys. `""` removes them. |

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
"board.nextCard" = ["J", "Down"]
```

`Mod` is Cmd on macOS and Ctrl on Linux and WSL. A sequence is written `G then B`. Several keys for one command are separated with ` or `, or written as a list. Save applies the change without a restart. A shortcut Coppice can't read, an unknown command id, a key another command already has, or a key that belongs to the browser or the system is skipped. That command keeps its default, and Settings explains why.
