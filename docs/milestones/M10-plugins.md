# M10 — Plugins (tools, skills, commands, storage)

## Goal

Give every agent a **tool-first harness**: a plugin system that bundles MCP tools, skills, commands, and scoped storage, with a built-in `coppice-core` plugin that exposes Coppice itself (tickets, comments, knowledge, results) as tools.

Today each run gets a long `context.md` that pre-loads ticket data and spells out the result JSON contract, collaboration rules, and on-demand file paths (`.agent/ticket.json`, `.agent/comments.json`, `.agent/runs.json`). After M10, agents **pull** what they need through tools and **submit** results through a validated tool call, so the context file shrinks to identity, the task, and a short tool index.

## Why before Security and Role-owner agents

- M11 (Security & sandbox) needs a single choke point to enforce policy on. Tool calls through Coppice's MCP endpoint are that choke point; capabilities become grants over tools, commands, paths, and secrets.
- M12 (Role-owner agents) needs agents to query domain state and raise signals. That is a plugin tool (`signal_create`), not another bespoke result-contract field.

## Product scope

### Plugin model

A plugin is a directory with a manifest:

```text
my-plugin/
  coppice-plugin.toml     # name, version, description, contents
  skills/<name>/SKILL.md  # instructions + optional assets, loaded on demand
  commands/<name>.md      # prompt templates invoked by humans (/name)
  mcp.toml                # external MCP servers (stdio command or http url)
```

| Part | What it is | Who uses it |
|------|------------|-------------|
| **Tools** | MCP tools, from `coppice-core` or external MCP servers declared by the plugin | Agents during runs |
| **Skills** | Named instruction bundles with a one-line description; full body loaded only when the agent asks | Agents (progressive disclosure) |
| **Commands** | Prompt templates with arguments, expanded server-side | Humans in ticket comments and Chat composer (`/review`, `/split`, …) |
| **Storage** | Plugin-scoped key/value + small blob store, scoped to workspace / board / agent / ticket | Tools of that plugin; visible read-only in the UI |

- Install from a **local path** only (managed volume, e.g. `/home/coppice/plugins/…`); no server-side fetch in v1
- Admin enables plugins workspace-wide, then per agent (`agent → plugins`); agent presets ship default plugin sets
- Plugins are versioned; upgrading keeps storage; disabling hides tools/skills/commands without deleting storage
- Existing free-form `agent.skills` tags remain labels; plugin skills are a separate concept (design gate decides naming to avoid confusion)

### Built-in `coppice-core` plugin

Always installed, enabled per profile. Tools (names locked in design gate):

| Tool | Replaces today |
|------|----------------|
| `ticket_get`, `ticket_comments`, `ticket_runs` | Pre-loaded snapshot + `.agent/*.json` files |
| `board_agents` | Agent roster text in context (who can be assigned / consulted) |
| `knowledge_search` | Pre-injected knowledge section (retrieval becomes on demand, still only approved knowledge) |
| `knowledge_propose` | Knowledge candidates in result; always lands **Pending** in the Knowledge Inbox |
| `comment_post` | Progress notes (the final summary still comes from the result) |
| `result_submit` | "Return a single JSON object" contract text; schema lives in the tool definition and the server returns validation errors so the agent can fix and resubmit |
| `skill_list`, `skill_load` | Connector-agnostic skill access for connectors without native skills support |
| `storage_get`, `storage_put`, `storage_list`, `storage_delete` | — (new) |

Rules:

- Server owns state: tools call the same services as the API (`ticket_service`, `comment_service`, `knowledge_service`, `result_contract`). No new workflow rules in tool handlers.
- Tool sets are **profile-scoped**: `conversation` / `human_chat` get read tools + their existing actions only; `full` gets the full set. M09 write-denial stays in force.
- `result_submit` is idempotent per run; the last valid submission wins. Final JSON on stdout remains a **fallback** for connectors without MCP.
- Storage is working memory, not knowledge. It is never injected into context automatically and cannot bypass Knowledge Inbox approval.

### Coppice MCP endpoint

- Streamable HTTP MCP endpoint on the server (e.g. `/mcp`), reachable from connector processes inside the server container
- Per-run **scoped token**: bound to run id, agent, ticket, context profile, enabled plugins; expires when the run ends
- `coppice mcp-bridge` (in the existing CLI binary, already on PATH) proxies stdio ↔ HTTP for connectors that only speak stdio MCP
- External plugin MCP servers are proxied through Coppice so every call is authorized, logged, and (in M11) policy-checked in one place

### Connector integration

Each connector gets a per-run MCP config and skills location generated into the worktree / run dir. Expected mechanisms (verify each in the design gate):

| Connector | MCP config | Native skills |
|-----------|------------|---------------|
| `claude-code` | `--mcp-config` file | project skills dir |
| `cursor` | `.cursor/mcp.json` in cwd | project rules/skills dir |
| `opencode` | `opencode.json` `mcp` block | via `skill_load` tool |
| `codex` | `mcp_servers` config override | via `skill_load` tool |
| `kilo-code` | design gate | via `skill_load` tool |
| `mock` | in-process MCP client driven by fixtures | fixtures |

Connectors without working MCP keep today's full context + stdout JSON contract, so nothing regresses.

### Context slimming

- New "tool-first" variants of the `full`, `human_agent`, and consultation contexts: identity, task, ticket title/status/substatus, human request, tool index, one-line skill index
- Drop from tool-first contexts: embedded result JSON examples, `.agent/*.json` instructions, pre-injected knowledge, long collaboration rule blocks (moved into tool descriptions and `coppice-core` skills)
- `context_budget` reports both variants so the reduction is measurable per run

### Commands

- `/command args` in ticket comments and Chat composer expands the template server-side into the human request
- Autocomplete lists commands from plugins enabled for the target agent
- Commands can also be exposed as MCP prompts where connectors support them

### Observability

- Every tool call is recorded (`run_tool_calls`: run, tool, args summary, result status, duration, error)
- Agent Run detail gets a **Tool calls** tab; live console shows tool calls inline
- Settings → Plugins: list, enable/disable, version, contents (tools, skills, commands), storage usage

## Out of scope

- Sandboxing external MCP server processes and per-tool capability grants (M11). In M10, only admins can install plugins and external MCP servers are **off by default**.
- Secret injection policy for plugin MCP servers beyond an explicit admin binding (M11 generalizes)
- Plugin marketplace, remote install, signing
- Signals / observation tools (M12)
- Agent↔agent tools that bypass ticket comments

## Dependencies

- M03–M04: run pipeline, live stream, artifacts
- M05: result contract, mentions, consultations (`result_submit` must keep those semantics)
- M06: knowledge service + inbox (`knowledge_search`, `knowledge_propose`)
- M08: managed `$HOME` for connector config/skills locations
- M09: `conversation` profile and write-denial

## Architecture notes

### Design gate (required before coding)

1. Spec under `docs/superpowers/specs/` — manifest schema, tool list + JSON schemas, profile → tool matrix, token model, per-connector wiring, tool-first context layout, fallback rules, storage limits
2. Plan under `docs/superpowers/plans/` — sequenced tasks + test matrix
3. Baseline measurement of current context sizes per profile on fixture tickets

### Suggested server modules

```text
server/src/
  plugins/
    manifest.rs
    registry.rs          # installed plugins, per-agent enablement
    skills.rs
    commands.rs
    storage.rs
  mcp/
    server.rs            # streamable HTTP endpoint
    token.rs             # per-run scoped tokens
    proxy.rs             # external plugin MCP servers
    core_tools/          # coppice-core tool handlers → services
  api/plugins.rs
cli/src/commands/
  plugin.rs              # coppice plugin list|install|enable|disable|doctor
  mcp_bridge.rs
```

### Suggested tables

```text
plugins
agent_plugins
plugin_storage_items
run_tool_calls
run_tool_tokens
```

### API sketch (lock in design gate)

```text
GET    /api/plugins
POST   /api/plugins                  # install from local path
PATCH  /api/plugins/:id              # enable / disable / upgrade
GET    /api/plugins/:id/storage
GET    /api/agents/:id/plugins
PUT    /api/agents/:id/plugins
GET    /api/commands?agentId=
GET    /api/agent-runs/:id/tool-calls
POST   /mcp                          # MCP streamable HTTP (run token auth)
```

## Testing strategy

### Unit

- Manifest parsing and validation
- Profile → tool matrix (chat profiles cannot see write tools)
- Run token scoping: wrong ticket / expired / other agent rejected
- `result_submit` validation errors and idempotency
- Command template expansion
- Storage scope isolation between plugins and scopes

### Integration

- Mock run calls `ticket_get` → `knowledge_search` → `result_submit` → ticket updated exactly as the stdout JSON path would
- `knowledge_propose` creates Pending inbox item only
- Tool-first context is smaller than the legacy context for the same fixture ticket
- Connector without MCP still uses the legacy context + stdout JSON
- Tool calls persisted and returned by the run API

### Smoke

- `make e2e-smoke-m10` — enable a fixture plugin on an agent, run a mock ticket that uses tools and a skill, see tool calls in Run detail, use a `/command` in a comment
- Default Compose stays mock-only

## Acceptance criteria

- [ ] Design spec + plan committed before implementation, with baseline context measurements
- [ ] Plugin manifest, install from local path, workspace + per-agent enablement (UI, API, CLI)
- [ ] Coppice MCP endpoint with per-run scoped tokens; `coppice mcp-bridge` for stdio connectors
- [ ] `coppice-core` tools for tickets, comments, agents, knowledge, storage, skills, and `result_submit`
- [ ] Tool access is profile-scoped; M09 write-denial and M06 inbox approval unchanged
- [ ] Skills load on demand (natively or via `skill_load`); commands expand from comments and Chat
- [ ] Plugin storage scoped and visible in the UI
- [ ] At least `mock` plus two real connectors run tool-first; the rest fall back to the legacy contract without regression
- [ ] Tool-first full-profile context measurably smaller than baseline (target ≥50% on fixture tickets)
- [ ] Tool calls recorded and shown in Agent Run detail
- [ ] CI mock path green; `make e2e-smoke-m10` passes

## References

- [M05 — Workflow & collaboration](./M05-workflow-and-collaboration.md) — result contract semantics
- [M06 — Knowledge & learning](./M06-knowledge-and-learning.md)
- [M09 — Agent Chat](./M09-agent-chat.md) — profiles and write-denial
- [M11 — Security & sandbox](./M11-security-and-sandbox.md)
- [Model Context Protocol](https://modelcontextprotocol.io)
