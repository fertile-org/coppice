# M13 — Security & sandbox

## Goal

Fail closed. A new agent can read and write its ticket worktree and a Coppice-managed home folder, and it can reach only its model provider, that repo's git remote, and the package registries listed below. Anything else waits for the user.

The user decides on the ticket: **Allow once**, **Always allow**, or **Deny**. Always allow applies to **This agent** (the default) or **All agents**. Every strict rule has a switch in Settings. The screen says what is actually enforced on this machine, for this agent CLI. These limits are not a virtual machine.

This is the source of truth for M13. No code lands ahead of it. It replaces the earlier M13 note, which assumed a Compose-era admin console and a process wrapper around every CLI.

Refs [#30](https://github.com/fertile-org/coppice/issues/30). Related: [#10](https://github.com/fertile-org/coppice/issues/10) pins the CLI flags this milestone translates policy into, and records the CLI version on the run.

## Problem

Today every run is stored with `sandbox_profile_id = "permissive-default"` (`server/src/sandbox/permissive.rs`). That id is a label. Nothing reads it.

The CLI is started by `cli_runner` and placed in its own session by `process_tree` (`setsid`). The child inherits the server's environment, including the user's real `HOME`. Launch flags turn the CLI's own checks off:

| Connector | What Coppice passes today | What that does |
| --- | --- | --- |
| `claude-code` | `--permission-mode bypassPermissions` | Skips Claude's permission prompts and lets an unsandboxed retry through |
| `codex` | `--dangerously-bypass-approvals-and-sandbox` | Turns Codex's Seatbelt / bubblewrap sandbox off |
| `cursor` | `--force` on ticket runs | Skips Cursor's approval prompt. Chat uses `--mode ask` |
| `kilo-code` | `--auto` | Auto-approves inside Kilo |
| `opencode` | per-run `opencode serve` | No OS sandbox. Chat write-denial is prompt text |

Cursor already points `HOME` at a directory under the artifacts dir, then points `XDG_CONFIG_HOME` back at the real `~/.config` and forwards `GIT_CONFIG_GLOBAL` and `GH_CONFIG_DIR` when those files exist. The other connectors use the real home so they can find their sign-in. That home also holds `~/.ssh`, `~/.aws`, browser data, and every other repo.

Plugin MCP stdio servers (`mcp/proxy/stdio.rs`) clear the environment and then inherit `PATH`, `HOME`, `LANG`, and `TMPDIR`. The pool is one shared process per `(plugin, server)` for every agent (`mcp/proxy/pool.rs`). They run as the server user. The architecture doc already says they stay that way until M13.

`POST /mcp` checks the per-run token and, for chat, drops tools that are not marked read-only (`ToolRegistry::allowed_for`). It does not check a capability policy. Chat write-denial for Codex and OpenCode is instructions in the prompt. Kilo refuses chat outright because its descriptor has `read_only_tools: false`.

Secrets (M07) are AES-256-GCM blobs in `secrets`, keyed from `[secrets] master_key`. A repo can point `forge_token_secret_id` at one. Nothing injects a secret into an agent, and nothing records who granted what.

Tickets already have `Blocked` and the substatuses `blocked_by_missing_capability`, `blocked_by_missing_secret`, `blocked_by_permission`, and `blocked_by_error`. `result_submit` maps `blockerType` onto those. The ticket metadata panel can display them. There is no approval card, no resume-on-grant, and no audit log.

## Non-goals

- A virtual machine, a container per run, gVisor, or Firecracker. Framework selection §4's sandbox v2 and v3 stay later.
- Resistance to a kernel exploit, a malicious Coppice build, or a user who turns the limits off.
- A native Windows sandbox. The beta has no Windows app. Windows 11 through WSL runs the Linux build ([install doc](../../website/src/pages/docs/install.md)); that path uses the Linux backend below and is honest about Windows interop.
- Role presets (DBA, QC, Security, …) and workspace signals. Those stay in [M14](./M14-role-owner-agents.md). M13 has the rule model they will grant into.
- The CLI-version canary in [#10](https://github.com/fertile-org/coppice/issues/10). M13 records the version and the flag set. The canary ticket stays on that issue.
- Rewriting `process_tree`. The wrapper process is the child it already tracks. A grandchild that calls `setsid` can still leave the group; that limitation stays.
- Multi-user RBAC. Desktop mode is one local admin.

## Threat model

Coppice runs on the user's machine. The agent CLI is a normal program of that user. A ticket, a repo file, or a web page can instruct it to read `~/.ssh`, call a cloud CLI, or send data to an unexpected host.

M13 reduces that blast radius:

- Files outside the worktree and the managed home are not visible to the agent, except the few sign-in files its CLI needs.
- Network destinations outside the allowlist do not connect.
- A secret value is in the process environment only after a grant for that agent, and it is scrubbed from logs and comments.
- A plugin process for that agent gets the same file and network limits. A remote plugin server is not on this machine; Coppice can only refuse the tool call.
- The user sees the request and the grant, and can revoke it.

A granted secret can still be sent to a host that is already allowed. A program that breaks Seatbelt or bubblewrap can still escape. The product says so on the agent.

## Decisions

Hung's product rules are requirements. The rest of this section is the design that implements them on the code that exists.

1. **Strict by default.** New runs use the policy in [Default policy](#default-policy). Registries start on, each with its own switch.
2. **Ask instead of fail.** The agent calls the Coppice tool `request_permission`. The ticket shows the request. **Allow once**, **Always allow**, or **Deny**. A grant resumes work.
3. **Always allow** asks where it applies. **This agent** is selected. **All agents** is the other choice. Saved rules live under Settings → Access and can be revoked there. The same list is on the agent page, filtered to that agent.
4. **Settings toggles** relax a rule for the whole workspace. The screen states the relaxed rule in a sentence, not only with a switch.
5. **Honest enforcement.** Three levels: **enforced**, **best-effort**, **not enforced**. The agent page and the run show the level for this connector on this machine. Copy never says the machine is locked down.
6. **The CLI's sandbox first.** Claude Code and Codex already enforce with macOS Seatbelt and Linux bubblewrap. Coppice translates policy into that CLI's per-run settings. Coppice adds its own wrapper only when the CLI has no sandbox, or the sandbox cannot express the rule. Wrappers are not nested: a second bubblewrap around Claude or Codex fails inside the first.
7. **One policy engine.** Files, hosts, commands, secrets, and MCP tools are rules. Connector id strings do not appear in the policy engine. A new connector adds one adapter and one descriptor block. The Settings screen and the ticket card do not change.
8. **Denied hosts are learned from a proxy that sees the name.** Seatbelt and bubblewrap deny by address and return a generic error to `npm`. The strategy is in [Spotting a block](#spotting-a-block).

## Architecture

```text
Resolved policy (no connector ids)
        │
        ├─ MCP gateway  tools/call, including request_permission
        │
        ├─ Secret injection + redaction
        │
        └─ Connector sandbox adapter
                ├─ native CLI settings/flags     Claude Code, Codex
                └─ Coppice OS backend + egress   Cursor, OpenCode, Kilo, plugin stdio
                        macOS Seatbelt
                        Linux bubblewrap (including WSL)
```

Business rules stay in `server/src/services/` and `server/src/domain/`. Handlers stay thin. Mutations keep the session cookie and `X-CSRF-Token`.

Suggested layout, matching modules that already exist:

```text
connectors/src/lib.rs
  SandboxSupport on ConnectorDescriptor     what the UI can claim

server/src/sandbox/
  permissive.rs      test-only profile id, behind sandbox.enforce = false
  policy.rs          rule types, resolver, default policy. No connector ids
  launch.rs          managed home, env allowlist, credential mounts
  proxy.rs           loopback CONNECT proxy and denial records
  detect.rs          turns a proxy denial or an adapter event into a permission request
  os/macos.rs        sandbox-exec profile
  os/linux.rs        bwrap argv
  audit.rs           append-only audit rows

server/src/providers/<connector>.rs
  the adapter: policy → flags/files, stdout → denial events

server/src/mcp/registry.rs
  policy check before dispatch; request_permission waits
```

`policy.rs` gets the same treatment as connector ids elsewhere: a unit test fails if a connector id literal appears in it.

### Policy model

A rule is one allow or one deny.

| Field | Values |
| --- | --- |
| `kind` | `filesystem_read`, `filesystem_write`, `network_host`, `command`, `secret`, `mcp_tool` |
| `pattern` | Path prefix, host (a leading `*.` matches one label), command prefix, secret name, or MCP tool name (`ticket_get`, `github__create_issue`) |
| `effect` | `allow` or `deny`. Deny wins |
| `scope` | `agent` or `workspace` |
| `agent_id` | Set when `scope` is `agent` |
| `source` | `default`, `toggle`, `grant`, `capability` |

A **capability** is a named bundle of rules (a command, a secret, and a host together). Granting it writes those rules at the chosen scope. M13 does not ship the role bundles from product design §18.1. The ticket can allow one resource without inventing a bundle name.

The resolver merges, in order: built-in default, workspace toggles, capability grants, saved rules. Deny wins over allow. A toggle that relaxes a class drops the default denies of that class. It does not drop an explicit deny the user saved.

These paths stay denied when every wide toggle is on. No switch opens them:

- The Coppice data dir's `secrets/` and `config.toml`
- The database directory (`pg/`)
- Another agent's managed home

The agent page says, when a wide file toggle is on: "Agents can read and write your files. Coppice's saved secrets stay hidden."

A run stores a snapshot of the resolved rules and the enforcement levels, so a later settings change does not rewrite history.

### Per-agent profile

The profile is the resolved rules for that agent plus the run's context profile (`full`, `human_agent`, `human_chat`, `conversation`, `knowledge_compaction`, `connector_check`). There is no sandbox-profile editor.

Chat write-denial (M09) is the filesystem-write rule for `human_chat` and `conversation`: the bound repo is readable, not writable. `ToolRegistry::allowed_for` becomes that profile's MCP slice of the same rules, including "read-only tools plus `result_submit`". It stops being a special case beside the policy.

`knowledge_compaction` stays a scratch directory, no repo, no plugin tools. `connector_check` stays the scratch dir it already uses.

`agent_runs.sandbox_profile_id` remains. New runs write `strict-default`. The column is the profile name; the snapshot is the rules.

### Connector adapter

Static facts live on `ConnectorDescriptor` in `connectors/src/lib.rs`, next to `caps` and `mcp_wiring`. The crate stays `serde` plus std. The server, `GET /api/connectors`, and Tools → Connectors already read this table.

```rust
pub enum Enforcement {
    Enforced,
    BestEffort,
    NotEnforced,
}

pub enum DenialSource {
    /// The CLI's own proxy or stream event names the host or path.
    CliEvents,
    /// Coppice's egress proxy is the network source. Files come from the OS backend log.
    CoppiceProxy,
    None,
}

pub struct SandboxSupport {
    pub filesystem: Enforcement,
    pub network: Enforcement,
    pub denial_source: DenialSource,
    /// True when Coppice must apply the OS backend. False when the CLI sandbox is the owner.
    pub needs_os_wrapper: bool,
    /// Hosts the CLI itself needs for the model. Not a Settings list.
    pub model_hosts: &'static [&'static str],
}
```

The levels on the descriptor are what this connector can do when its backend is available. Claude, Codex, Seatbelt, and bubblewrap read those limits when the process starts. [When a grant takes effect](#when-a-grant-takes-effect) says which grants can change a running process and which ones start a follow-up run. The run's recorded level can be lower: bubblewrap missing, CLI too old, or a toggle off. The UI shows the run's level.

Translation stays in the provider adapter, the same place `mcp/wiring.rs` already builds per-run MCP files. The adapter does not write the user's real CLI config and does not write the worktree. Files go under `run_dir` (`<artifacts_dir>/runs/<run_id>/`), which is already absolute so a relative path cannot land in the repo.

```text
SandboxLaunch {
  args: Vec<String>              // appended to the existing CLI argv
  env: Vec<(String, String)>     // added to the allowlist, not a full environment
  files: Vec<(PathBuf, String)>  // per-run settings, mode 0600
  needs_os_wrapper: bool
  denial_source: DenialSource
}
```

Each adapter also parses its stdout JSON into zero or more denial events: `{ kind, resource }`. Parsing is allowlisted per event shape. A generic "permission denied" line is not an event.

Adding a connector, on top of [architecture.md § Adding a connector](../architecture.md#adding-a-connector):

1. Set `SandboxSupport` on the descriptor.
2. Implement `SandboxLaunch` and the denial parser in that provider file.
3. Record the CLI version and the full argv on the run (the pin [#10](https://github.com/fertile-org/coppice/issues/10) asks for).

The policy engine, the ticket card, and Settings → Access stay as they are.

If the CLI rejects the sandbox flag, or the version is older than the floor pinned in the adapter, the adapter does not fall through to today's bypass flags. The run fails with a clear error and the enforcement level is `not enforced`. Updating the floor is a deliberate change, logged with the version.

### OS backends

Used only when `needs_os_wrapper` is true, or when a native sandbox failed to start and this run is not allowed to continue without it.

| Platform | Backend | Notes |
| --- | --- | --- |
| macOS (beta: Apple Silicon) | `sandbox-exec` Seatbelt profile | No extra install. Profile allows the worktree, managed home, read-only sign-in files, and OS libraries. Network is the proxy, loopback, and the adapter's model hosts |
| Linux x64, including WSL2 | `bwrap` | The `.deb` depends on `bubblewrap` and `socat`. Ubuntu 24.04 and later, including Ubuntu on WSL, often set `kernel.apparmor_restrict_unprivileged_userns=1`. Packaging documents the `bwrap` AppArmor profile from the [Claude sandbox docs](https://code.claude.com/docs/en/sandboxing), or the run fails closed with the message below |
| Docker Compose | Same Linux backend | User namespaces often do not work in the container. See [Migration](#migration) |
| Native Windows | None | No installer. A from-source run reports `not enforced` and does not pretend otherwise |

The desktop app **does not start the agent** when the wrapper is required and the backend cannot start, unless the user has turned on "Run agents when limits can't be applied". The ticket is Blocked — error, with:

> This machine can't limit agent programs yet. Agents are paused. You can fix the Linux setup, or turn limits off in Settings.

WSL interop is a hole: a sandboxed Linux process can still start a Windows binary under `/mnt/c` unless the Unix socket that launches it is blocked. The Linux profile denies `/mnt/c` and, when the optional seccomp helper is absent, the card says:

> Running in WSL. Limits apply to Linux programs. A Windows program the agent starts can get around them.

That matches the install doc's warning about CLI paths under `/mnt/c`.

Coppice does not wrap a process whose adapter already owns a Seatbelt or bubblewrap sandbox. Nested `bwrap` fails with "Operation not permitted", and Claude would then skip the sandbox unless `failIfUnavailable` is set. The adapter sets that flag so a failed native sandbox stops the run instead of continuing wide open.

### Managed home and credentials

Desktop v1 intentionally uses the real `HOME` so each CLI finds its existing login. M13 keeps the login and drops the rest of the home.

Every connector gets:

- `HOME` = `<artifacts_dir>/agent-homes/<agent_id>/` (stable across runs so a resume still finds the CLI's session state)
- `TMPDIR` = that home's `tmp/`
- An environment built from a cleared list: `PATH`, `LANG`, `TMPDIR`, `COPPICE_MCP_URL`, `COPPICE_MCP_TOKEN`, the descriptor's `auth_env` names when they are set, and secrets granted to this run

Not passed through: `AWS_*`, `SSH_AUTH_SOCK`, `DATABASE_URL`, `SECRETS_MASTER_KEY`, `GH_TOKEN`, and the rest of the server environment.

Sign-in files are bind-mounted or copied read-only into the managed home at the path the CLI expects. The mount is the credential file, not the whole config directory. In particular, Codex's `~/.codex/config.toml` and Claude's `~/.claude/settings.json` are not mounted: those files can turn the sandbox off. Coppice writes the per-run policy itself.

| Connector | Mounted sign-in | Left behind |
| --- | --- | --- |
| `claude-code` | `~/.claude/.credentials.json` when present. `ANTHROPIC_API_KEY` / `CLAUDE_CODE_OAUTH_TOKEN` via `auth_env` | The rest of `~/.claude`, including project memory and user settings |
| `codex` | `~/.codex/auth.json`. `OPENAI_API_KEY` via `auth_env` | `~/.codex/config.toml` |
| `cursor` | `cursor/auth.json` from `~/.config/cursor/` or `~/.cursor/` | The rest of `~/.config`. Stop setting `XDG_CONFIG_HOME` to the real config home. Stop forwarding `GH_CONFIG_DIR` |
| `opencode` | `~/.local/share/opencode/auth.json` | `~/.opencode` (install tree; the probe already ignores it) |
| `kilo-code` | The auth file the live CLI actually reads, once [#10](https://github.com/fertile-org/coppice/issues/10) / M10's live check names it. Until then the card says sign-in files are not enforced | A whole `~/.local/share/opencode` or `~/.kilocode` tree |

`GIT_CONFIG_GLOBAL` points at a gitconfig inside the managed home. It sets the proxy when this run uses Coppice's egress proxy, and a credential helper only when a forge-token grant exists for this repo. The helper reads a `0600` file in the managed home. The token is not put on the command line. The user's `~/.ssh` is not mounted. Agent `git push` and `gh` stay off until that grant. Coppice's own push and create-PR actions (M07) stay on the server, behind `git.push_enabled`, and do not run inside the agent.

### Spotting a block

Chosen strategy: **a proxy that sees the hostname, plus the adapter's structured events.** Not free-form stderr.

`npm install` against a host outside the list usually dies with `EPERM`, `ECONNREFUSED`, or a DNS failure. The agent reports a network error and does not know Coppice refused the host. Seatbelt and bubblewrap do not log the name the program asked for. Matching `npm ERR!` misses pip, cargo, curl, and the next package manager.

Two proxies, one denial record:

1. **The CLI's proxy, when it has one.** Claude Code sends sandboxed commands through its own proxy and names the denied host in the tool result. Codex does the same when `features.network_proxy` is on. The adapter parses that event. Coppice does not put a second proxy in front of these CLIs.
2. **Coppice's loopback CONNECT proxy, for everyone else.** Cursor, OpenCode, Kilo, and plugin stdio processes get `HTTP_PROXY`, `HTTPS_PROXY`, and `ALL_PROXY` pointed at `127.0.0.1`, and a managed gitconfig `http.proxy`. The OS backend allows outbound TCP only to that proxy, to loopback (the MCP gateway), and to the adapter's model hosts (some CLIs do not honor the proxy for their own API). The proxy allow-or-denies on the CONNECT name and writes a denial row: run id, host, time.

A denial becomes one `permission_requests` row. The ticket shows it immediately, while the run is still going. The same host is not asked twice in one run.

What does **not** raise a prompt: a non-zero exit, the words "permission denied", or a timeout. Those stay ordinary failures. False prompts train the user to click Allow.

Filesystem and command denials are not visible to an HTTP proxy.

- Claude and Codex: the adapter's structured events (denied path, denied command).
- Coppice's Seatbelt profile: the profile is launched with denial logging, and `detect.rs` reads that log.
- bubblewrap: a denied file is `EACCES` with no host-style name. The card's filesystem level stays **enforced** for the mount set (the file was not there) and **best-effort** for explaining *which* path failed. The agent can still call `request_permission` with the path.

When the user allows a host, the Coppice proxy learns it immediately. A CLI allowlist is rewritten before the follow-up process starts, so the retry is not denied again.

### MCP tool checks

`ToolRegistry::call` is the choke point (architecture: one router, then `run_tool_calls`). Before `source.call`:

- The tool must be on the resolved policy for this run. Chat's read-only filter is that check, not a parallel one.
- A miss returns a tool error the agent can read: `not allowed: <tool>. Call request_permission, or submit a blocked result.`
- The same miss opens a permission request of kind `mcp_tool` so the ticket shows **Enable tool** even if the agent never asks.

`request_permission` is a core tool, on every profile including chat and the connector check.

```json
{
  "kind": "network_host",
  "resource": "registry.npmmirror.com",
  "why": "The lockfile downloads from this mirror."
}
```

`kind` is the policy kind. `resource` is one host, one path, one command prefix, one secret name, or one tool name. `why` is the sentence on the ticket.

The tool **waits**. It is exempt from `mcp.call_timeout_secs` (60). The wait ends when the user decides or the run's own timeout fires. On timeout the ticket stays blocked and the tool result is `denied: no decision before the run timed out`.

Decisions the tool returns to the agent:

| User choice | Tool result | What was saved |
| --- | --- | --- |
| Allow once | `allowed for this run` | A rule with source `grant` for this run and, when a follow-up is required, for that one follow-up run. It does not stick after that |
| Always allow, This agent | `allowed for this run` | An agent-scoped rule |
| Always allow, All agents | `allowed for this run` | A workspace-scoped rule |
| Deny | `denied` | The request row only |

The agent does not learn the scope. It learns whether it may proceed. That return happens only when the grant applies inside the waiting process. When it does not, the process is stopped instead, as [When a grant takes effect](#when-a-grant-takes-effect) describes.

### When a grant takes effect

Claude, Codex, Seatbelt, and bubblewrap read their allowlists at process start. They do not reload a settings file or a profile while the process is running. A secret is an environment variable, also fixed at start.

| Grant | Takes effect |
| --- | --- |
| MCP tool | In the waiting process. `request_permission` returns, and the next tool call is checked against the new rule |
| Network host, Coppice egress proxy (Cursor, OpenCode, Kilo, plugin stdio) | In the waiting process. The proxy's list is live, so a retry of the same command can connect |
| Network host or path, Claude Code or Codex | Follow-up run. The per-run settings are rewritten, then the process is stopped and resumed so the CLI loads them |
| Filesystem path, Coppice Seatbelt or bubblewrap | Follow-up run. The profile is built at `exec` |
| Secret | Follow-up run. The variable is in the new environment |
| Command | Follow-up run when a native sandbox or a saved deny would have blocked it. The default profile does not deny commands by name |

Allow once and Always allow use the same buttons either way. When the grant needs a new process, Coppice does not leave the agent blocked on a tool result that would still fail. It records the decision, stops the process group through the existing cancel path, and queues the resume run. The new turn is told what was allowed. `request_permission` does not return in that case; the process is already gone.

Deny never starts a run.

### Approval, Blocked, and resume

While the decision can apply in-process, the MCP call stays parked, the ticket shows the card, and a notification of kind `permission_requested` uses the card's sentence. Allow or Deny unblocks the tool. The run continues.

A detected block cannot un-fail the command that already died. The card still comes up immediately. If the grant is live (the Coppice proxy), a retry in the same process works and the ticket stays In Progress.

The ticket moves to `Blocked` only when the process has exited and the user has not decided yet, or when they choose Deny. An Allow that needs a follow-up run queues that run and the usual start transition puts the ticket back In Progress. Blocked uses the result-contract fields that already exist:

- Substatus from the kind: `blocked_by_permission` (host, path, command, tool), `blocked_by_missing_capability` (a bundle), `blocked_by_missing_secret` (secret name only), `blocked_by_error` (backend missing)
- `substatus_metadata` includes `permissionRequestId`, `kind`, and `resource`. The metadata panel already keys capability and secret off this object; extend it rather than adding a status

**Allow once** and **Always allow** queue that follow-up through the existing resume path (`resume_session_id`, connector `caps.run_resume` / `chat_resume`) whenever the grant needs a new process, including when the user decides before the CLI exits. The follow-up context says what the user allowed. Connectors that cannot resume (Kilo; Codex is best-effort and falls back to the transcript) get a fresh session with that sentence. **Deny** does not start a run. It posts the denial on the ticket and leaves the ticket Blocked.

**Ask the agent why** posts a comment and queues that follow-up with the question. It is the same resume path, without granting.

Guided actions on the Blocked ticket, shown only when they match the request:

| Action | When |
| --- | --- |
| Allow once / Always allow / Deny | Always |
| Allow command | `kind` is `command` |
| Add secret | `kind` is `secret`. Name and value form. The value is write-only |
| Grant capability | The request is a bundle |
| Enable tool | `kind` is `mcp_tool` |
| Reject | Same as Deny |
| Ask the agent why | Always |

Copy on the card is the resource and the agent's `why`, then the buttons. Example: "Claude wants to reach registry.npmmirror.com. The lockfile downloads from this mirror."

### Secrets

The `secrets` table and `SecretService` stay. Grants are new rows, not new ciphertext.

Injection is an environment variable on the agent process and, when the grant includes a plugin, on that agent's plugin process. The name is the secret's name. The value is never written to a prompt, `.agent/context.md`, a comment, a tool result, an API response, or the audit detail.

Redaction reuses the plugin MCP `Redactor`: every CLI stdout and stderr line, and every comment body the run creates, is scrubbed for granted values before it is stored or streamed. Scrubbing a value that is shorter than 8 characters is skipped so ordinary words are not eaten; those secrets are still kept out of prompts.

The Settings list shows name, which agents, which plugins, which repos, and when it was created. After save, the value is not shown again. This is the M07 forge-token screen, extended. It is not a new product area.

A repo's forge token is eligible to be granted to an agent for that repo. It is not injected on its own.

### Plugin MCP servers

stdio servers run **per agent**, not once for the workspace. The pool key gains `agent_id`. Idle shutdown (`mcp_idle_shutdown_secs`) still applies. Two agents with the same plugin do not share a process, because their homes, secrets, and host lists differ.

The plugin process uses that agent's managed `HOME`, `TMPDIR`, and network policy. Its cwd stays the plugin root for the server itself; tool calls that need the worktree receive the path as an argument, the way they do today. The Coppice OS backend applies, including when the agent CLI is using its own sandbox: the plugin is not inside the CLI. The card says: "Plugin programs are limited by Coppice. They are not inside Claude Code's sandbox."

Remote HTTP plugin servers stay remote. The tool call is policy-checked. If the server's URL host is outside the agent's network rules, the call is refused before connect. The card says the remote program is not limited by this machine.

Enabling a plugin still shows the existing confirmation. The confirmation gains one sentence when the plugin has a stdio server: "This plugin runs on your machine, under the same limits as the agent that uses it."

### Audit log

Append-only `audit_log`. The actor is the desktop admin user. Detail JSON has ids and names, never a secret value, never a token.

Recorded actions: rule granted, rule revoked, secret created, secret granted, secret deleted, sandbox toggle changed, capability granted, plugin installed, plugin enabled, branch pushed, pull request created.

Settings → Access shows the latest rows, filterable, read-only. A grant also appears on the ticket as a normal comment so the board shows the work: "You allowed registry.npmmirror.com for this agent."

## Data model

New migration. Existing tables are extended, not replaced.

```text
policy_rules
  id              UUID PRIMARY KEY
  kind            TEXT NOT NULL
  pattern         TEXT NOT NULL
  effect          TEXT NOT NULL          -- allow | deny
  scope           TEXT NOT NULL          -- agent | workspace
  agent_id        UUID REFERENCES agents(id) ON DELETE CASCADE
  source          TEXT NOT NULL          -- default rows are not stored; grant | capability | toggle-exception
  capability_id   UUID REFERENCES capabilities(id) ON DELETE SET NULL
  created_at      TIMESTAMPTZ NOT NULL
  revoked_at      TIMESTAMPTZ

capabilities
  id              UUID PRIMARY KEY
  name            TEXT NOT NULL
  description     TEXT NOT NULL

capability_rules
  capability_id   UUID REFERENCES capabilities(id) ON DELETE CASCADE
  kind            TEXT NOT NULL
  pattern         TEXT NOT NULL
  effect          TEXT NOT NULL

permission_requests
  id              UUID PRIMARY KEY
  run_id          UUID REFERENCES agent_runs(id) ON DELETE CASCADE
  ticket_id       UUID REFERENCES tickets(id) ON DELETE CASCADE
  agent_id        UUID REFERENCES agents(id) ON DELETE CASCADE
  kind            TEXT NOT NULL
  resource        TEXT NOT NULL
  why             TEXT NOT NULL
  status          TEXT NOT NULL          -- pending | allowed_once | allowed_always | denied | expired
  scope           TEXT                   -- agent | workspace, set when allowed_always
  rule_id         UUID REFERENCES policy_rules(id)
  created_at      TIMESTAMPTZ NOT NULL
  decided_at      TIMESTAMPTZ
  decided_by      UUID REFERENCES users(id)

secret_grants
  id              UUID PRIMARY KEY
  secret_id       UUID NOT NULL REFERENCES secrets(id) ON DELETE CASCADE
  agent_id        UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE
  plugin_id       UUID REFERENCES plugins(id) ON DELETE CASCADE
  repo_id         UUID REFERENCES repos(id) ON DELETE CASCADE
  created_at      TIMESTAMPTZ NOT NULL

Unique where `plugin_id` and `repo_id` are null: `(secret_id, agent_id)`. A second unique index covers the same pair plus a non-null `plugin_id`. A third covers a non-null `repo_id`. Postgres partial unique indexes, so a missing plugin or repo is not stored as a fake id.

audit_log
  id              UUID PRIMARY KEY
  created_at      TIMESTAMPTZ NOT NULL
  actor_user_id   UUID REFERENCES users(id)
  action          TEXT NOT NULL
  agent_id        UUID
  ticket_id       UUID
  run_id          UUID
  detail          JSONB NOT NULL
```

`agent_runs` gains:

- `policy_snapshot JSONB NOT NULL` default `'{}'` for old rows
- `enforcement JSONB NOT NULL` default `'{}'` — `{ "filesystem", "network", "backend", "cliVersion", "note" }`
- `sandbox_profile_id` stays `TEXT`. New value `strict-default`

`backend` is one of `claude-sandbox`, `codex-sandbox`, `coppice-seatbelt`, `coppice-bwrap`, `none`.

Indexes: pending permission requests by ticket, rules by agent where `revoked_at` is null, audit by `created_at` desc.

### API

Admin session, CSRF on writes, same style as the connectors routes.

```text
GET    /api/settings/access
PUT    /api/settings/access                 patches [sandbox] in config.toml
GET    /api/access/rules?agentId=
DELETE /api/access/rules/:id
GET    /api/agents/:id/access               resolved rules + enforcement for this machine
POST   /api/permission-requests/:id/decide  { "decision": "allow_once"|"allow_always"|"deny", "scope": "agent"|"workspace" }
GET    /api/secrets                         names and grants only
POST   /api/secrets                         { name, value } — value write-only
POST   /api/secrets/:id/grants
DELETE /api/secrets/:id
GET    /api/audit-log?action=&agentId=
```

No `/api/sandbox-profiles` catalog. The profile is code.

`GET /api/tools/connectors` gains the enforcement this machine can actually apply for that CLI, so the Connectors card can say it next to Ready / Not on your PATH.

## config.toml

TOML stays. Workspace switches live here because the desktop app already treats `config.toml` as the operator file: connector toggles patch it in place and keep comments, and Settings can save the whole file. Per-agent grants stay in Postgres; they change while a run is waiting and must not rewrite the file out from under the editor.

Missing keys mean the defaults below. Existing files keep working. The desktop generator does not rewrite a config it has already written; the first launch after this milestone patches in a `[sandbox]` block the same way connector enable patches `[agent.connectors.*]`.

```toml
[sandbox]
enforce = true
# Desktop stays false. The Compose example sets true: user namespaces often
# do not work in that container. The UI still says limits are not enforced.
allow_unenforced = false
allow_all_network = false
allow_all_filesystem = false
allow_user_home = false

[sandbox.registries]
npm = true
yarn = true
pypi = true
crates = true
go = true
rubygems = true
maven = true
```

`enforce = false` is the test and break-glass switch. It restores today's behavior (`permissive-default`, inherited `HOME`, current CLI flags). The UI banner is the same sentence as "limits are off", and it is not the default in the root example or the desktop file.

`SECRETS_MASTER_KEY` already overrides `[secrets] master_key`. No new `SANDBOX_ENFORCE` environment variable. Compose does not grow a sandbox env knob.

Root `config.example.toml` and the desktop file use the defaults above. `deploy/config/config.example.toml` sets `allow_unenforced = true` with the comment in the sample. That split is the only Compose-specific sandbox setting.

## UX

The team's rule: show the work, hide the machinery. User-facing strings do not say sandbox, Seatbelt, bubblewrap, policy, profile, connector id, or the test provider's name. Buttons allow or deny. Ticket and plan approval stay the words the product already uses. Diffs are accepted.

### Approval card

On the ticket, above the comment field, while a request is pending:

> **Claude wants to reach registry.npmmirror.com.**
> The lockfile downloads from this mirror.
>
> [Allow once]  [Always allow]  [Deny]
>
> Always allow applies to **This agent**. Change to **All agents**.

**This agent** is the selected choice. **All agents** is one click away, not a second screen. Deny needs no confirm. Allow always leaves a comment on the ticket.

### Blocked ticket

The board column is the existing Blocked column. The card is the same approval card, with the extra actions from the table above when the kind needs them. Substatus text stays the existing labels (`Blocked — permission`, `Blocked — secret`, `Blocked — capability`, `Blocked — error`).

### Settings → Access

A new section on the existing Settings page, not a new top-level product. Four groups:

1. **Switches**, each with a sentence of what is true right now.
   - Allow all network access. Off: "Agents can reach their model, this repo's git host, and the package registries below." On: "Network limits are off. Agents can reach the internet."
   - Allow reads and writes outside the ticket folder. On: "Agents can read and write your files. Coppice's saved secrets stay hidden."
   - Allow the home folder. On: "Agents can read your home folder, including SSH keys, cloud credentials, and browser data."
   - Run agents when limits can't be applied. On: "Agents still run when this machine can't limit them."
2. **Package registries.** One switch each, labeled with the registry name and the hosts. Off: "Agents can't download from npm."
3. **Saved rules.** One row: what, who (this agent or all agents), when. Revoke is on the row. The agent page shows the same rows for that agent.
4. **Secrets** and **Recent activity** (the audit log).

The raw TOML editor stays for everything else. These switches patch `[sandbox]` and do not require a restart. A hand-edit applies on the next server start, same as the rest of the file.

### What is enforced

On the agent page, and on the run inside the ticket:

> **What Claude can do on this Mac**
> Files: the ticket folder and Claude's Coppice folder. Shell commands are limited by macOS. Claude's own file tools follow the same list, checked by Claude before they run.
> Network: Anthropic, this repo's git host, and the package registries you left on.
> This is not a separate computer or a virtual machine. The limits stop ordinary mistakes and most attempts to read other files or reach other sites. They are not a guarantee.

Levels use plain words:

| Level | Sentence |
| --- | --- |
| enforced | "Limited on this machine." |
| best-effort | "Coppice asks for these hosts. This machine cannot block every other connection." |
| not enforced | "Not limited on this machine." |

A relaxed switch adds the matching "limits are off" sentence at the top of this card.

Tools → Connectors adds one line under the readiness label: "Limits on this machine: limited." / "Limits on this machine: not limited." The test provider is already hidden on that page and stays hidden.

## Default policy

Applies when `enforce = true` and the wide switches are off. Registries default on.

### Files

| Access | Default |
| --- | --- |
| Read and write | The ticket worktree. Chat and compaction have no write to a repo. The managed home and its `tmp/` |
| Read only | The sign-in files in the table above. OS libraries the CLI needs to start (`/usr`, `/lib`, `/opt/homebrew`, `/System`, `/Library`, `/bin`, `/opt`) |
| Denied | The real home, `~/.ssh`, `~/.aws`, `~/.config/gh`, browser profiles, other registered repos, other agents' homes, Coppice `secrets/`, `config.toml`, `pg/` |

### Network

Allowed hosts are the union of:

1. The adapter's `model_hosts` (not editable; the card lists them).
2. The host from this ticket's repo `remote_url`, port 443 for `https` and port 22 for `ssh` / `git@`. No remote URL means no git host. An SSH session still needs a key grant; the host being allowed is not a credential.
3. Hosts for each registry switch that is on.

Model hosts are pinned in the adapter and logged with the CLI version. Starting set, corrected when a live canary shows a host the CLI actually needs:

| Connector | Model hosts |
| --- | --- |
| `claude-code` | `api.anthropic.com` |
| `codex` | `api.openai.com` |
| `cursor` | `api2.cursor.sh`, `api.cursor.com` |
| `opencode` | Whichever host the configured model provider uses. The OpenCode adapter records it from the provider id at launch. An unknown provider is `not enforced` for that host and the run does not add a wildcard |
| `kilo-code` | Same rule as OpenCode, once the live CLI is checked. Until then, no extra hosts |

Telemetry and crash-report hosts are not in the default list.

### Package registries

Each switch maps to these hosts and no others. Download hosts are included because blocking them makes the registry switch look on while installs fail.

| Switch | Hosts |
| --- | --- |
| npm | `registry.npmjs.org` |
| yarn | `registry.yarnpkg.com` |
| pypi | `pypi.org`, `files.pythonhosted.org`, `pypi.python.org` |
| crates | `crates.io`, `index.crates.io`, `static.crates.io` |
| go | `proxy.golang.org`, `sum.golang.org` |
| rubygems | `rubygems.org`, `index.rubygems.org` |
| maven | `repo.maven.apache.org`, `repo1.maven.org` |

`sum.golang.org` is on because the Go toolchain rejects modules without the checksum database. `files.pythonhosted.org`, `static.crates.io`, and `index.crates.io` are where the bytes actually come from.

### Commands, tools, secrets

Commands are not denied by name in the default profile. `curl` inside the worktree can run; the network list decides where it can go. A saved rule can deny a command prefix. The old profile that denied `curl` and `wget` by default would break installs and is not carried over.

MCP tools: the core set for that context profile, plus plugin tools for plugins enabled on that agent. Chat keeps write tools off.

Secrets: none injected. Coppice's server-side forge token for push and create-PR is unchanged and is not an agent secret.

## Enforcement matrix

"Native" means the CLI's own sandbox is the owner for that column. "Coppice" means the OS backend plus, for network, Coppice's proxy. Levels assume the backend started and the switches are off. If the backend did not start, both columns drop to **not enforced** and the desktop app pauses the run.

| Connector | Filesystem | Network | Launch change | Denial signal | Wrapper |
| --- | --- | --- | --- | --- | --- |
| `claude-code` | **Enforced** for shell commands (Seatbelt or bubblewrap). File tools (Read, Edit, Write, WebFetch) run outside that sandbox. They are **enforced by Claude's permission rules** in the per-run settings, which Claude checks before the tool runs. The card says that in the sentence above | **Enforced** by Claude's proxy when the sandbox starts | Per-run `--settings` file under `run_dir`. Remove `bypassPermissions`. Set `sandbox.enabled`, `sandbox.network.strictAllowlist`, `sandbox.allowUnsandboxedCommands: false`, `sandbox.failIfUnavailable: true`, `denyRead` for the real home and other repos, and matching `permissions.deny` rules so file tools follow the same paths. A repo `.claude/settings.json` must not be able to loosen this; Claude treats a `--settings` false for `allowUnsandboxedCommands` as admin-required | Claude's tool result names the denied host. Adapter parses that event | No. Nesting breaks the native sandbox |
| `codex` | **Enforced** for writes (workspace + managed home) when the permission profile loads. Legacy `--sandbox workspace-write` still allows reading the whole disk; the adapter does not use that legacy mode for the default | **Enforced** when this CLI has permission profiles and `features.network_proxy`. An older CLI that can only open or close all network leaves network **off** (fail closed) and the card says package installs need a newer Codex, or the all-network switch | Remove `--dangerously-bypass-approvals-and-sandbox`. Per-run `-c` / config in the managed home only. Do not set `sandbox_mode` in the same launch as `default_permissions`; Codex ignores the profile if both are set | Codex proxy / sandbox events, parsed by the adapter | No, while the native sandbox started |
| `cursor` | **Enforced** by Coppice's OS backend. Cursor's `--force` stays so the CLI does not sit on a prompt nobody can see. `--mode ask` stays for chat, on top of the file rule | **Enforced** by the Coppice proxy plus the OS network rule. Cursor has no host sandbox | Keep the per-run `HOME` and `cli-config.json`. Stop pointing `XDG_CONFIG_HOME` at the real `~/.config`. Stop forwarding `GH_CONFIG_DIR` | Coppice proxy for hosts. Seatbelt denial log for files. bubblewrap file denials are best-effort to name | Yes |
| `opencode` | **Enforced** by the OS backend around `opencode serve`. OpenCode's own permission file is extra, not the claim | **Enforced** by the Coppice proxy | Existing per-run `OPENCODE_CONFIG`. The serve process is the wrapped child | Coppice proxy | Yes |
| `kilo-code` | Same shape as OpenCode. `--auto` stays so Kilo does not prompt. The live CLI is still unverified; until the adapter's flags are checked against it, the card says **not enforced** and desktop pauses Kilo runs when `allow_unenforced` is false | Same | Existing `KILO_CONFIG` | Coppice proxy | Yes, once the live check lands |
| Plugin stdio | **Enforced** by the OS backend, per agent | Same hosts as that agent, via the Coppice proxy | Pool key includes `agent_id`. Managed home. Not inside the CLI sandbox | Coppice proxy | Yes |
| Plugin HTTP | **Not enforced** for the remote process. The call is refused when the URL host is outside the rules | The connect is **enforced** as a policy check before the request | Existing HTTP transport | Policy error, no sandbox log | No |

Platform claim the card is allowed to make:

| Where | Claim |
| --- | --- |
| macOS Apple Silicon | Seatbelt. Enforced when the profile loads |
| Linux x64 `.deb`, and that `.deb` under WSL2 | bubblewrap. Enforced when `bwrap` starts. WSL adds the Windows-program sentence |
| Docker Compose | Often **not enforced**. Runs still start because `allow_unenforced` is true in the Compose example. The card says not limited |
| Native Windows | **Not enforced.** Out of scope |

## Migration

- Agents and tickets are not rewritten. The next run of an existing agent is strict. In-flight runs finish on the flags they were launched with.
- Old `agent_runs.sandbox_profile_id = 'permissive-default'` stays on those rows as history. New launches write `strict-default` even if some other row still says permissive. The resolver does not treat the old id as "allow everything".
- `permissive.rs` remains for tests and for `sandbox.enforce = false`. It is not a profile the UI offers.
- Config files with no `[sandbox]` table get the strict defaults from the config types. Nobody has to edit TOML to be safe.
- Desktop data dirs are patched in place with the `[sandbox]` block. Comments and other keys stay.
- Existing `secrets` rows gain no grants. Forge tokens keep working for Coppice's push and create-PR. They are not dropped into agent environments.
- Cursor runs that depended on the real `~/.config/gh` or the user's gitconfig lose that on the next run and ask. That is the point of [#30](https://github.com/fertile-org/coppice/issues/30).
- Chat write-denial does not get looser. Kilo chat, which is refused today, stays refused until Kilo's wrapper is verified. Then the file rule enforces it and the refuse can go.

## Delivery

Each pull request merges on its own and leaves the app working. Later ones depend on earlier ones. Automated tests use the existing test provider and `fake-cli`. A real CLI is not required in CI. Desktop and release builds keep omitting the test provider.

Do not run `make test` on every one of these. Use the targeted command. The last pull request runs the full suite.

1. **Policy resolver and tables.** `sandbox/policy.rs`, migration, config types, example TOML. Launch flags unchanged. Test: default policy allows `registry.npmjs.org` and the worktree, denies `~/.ssh` and an arbitrary host, deny wins, toggles relax only their class, the hard floor holds, `policy.rs` contains no connector id.
2. **Audit log for actions that exist today.** Secret create/delete, plugin install/enable, push, create PR. `GET /api/audit-log`. Test: an integration test performs each action and reads the row back; the secret value is absent.
3. **`request_permission` and the ticket card.** Waiting tool, decision API, Allow once / Always allow / Deny, comment on allow, notification. Grants apply in-process for this pull request (no OS sandbox yet). The follow-up-run path from [When a grant takes effect](#when-a-grant-takes-effect) lands with the Claude, Codex, and OS-backend pull requests. Test: server integration with the test provider calling the tool; web test for the three buttons and the This agent / All agents choice.
4. **MCP policy check.** `ToolRegistry::call` consults the resolver. Chat write tools stay denied. A miss opens a permission request. Test: existing chat registry tests plus a denied plugin tool.
5. **Secret grants, injection, redaction.** Test: agent outside the grant does not see the variable; a granted value is scrubbed from a fake CLI stdout line and from a comment; API JSON has no value.
6. **Managed home and env scrub.** All five connectors. Credential mounts. Cursor stops forwarding the real config home and `GH_CONFIG_DIR`. Test: adapter tests assert `HOME` is under artifacts, `AWS_SECRET_ACCESS_KEY` is absent, auth file path is present, user `config.toml` for Codex is not.
7. **Claude Code adapter.** Per-run settings, bypass flag gone, denial parser. Test: golden argv and settings JSON; a fixture stream event becomes a permission request. Enforcement card for Claude only, with the file-tool sentence.
8. **Codex adapter.** Bypass flag gone, permission profile plus network proxy, older-CLI fail-closed path. Test: golden `-c` args; a fixture denial event; a version-too-old case that does not pass the bypass flag.
9. **Coppice OS backend, egress proxy, plugin pool per agent.** Cursor and OpenCode use it. Kilo uses the same code path and stays paused on desktop until its flags are verified against the live CLI (the card stays "not limited"). Plugin stdio is per agent. Desktop pauses when `bwrap` cannot start; the Compose example stays `allow_unenforced`. Test: proxy unit test denies `evil.example` and allows `registry.npmjs.org`; bwrap argv unit test; a live `bwrap` test only when the binary exists, otherwise skipped; pool test that two agents do not share one stdio process.
10. **Settings → Access and the rest of the honest card.** Switches patch TOML and keep comments. Saved rules revoke. Connectors line. Agent card for every connector. Test: web tests for the "limits are off" sentences and for revoke; config patch test.
11. **Blocked actions and smoke.** Add secret, Allow command, Grant capability, Enable tool, Ask the agent why, resume on allow. `make e2e-smoke-m13` drives the card through the test provider: blocked ticket, Allow once, follow-up run succeeds. Then `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test`. After a green full Rust run, `make clean`.

Manual, not CI: on a Mac and on Ubuntu x64, with a real Claude Code and a real Codex, `npm install` in the worktree works, a fetch of an unlisted host raises the card, and `~/.ssh` is not readable. Kilo stays at its verified line until that CLI is checked. Record the CLI versions next to the flags, for [#10](https://github.com/fertile-org/coppice/issues/10).

## Acceptance criteria

- [ ] A new agent, with no grants, cannot read `~/.ssh` or `~/.aws`, cannot see other repos, and cannot open a TCP connection to an arbitrary host. The model host, the repo remote host, and the default registry hosts work.
- [ ] Each registry switch is in Settings. Turning one off removes exactly the hosts in the table above.
- [ ] `request_permission` parks the run until Allow once, Always allow, or Deny. Always allow defaults to This agent and can apply to All agents. The rule shows under Settings → Access and on the agent, and revoke works.
- [ ] A host denied by the proxy or by a Claude/Codex event raises the same card, including when the agent never called the tool.
- [ ] Allow after the process exited resumes through the existing session-resume path.
- [ ] Every MCP tool call is checked against the resolved policy. Chat write-denial is that policy for chat profiles.
- [ ] Secrets are injected only for a matching grant, redacted from logs and comments, and never returned by the API.
- [ ] Plugin stdio servers run per agent under that agent's file and network limits. Remote plugin calls to a host outside the list are refused.
- [ ] The agent page states the enforcement level for that CLI on that machine, and says this is not a virtual machine. A relaxed switch is described in a sentence.
- [ ] Claude Code and Codex launches do not pass `bypassPermissions` or `--dangerously-bypass-approvals-and-sandbox`. Coppice does not wrap those processes in a second OS sandbox while the native one started.
- [ ] Cursor, OpenCode, and verified Kilo runs use the Coppice backend. Desktop does not start them when the backend cannot. Compose with `allow_unenforced` still runs and says not limited.
- [ ] Audit rows exist for grants, revokes, secret changes, toggles, plugin enable, push, and create PR. Values are absent.
- [ ] `make e2e-smoke-m13` passes. Full `make test`, clippy, and `make web-test` pass on the last pull request.

## Open questions

1. **Agent push.** This spec lets an agent fetch its repo remote, and it injects that repo's forge token only after a grant. Push from inside the agent stays off until then. Coppice's own Push and Create PR buttons are unchanged. Is the forge-token grant the right way to let the agent push, or should the agent never push?
2. **All agents** means the whole workspace on this machine. There is no per-repo rule in M13. Say if a repo-sized scope is needed now.
3. **Compose** keeps running when the container cannot sandbox, and the card says not limited. The desktop app pauses instead. Confirm that split.

## References

- [#30 — Fail-closed network and session access](https://github.com/fertile-org/coppice/issues/30)
- [#10 — Pinned run contract and CLI-version canary](https://github.com/fertile-org/coppice/issues/10)
- [Product design §14 and §18](../philosophy/final_agent_workspace_product_design.md)
- [Framework selection §4](../philosophy/final_agent_workspace_framework_selection.md) (process sandbox now; containers and VMs later)
- [Architecture — connectors, MCP, plugin proxy](../architecture.md)
- [Connectors developer guide](../providers/README.md)
- [Claude Code sandbox](https://code.claude.com/docs/en/sandboxing) (Seatbelt, bubblewrap, proxy, `strictAllowlist`, file tools outside the shell sandbox)
- [Codex permissions](https://learn.chatgpt.com/docs/permissions) (profiles, network proxy; do not combine with `sandbox_mode`)
- [M07 — Git/PR and forge secrets](./M07-trust-and-signals.md)
- [M09 — Agent chat](./M09-agent-chat.md)
- [M10 — Plugins](./M10-plugins.md)
- [M11 — Desktop release](./M11-desktop-release.md)
- [M12 — Beta release](./M12-beta-release.md)
- [M14 — Role-owner agents](./M14-role-owner-agents.md)
