# Agent Chat — Provider session resume

**Status:** Accepted design gate  
**Date:** 2026-09-28  
**Milestone context:** [M09 — Agent Chat](../../milestones/M09-agent-chat.md) (shipped); this spec amends chat turn execution and connector chat eligibility.

## Decision summary

Agent Chat (`chat_turn` / `context_profile = conversation`) today starts a **new provider invocation per human message**, rebuilds **full transcript** into `.agent/context.md`, and passes `resume_session_id: None` without persisting provider session ids on chat turns. That is correct for audit durability but wasteful for latency and tokens.

**Change:** Reuse the vendor session across turns within one Coppice `chat_sessions` row for **opencode**, **cursor**, **claude-code**, and **codex**, with **hybrid fallback**: resume + slim context first; on invalid/expired resume, one retry with full Coppice transcript (current behavior), then refresh stored provider session id.

**Also:** Enable Agent Chat for **opencode** and **codex** (remove fail-closed `read_only_tools` refusal), with explicit documentation of weaker enforceability than claude-code/cursor. Prompt + chat rules remain; M07 sandbox is defense-in-depth, not a substitute for connector allowlists.

**Scope:** Agent Chat only. Ticket `human_chat`, `work_on_ticket`, and consultation paths are unchanged except shared provider resume helpers where natural.

**Out of scope:** `kilo-code` chat enablement; long-lived single process per chat; token-budget transcript truncation (separate follow-up).

## Goals

- Multi-turn Agent Chat avoids re-sending full transcript when provider resume succeeds.
- All four named connectors implement the same **resume contract** (capture id, pass resume, tests, docs) — no silent “only works on Claude.”
- Hybrid fallback (C): resume failure → full transcript retry → persist new session id or visible error.
- OpenCode and Codex agents can run `chat_turn` with documented risk.
- Coppice DB transcript remains the audit source of truth.

## Non-goals

- Changing ticket `@mention` Chat (`human_chat`) resume behavior.
- Replacing cutoff → child session semantics (child does not inherit provider session id).
- Guaranteeing Codex CLI session resume reliability (fallback is expected).
- Proving OpenCode API read-only tool enforcement equal to `--mode ask` / `--allowedTools`.

## Current behavior (facts)

| Layer | Behavior |
|-------|----------|
| Coppice session | One `chat_sessions` row; each message enqueues new `agent_runs` (`chat_turn`). |
| Context | `format_transcript` loads **all** messages; `build_conversation_context` writes full transcript to `context.md`. |
| Provider | `execute_chat_turn` sets `resume_session_id: None`, `session_created_tx: None`. |
| Ticket resume | `load_resume_session_id` only for `work_on_ticket` + claude-code/cursor. |
| OpenCode | Always `create_session`; no `resume_session_id` parameter on `run_session`. |
| Chat gates | `opencode` and `codex` return `refuse_unsupported_read_only` when `read_only_tools: true`. |

## Data model

Add to `chat_sessions`:

| Column | Type | Purpose |
|--------|------|---------|
| `provider_session_id` | `TEXT NULL` | Last known vendor session id for this Coppice chat session |
| `provider_session_connector` | `TEXT NULL` | Connector id that created `provider_session_id` (invalidate on mismatch) |

Rules:

- Set/update after **successful** chat turn when provider reports a session id (same mechanism as ticket runs: `session_created_tx` → `agent_runs.session_id` + mirror to `chat_sessions`).
- Clear on: cutoff (parent), connector mismatch with bound agent, resume+fallback failure classified as expired, or successful fallback that started without resume (optional: always overwrite on success).
- **Cutoff child:** `provider_session_id` NULL; first turn uses full transcript only.

Continue storing per-run `agent_runs.session_id` for debugging and artifacts.

## Context profiles

### Full turn (turn 1, fallback, or no stored id)

Unchanged: `build_conversation_context` + full `format_transcript`.

### Resume turn (turn 2+ when resume attempted)

New `build_conversation_resume_context`:

- Session metadata (ids, agent name/role, system prompt).
- Working directory hint when repo/scratch cwd resolved.
- **Only** the triggering human message body (from `run.chat_message_id`), not full transcript.
- Same Coppice chat rules and JSON result contract as conversation profile.

Prompt to CLI remains `coppice_run_prompt()` (“Read `.agent/context.md` …”).

## Turn execution (`execute_chat_turn`)

```text
load chat_session.provider_session_id + provider_session_connector
if stored id present AND matches agent.connector:
  write slim context.md
  run provider with resume_session_id + session_created_tx
  on success → update chat_sessions + finish run
  on resume-invalid error → log chat_resume_fallback=full_transcript → Attempt B
else:
  Attempt B only

Attempt B:
  write full context.md (format_transcript)
  resume_session_id = None
  session_created_tx enabled
  on success → update chat_sessions provider session from new id
  on failure → clear stale provider_session_id, fail turn visibly
```

Structured logging (every chat turn):

- `connector`
- `chat_resume_attempted` (bool)
- `chat_resume_used` (bool)
- `chat_resume_fallback` (bool)

## Connector parity (mandatory)

All four connectors must implement the same orchestration inputs and ship **unit-level** proof plus **docs** rows. None may be omitted because CI uses `mock` only.

| Connector | Resume mechanism | Session id capture | Chat enablement (this spec) |
|-----------|------------------|--------------------|-----------------------------|
| **claude-code** | `--resume <id>` | stream-json session event → `session_created_tx` | Already chat-capable (`CHAT_READ_ONLY_TOOLS`) |
| **cursor** | `--resume <id>` | stream-json → `session_created_tx` | Already chat-capable (`--mode ask`) |
| **codex** | `codex exec resume <id>` | JSONL session id → `session_created_tx` on chat turns | **Remove** `refuse_unsupported_read_only` for conversation; document bypass flag risk |
| **opencode** | Reuse session: `prompt_async` on existing id, same directory | Existing create path + reuse path → `session_created_tx` | **Remove** `refuse_unsupported_read_only`; document no hard read-only allowlist |

### OpenCode implementation notes

- Extend `OpenCodeClient::run_session` (or sibling) to accept `Option<&str> resume_session_id`:
  - `None` → `create_session` (today).
  - `Some(id)` → verify session exists for directory (or handle 404 → resume-invalid for fallback); **do not** create a new session.
- `OpenCodeProvider::run` must pass `input.resume_session_id` and honor `read_only_tools` by **not** refusing — chat rules in context only until a proven API flag exists.

### Codex implementation notes

- Resume argv already exists; wire `session_created_tx` in `execute_chat_turn` like ticket runs.
- Document in [codex.md](../../providers/codex.md): session resume best-effort; Coppice fallback to full transcript is normal.

### Shared orchestration

`execute_chat_turn` enables `session_created_tx` for: `opencode`, `claude-code`, `cursor`, `codex` (same set as ticket worker today).

Extract or generalize resume id loading for chat:

- Prefer reading `chat_sessions.provider_session_id` in `execute_chat_turn` (not only “previous run” query) for clarity.
- Optionally factor `load_chat_resume_session_id(pool, session_id)` for tests.

## Error classification

Providers should map obvious “session not found / invalid resume” failures to a distinguishable `ProviderError` variant or message substring the worker recognizes for fallback. If classification is uncertain, **one** resume attempt then fallback (avoid infinite loops).

Never silent fallback: log `chat_resume_fallback=full_transcript`.

## Chat-capable connector matrix (amends M09)

| Connector | Enforceable read-only? | Agent Chat |
|-----------|------------------------|------------|
| `mock` | Yes (fixtures) | Supported |
| `claude-code` | Yes — narrow `--allowedTools` | Supported |
| `cursor` | Yes — `--mode ask` | Supported |
| `codex` | **No** — bypass flag | **Supported** (prompt + rules; documented risk) |
| `opencode` | **No** proven allowlist | **Supported** (prompt + rules; documented risk) |
| `kilo-code` | No | Unsupported (unchanged) |

Update [2026-09-08-m09-agent-chat-design.md](./2026-09-08-m09-agent-chat-design.md) chat-capable table footnote or add pointer to this spec when implementing.

## Documentation

- [providers/README.md](../../providers/README.md): new section **Agent Chat multi-turn** with parity table for all four connectors.
- Per-provider pages: short subsection on session reuse, fallback, and (for codex/opencode) write-tool risk.
- [development.md](../../development.md) or [testing.md](../../testing.md): how to run multi-turn chat integration tests.

## Testing

| Test | Purpose |
|------|---------|
| `integration_chat` multi-turn (mock) | Second message uses `resume_session_id`; fixture or spy asserts slim vs full context |
| `integration_chat` resume failure (mock) | First resume error → fallback transcript → success |
| Unit: `claude_code`, `cursor`, `codex` argv | Resume flag present when id set |
| Unit: `opencode_client` | Resume path skips `create_session`, calls `prompt_async` on existing id |
| Remove/update `conversation_profile_refuses_write_capable_connectors` | Cursor test stays for **unsupported** connector only, or split: cursor **supports** read_only; add test that codex/opencode **accept** `read_only_tools: true` without `InvalidInput` |
| Connector parity table test (optional) | Static list of four ids required in chat resume path |

E2E: extend `e2e/smoke/m09-chat.mjs` with a second human message when cheap (mock worker).

## Security / trust

- Enabling codex/opencode chat increases blast radius vs claude/cursor allowlists. Mitigations: human-owned sessions, write denial in prompt, future M07 sandbox, visible docs, fail-closed on tool violations where detectable.
- Provider session ids are opaque strings; store only on sessions the user already owns.

## Rollout

1. Migration + domain types for new columns.
2. `build_conversation_resume_context` + worker two-attempt flow.
3. OpenCode client resume + provider wiring.
4. Remove opencode/codex read_only refusal; update matrix and docs.
5. Tests + smoke.

## Success criteria

- Two+ turns in one Agent Chat session on mock CI without full transcript on turn 2 when resume succeeds.
- All four connectors have automated resume wiring tests and README parity row.
- Codex/OpenCode agents can complete `chat_turn` in integration tests without `cannot enforce read-only` errors.
- Logs clearly show resume vs fallback for operator debugging.
