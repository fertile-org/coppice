# M10 — Plugins (tools & skills)

## Goal

Give every agent a **tool-first harness**. Coppice becomes the single MCP gateway for every run: built-in tools expose Coppice itself (tickets, comments, agents, knowledge, results), skills load on demand, and plugin MCP tools are proxied — all through one audited endpoint.

Today each run gets a long `context.md` that pre-loads ticket data and spells out the result JSON contract and collaboration rules. After M10, agents **pull** what they need through tools and **submit** results through a validated `result_submit` tool, so the context shrinks to identity, task, and a skill index.

## Why before Security and Role-owner agents

- M12 (Security & sandbox) enforces policy at the gateway — one choke point instead of per-connector rules.
- M13 (Role-owner agents) raises signals through a tool, not another bespoke result-contract field.

## Product scope

- **Plugin format:** existing Claude Code / Cursor plugin format (`.claude-plugin/plugin.json`, `skills/`, `.mcp.json`) plus skills-only folders; third-party plugins work unchanged
- **Plugin sources:** multiple admin-chosen plugin directories (desktop Browse) + install from git URL into a plugin dir
- **Enablement:** installed plugins are workspace-wide and start disabled; each agent selects whole plugins; presets ship defaults
- **Gateway:** `/mcp` streamable HTTP endpoint with per-run tokens; each connector is configured with exactly one MCP server, `coppice`
- **Core tools:** `ticket_get`, `ticket_comments`, `ticket_runs`, `ticket_search`, `board_agents`, `knowledge_search`, `skill_list`, `skill_load`, `comment_post`, `result_submit` — scoped per context profile
- **Built-in `coppice` plugin:** platform skills (collaboration, splitting, PM / Tech Lead / QC role rules) replacing prose in `context_builder.rs`
- **Plugin MCP proxy:** stdio and remote HTTP servers, namespaced tools, health/restart, encrypted per-plugin settings
- **One tool-first path** for all six connectors; final-JSON parsing kept only as a safety net; legacy fat context removed
- **Observability:** tool calls, Skills Used, Knowledge Used in Agent Run detail; tool calls inline in live console

## Out of scope

- Coppice-owned agent loop (direct LLM API calls)
- Plugin storage; plugin `commands/`, `agents/`, `hooks/` (listed as unsupported)
- Marketplaces, signing
- Personal access tokens / external agents using Coppice MCP (token layer designed to allow it later)
- Sandboxing plugin processes, per-tool grants, secret scoping (M12)
- Signals / observation tools (M13)

## Dependencies

- M05 result contract semantics, M06 knowledge + inbox, M07 encrypted secrets, M08 managed connectors, M09 profiles + write-denial

## Design

[Design spec](../superpowers/specs/2026-09-29-m10-plugins-design.md) — architecture, tool matrix, token model, connector wiring, data model, API, testing, delivery order.

## Acceptance criteria

- [ ] Design spec + implementation plan committed; connector verification recorded
- [x] Plugin dirs, scan, git install, enable, agent assignment work via UI and API
- [x] Claude Code / Cursor format plugins and skills-only folders load unchanged; unsupported parts listed
- [x] `/mcp` gateway with per-run tokens; core tools and `result_submit` per profile matrix
- [x] Plugin MCP servers proxied with namespacing, health, restart, settings
- [ ] All six connectors run tool-first; legacy fat context removed
- [x] `full` context ≥50% smaller than baseline on fixture tickets
- [x] Knowledge Used, Skills Used, and tool calls visible in Agent Run detail
- [ ] M05 / M06 / M09 behavior unchanged; existing smokes pass
- [x] CI mock path green; `make e2e-smoke-m10` passes

Open (2026-10-03): live verification of the `claude-code`, `codex`, and `kilo-code` gateway wiring needs their CLIs (only `cursor` and `opencode` are verified). Record it from **Tools → Connectors → Test connection** ([connector diagnostics](../superpowers/specs/2026-10-03-connector-diagnostics-design.md), [how to verify](../providers/README.md#diagnostics-tools--connectors)) and update the status table in the providers README. Of the existing smokes, `make e2e-smoke-m05` fails on a reused database (handoff picks an older same-key agent); the others pass.

## References

- [M12 — Security & sandbox](./M12-security-and-sandbox.md)
- [Model Context Protocol](https://modelcontextprotocol.io)
