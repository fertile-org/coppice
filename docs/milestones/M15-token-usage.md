# M15 — Token usage

## Goal

Show how many tokens agents spend, per LLM request, per run, per ticket and per agent, so a human can answer "what did this ticket cost to deliver, and which agent spent it?" without reading consoles.

Status: **phase 1 merged** (PR #215). The [connector groundwork](#adding-connectors) and [phase 2](#phase-2-settings--usage) are built for Claude Code (deliverables D1–D6 and D8–D11). D0's manual check passed with a real Claude Code 2.1.318 ticket run. Cursor has a reader too: run totals only, from a real capture, on the same groundwork. Codex (D7) is done: per-turn rows from a real 0.163.0 capture. Still open: the [deferred](#deferred) readers for the remaining connectors (D12+).

## Problem

- No connector parser kept usage. Claude Code, Codex and the ACP connectors all receive token counts on stdout and dropped them (`providers/codex.rs` notes `turn.completed.usage`; `acp_connector.rs` ignores `usage_update`).
- A ticket's work is spread over many runs: `plan_ticket`, `work_on_ticket`, `respond_to_mention`, review, restarts, `continued` runs, `verify_parent`, and the runs of its split children. The Runs tab listed them one by one; nothing summed them or grouped them by agent.
- Connector subagents (Claude Code's Agent tool) were invisible: a run that spawned three subagents looked the same as one that spawned none.
- Saved console events (`console.events.jsonl`) are normalized and carry no usage, so past runs cannot be backfilled. Tracking starts when this ships.

## Decisions

| Topic | Decision |
|---|---|
| Unit of tracking | **One LLM request.** Where a CLI only reports per turn (Codex), the row is a turn and says so (`granularity = 'turn'`). |
| Money | **Tokens only in the UI.** A CLI-reported cost (Claude Code `total_cost_usd`) is stored and not served. No price table. Many users run on subscription plans, where a dollar figure misleads. |
| Subagents | **Count them.** A run lists the subagents its connector spawned, with their tokens, nested under the run. |
| Split tickets | A parent shows two numbers: **this ticket** (its own runs) and **including subtickets** (recursive over `tickets.parent_ticket_id`). Children show only their own. Nothing is counted twice. |
| Knowledge compaction | **Workspace overhead**, not charged to tickets. Compaction and connector-check runs have no `ticket_id`, so no ticket total includes them. Chat turns roll up to their chat session (phase 2). |
| Honesty | Same rule as `reported_model`: only what the CLI reported. Missing is `NULL` and shows "not reported", never 0. A total that is only the sum of what was streamed (`partial`) says so. |
| Tracked runs | A run is tracked when a reader was attached to it, and that is **recorded at run time**: the stream handle stores a `run_usage` row (status `none`) as soon as tracking starts. Runs without a row (connectors without a reader, runs from before M15) are left out of every count. No connector, current or past, is looked up afterwards. |
| Coupling | Usage is keyed by **wire format**, not connector. Storage, API and UI only see normalized events; a connector joins by declaring its format and, if the format is new, adding one reader. |
| Scope | **Claude Code, Cursor and Codex.** Other connectors declare `UsageFormat::None` until a capture and reader exist; their runs show nothing. Their readers are [deferred](#deferred) until their CLIs are available for a capture. |

## What the CLIs report

Verified against Claude Code 2.1.318 on 2026-11-05: `fixtures/claude-code/claude-code-2.1.318-usage-subagent.jsonl` (a Bash call and one subagent) and `...-usage-partial.jsonl` (with `--include-partial-messages`). Resume behaviour checked on 11 releases from 2.0.0 to 2.1.318 on 2026-11-08 (finding 8; `claude-code-2.1.300-usage-resume.jsonl`). Other rows come from the existing parsers and are notes for the follow-up; each needs a captured run before its reader is written.

| Connector | Per request | Run total | Subagents | Verified |
|---|---|---|---|---|
| Claude Code | `stream_event` → `message_delta.usage` (needs `--include-partial-messages`); `assistant.message.usage` without it | `result.usage` (main thread), `result.modelUsage` (main + subagents, per model) | Agent `tool_result` → `tool_use_result.usage`, `system/task_notification.usage` | **yes** |
| Codex | `turn.completed.usage` (per turn: `input_tokens` includes `cached_input_tokens`; also `cache_write_input_tokens`, `output_tokens`, `reasoning_output_tokens`). Captures: `fixtures/codex/codex-0.163.0-usage.jsonl`, `...-usage-resume.jsonl` (0.163.0, 2026-11-19) | sum of turns | — (none in capture) | **yes** |
| Cursor | — | `result.usage` (`inputTokens`, `outputTokens`, `cacheReadTokens`, `cacheWriteTokens`; `reasoningTokens` ignored). Capture: `fixtures/cursor/cursor-agent-2026.11.13-eacade1-usage.jsonl` (2026.11.13-eacade1) | — | **yes** |
| OpenCode (CLI JSON) | `step_finish.part.tokens` (`input`, `output`, `reasoning`, `cache.read` / `cache.write`). Captures: `fixtures/opencode-cli/opencode-2.0.20-usage.jsonl`, `...-usage-resume.jsonl` (2.0.20, 2026-11-22; model `opencode-go/kimi-k2.7-code`) | sum of `step_finish` rows this process emitted | — (subagent tool has no usage; child session id only) | **yes** |
| OpenCode (ACP) | `session/prompt` result `usage` (`inputTokens`, `outputTokens`, `cachedReadTokens`, optional `thoughtTokens`; `totalTokens` = their sum) plus `usage_update` (`used` / `size`). Captures: `fixtures/opencode-acp/opencode-2.0.20-usage.jsonl`, `...-usage-resume.jsonl` (2.0.20, 2026-11-22; model `opencode/big-pickle`). Fixtures are full JSON-RPC stdout; `AcpConsole` observes unwrapped `session/update` payloads and the prompt result | per prompt (billable fields); `used`/`totalTokens` is context fill, not a sum of earlier prompts | child tools carry `_meta.opencode/child-session`; no separate usage | **yes** |
| Other ACP (Copilot, Qwen Code, Kimi, Grok) and Kilo | same ACP shapes expected; not captured here | — | — | no |

### Codex findings

Checked on Codex CLI 0.163.0 on 2026-11-19 (`codex-0.163.0-usage.jsonl`, shell command; `codex-0.163.0-usage-resume.jsonl`, resumed second turn):

1. **`input_tokens` includes `cached_input_tokens`.** Uncached input is `input_tokens - cached_input_tokens` (25711 − 23552 = 2159 on the fresh capture).
2. **`turn.completed.usage` is per turn, including on resume.** The resumed process reported output 48 only; it did not include the earlier turn's 41. The reader sums the turns it sees and needs no resume subtraction.
3. **No model** appears on `turn.completed` or elsewhere in the `--json` stream.
4. **No subagent usage** appeared in the standard capture. `cache_write_input_tokens` and `reasoning_output_tokens` are present (both 0 on these runs); `output_tokens` includes reasoning (session `total_tokens` is input + output).

### OpenCode findings

Checked on OpenCode 2.0.20 on 2026-11-22. Two live transports on the same major version (chosen by model in `opencode.rs`, not by version alone):

**2.x ACP** (`opencode acp`; `fixtures/opencode-acp/opencode-2.0.20-usage.jsonl` shell + subagent, `...-usage-resume.jsonl`):

1. **Per-prompt usage sits on the `session/prompt` result**, not on message parts. Fields: `inputTokens`, `outputTokens`, `cachedReadTokens`, optional `thoughtTokens`, and `totalTokens`. `totalTokens` equals the sum of those four (thought omitted when 0 / absent). Example: 230 + 4 + 8704 = 8938.
2. **`thoughtTokens` are outside `outputTokens`.** When present, add them into Coppice `output`. The older read-only fixture (`opencode-2.0.20-read-only-turn.jsonl`) had thought 17 with output 14; the shell+subagent capture omitted `thoughtTokens` while still streaming `agent_thought_chunk` text.
3. **`cachedReadTokens` are outside `inputTokens`.** Uncached input is `inputTokens` as reported; do not subtract cache from input.
4. **`usage_update` is the context-window meter** (`used` matches that prompt's `totalTokens`, `size` 200000 on these runs). Treat `used`/`totalTokens` as fill, not a run total to sum across prompts.
5. **Resume does not re-count earlier prompts' billable tokens.** A resumed process's prompt result reports that turn only (output 5 vs earlier 4; input 258 vs 230). No resume subtraction.
6. **No model** on the usage payload or `usage_update`.
7. **Subagents appear; they report no usage.** Handshake advertises `_meta.opencode/child-session-updates`. A `subagent` tool (`kind: think`, `agent: general`) streams child shell calls on the parent session with `_meta.opencode/child-session` (`id`, `parentID`, `depth`, `title`). The parent prompt `usage` is the only token block; there is no child usage field to nest.

**2.x CLI JSON** (`opencode run --standalone --format json --auto`; `fixtures/opencode-cli/opencode-2.0.20-usage.jsonl` shell + subagent, `...-usage-resume.jsonl`; model outside the ACP catalog):

1. **Usage is on `step_finish.part.tokens`:** `input`, `output`, `reasoning`, `cache.read`, `cache.write`. Also `part.cost` (store only; do not serve money).
2. **`input` does not include `cache.read`.** Resume showed `input` 275 with `cache.read` 9472, so cache is separate (unlike Codex).
3. **`reasoning` is outside `output`.** Fresh capture: output 62, reasoning 128. Add reasoning into Coppice `output`.
4. **Granularity is per step** (one LLM round that ends in `step_finish`, often `reason: "tool-calls"`). A following text-only step may emit `text` without a trailing `step_finish` before the process exits; a reader that only sums `step_finish` can miss that last step and should mark the run `partial` unless another total appears (none did in these captures).
5. **Resume emits only this process's steps.** Resume `input` 275 / `output` 18 did not include the earlier step's 8601 / 62. Sum what you see; no resume subtraction.
6. **No model** on the JSON lines. **No context-window size** on this transport.
7. **Subagent tool has no tokens.** `tool: "subagent"` completes with a child `sessionID` in metadata; only the parent `step_finish` carries `tokens`.

**1.x HTTP/SSE (`opencode serve`)** is **not tracked.** The serve path has no `UsageHook`. Fresh installs are 2.x. Capturing 1.x needs a pinned 1.x binary beside the 2.0.20 used above; none was available without replacing it. Leave 1.x out until a capture justifies a third reader and a hook on the serve client.

### Claude Code findings

1. **`assistant` events repeat usage once per content block.** A request with a `tool_use` for Bash and one for Agent emitted two `assistant` events with the same `message.id` and the same `usage`. Dedupe by `message.id`.
2. **`assistant.message.usage.output_tokens` is not final.** It is the value from `message_start`: 16 in the stream vs. 77 final; 6 vs. 76. Input and cache fields are exact.
3. **`--include-partial-messages` gives exact per-request usage.** `stream_event` with `event.type == "message_delta"` carries final `output_tokens`, the cache fields and `output_tokens_details.thinking_tokens`; the per-request outputs summed to `result.usage.output_tokens` exactly (77 + 25 = 102). Every text delta becomes a stdout line too, but raw lines are parsed and dropped (`cli_runner.rs`), never stored, so the cost is CPU only. Every consumer of Claude's stdout ignores unknown event types.
4. **Subagent requests are not streamed.** Only the subagent's prompt (`user` event with `parent_tool_use_id`) appears. Its totals arrive in the Agent tool result: `tool_use_result.{agentId, agentType, resolvedModel, totalTokens, totalToolUseCount, totalDurationMs, usage}`. `system/task_started` names it; `system/task_notification.usage.total_tokens` arrives slightly earlier.
5. **`result.usage` is the main thread only; `result.modelUsage` includes subagents** (and Claude Code's own side calls). In the capture, `usage.output_tokens` 286 + subagent 67 = `modelUsage[...].outputTokens` 353. The run total is taken from `model_usage_since_last_result` when present, else from `modelUsage` (see 8).
6. **Thinking tokens** are a subset of output: `output_tokens_details.thinking_tokens`, `modelUsage[].thinkingTokens`.
7. **A process can produce several `result` events** (read-only runs, background tasks). Since 2.1.309 each carries `model_usage_since_last_result`, which covers that process only; the reader sums it.
8. **On `--resume`, `modelUsage` covers the whole session from 2.1.277 on.** A resumed process reports the earlier processes' tokens again (2 one-word turns: 3 output tokens each, `modelUsage` 6 on the second). Every `work_on_ticket` run after an agent's first on a ticket resumes its session, and so does every chat turn. Coppice supports Claude Code from 2.0.0:

   | Claude Code | Resumed `modelUsage` | `model_usage_since_last_result` | Reader |
   |---|---|---|---|
   | 2.0.0 – 2.1.276 | this process | absent | uses `modelUsage` |
   | 2.1.277 – 2.1.308 | whole session | absent | resumed: no CLI total, sum of rows (`partial`) |
   | 2.1.309 + | whole session | present | uses `model_usage_since_last_result` |
9. **One run can start several processes.** Connector checks run one `claude` process per phase on the same run. Each process's total is added to the earlier ones; while a later process has not reported its own, the run is `partial`.

## Architecture

```text
CLI output event
  → adapter: stream.track_usage(descriptor.usage, resumed) per process, then stream.observe_usage(&event)
      → RunStreamHandle: reader.start_process(resumed), reader.read(event) → Vec<UsageEvent>
      → UsageTracker.apply → UsageSnapshot (versioned) → watch channel
  → usage_service::spawn_usage_writer: save each snapshot as it changes
  → usage_service::record_usage at run end: save the last one, then publish `usage.updated` (ticket runs)
  → GET /api/agent-runs/{id}/usage, GET /api/tickets/{id}/usage
```

| Layer | File | Knows about |
|---|---|---|
| Descriptor | `connectors/src/lib.rs`: `UsageFormat`, `ConnectorDescriptor::usage`, `reports_usage()` | which format a connector speaks |
| Readers | `server/src/providers/usage/`: `UsageReader` trait (`start_process`, `read`), `reader_for(format)`, `ClaudeStreamJsonReader`, `NormalizedReader` | one wire format each |
| Domain | `server/src/domain/usage.rs`: `TokenCounts`, `UsageEvent` (`request` / `subagent` / `runTotal` with `byModel` / `runTotalUnknown` / `contextWindow`), `UsageTracker` | normalized usage only |
| Stream | `server/src/sessions/run_registry.rs`: `track_usage`, `observe_usage`, `usage_snapshot`, `subscribe_usage` | a reader and a tracker per run |
| Storage | `server/src/services/usage_service.rs`, migration `069_token_usage.sql` | snapshots, rollups |
| API | `server/src/api/usage.rs` | — |
| Web | `web/src/lib/schemas/usage.ts`, `features/runs/RunTokenUsage.tsx`, `features/runs/tokenUsage.ts`, `features/tickets/TicketUsageSummary.tsx`, `features/tickets/TicketTokens.tsx` | — |

- Events with the same id update one row (fields the new event reports replace the old ones). The CLI's run total wins over the sum of rows; without one, the total is the sum (`partial`). `runTotalUnknown` drops an earlier total that no longer covers the run.
- A partial sum adds each request and each subagent breakdown. Claude Code gives subagents a total but no breakdown (finding 4), so those totals are kept apart as `subagent_tokens` and counted on top.
- `track_usage` publishes a version-0 snapshot (status `none`) right away, so a tracked run that reports nothing is still stored as tracked.
- A stopped run publishes `agent_run.finished` before its worker saves the final snapshot, so the worker announces the save with `usage.updated`; the web app refetches usage on either event, for every ticket (a parent's total includes its subtickets).
- The writer holds only a watch receiver, so it ends when the run's handle is dropped. Every snapshot has a version; `run_usage` refuses a write older than what it holds, so the live writer and the final flush can race safely.
- MockProvider fixtures carry a `usage` array of normalized events and replay it through the same `observe_usage` path (`UsageFormat::Normalized`), so CI covers storage and API end to end. A fixture without `usage` is an untracked run; `"usage": []` is a tracked run that reported nothing.

## Data model

Separate tables; `agent_runs` and its queries are unchanged.

- `run_usage` (one row per tracked run): `status` (`none` | `reported` | `partial`), `version`, input / cache read / cache write / output / reasoning tokens, `subagent_tokens` (partial totals: subagents without a breakdown), `request_count`, `subagent_count`, `reported_cost_usd` (stored, not served).
- `run_llm_requests` (`run_id`, `seq`): `external_id`, `granularity`, `model`, the five token counts.
- `run_subagents` (`run_id`, `seq`): `external_id`, `agent_type`, `description`, `model`, `status`, the five token counts, `total_tokens` (the CLI's own total), `tool_uses`, `duration_ms`.

No `run_usage` row means Coppice could not read the run's usage.

## API

- `GET /api/agent-runs/{id}/usage` → `{ totals: { status, tokens, subagentTokens, requestCount, subagentCount } | null, requests: [...], subagents: [...] }`. `totals` is `null` for an untracked run.
- `GET /api/tickets/{id}/usage` → `{ runs: [{ runId, agentId, jobType, usage }], own, subtickets: { ticketCount, runsWithUsage, partialRuns, tokens, subagentTokens } | null, byAgent: [...], byJobType: [...] }`. `usage` is `null` for an untracked run. Groups (`own`, `byAgent`, `byJobType`) count tracked runs only: `runs`, `runsWithUsage`, `runsNotReported` (finished, nothing reported), `partialRuns` (finished with only a partial total), `tokens`, `subagentTokens`.

## UX

- **Ticket drawer → Detail tab**: a Tokens row with the ticket's total (and "Incl. N subtickets"), a note when some runs ended with partial totals, and "View runs →" to the Runs tab.
- **Runs tab**: a summary card above the runs: "This ticket" total, "Including N subtickets" when children reported usage, runs that reported nothing, runs with partial totals, and tokens by agent (when more than one agent ran). Each run shows `Tokens: 24k in · 182k cached · 9k out` (with `· 200k subagents` and "(partial)" for a partial total), or "Tokens not reported" for a finished tracked run that reported nothing, or nothing for an untracked run.
- **Run details → Tokens tab**: run total, with a note for a partial total (worded for a running and for a finished run), each request (model, tokens), each subagent (type, model, status, description, tokens or the CLI's total, tool uses). Polls while the run is active.
- Numbers are compact (`182k`, `1.2M`) with exact counts in a tooltip. "In" is uncached input plus cache writes; cached reads are shown apart because they dominate the count but are much cheaper against plan limits.

## Out of scope

- Showing money, price tables, currency
- Token budgets, caps, alerts (natural follow-up; pairs with M13)
- Backfill of runs before this milestone
- Plan rate-limit display (Claude Code's `rate_limit_event` carries 5-hour / 7-day utilization)
- Exact token attribution to MCP tool calls

## Delivery

**Done (phase 1):** descriptor `UsageFormat`, reader layer, Claude Code reader with `--include-partial-messages`, tracker, stream handle, writer and final flush on every run path (ticket, plan, chat, compaction, connector check), migration, API, Runs tab and Tokens tab.

The remaining work is split into deliverables. Each one merges on its own and has its own check, so it can be reviewed and verified without the others. Automated checks are targeted `cargo test` runs and `make web-test`. "Manual" means a run with the real connector CLI.

| # | Deliverable | Needs | Verified by |
|---|---|---|---|
| D0 | **Done.** **Close phase 1 on `main`.** The two open items in [Acceptance criteria (phase 1)](#acceptance-criteria-phase-1). | — | Manual: a real Claude Code ticket run shows tokens in the Runs tab. `make test`, clippy and `make web-test` pass on `main`. |
| D1 | **Done.** **Process context.** `start_process(&ProcessContext { resumed, cli_version })`; the Claude Code reader reads `resumed` from it. ([§ 2](#2-a-process-context-instead-of-a-flag)) | — | The existing `providers::usage` tests pass unchanged. A unit test shows the CLI version reaches the reader. |
| D2 | **Done.** **Usage hooks in the shared runners.** `cli_runner.rs` and `acp_connector.rs` call `track_usage` and `observe_usage`; adapters drop their calls. ([§ 1](#1-usage-hooks-live-in-the-shared-runners)) | D1 | No adapter under `server/src/providers/` calls a usage method. A `cli_runner` test replays a Claude Code capture through a stub program and stores the same snapshot as the reader test. A connector with `UsageFormat::None` stores no `run_usage` row. Manual: a Claude Code run shows the same tokens as before. |
| D3 | **Done.** **Reader test harness.** The table-driven test, a `.usage.json` for every Claude Code capture, and the invariants that do not need coverage. ([§ 5](#5-one-test-harness-for-every-reader)) | — | The harness passes on every capture. A case with a wrong expected file and a case with a negative count both fail. |
| D4 | **Done.** **Coverage per format.** `UsageFormat::coverage()`, `usageCoverage` on connector responses, and the harness check that events stay within coverage. ([§ 4](#4-coverage-declared-per-format)) | D3 | Every format has coverage (an exhaustive `match`, so a missing one does not compile). An API integration test reads `usageCoverage`. A harness case that emits a count outside coverage fails. |
| D5 | **Done.** **Run dimensions and usage that outlives runs.** `agent_runs.connector` written when every run starts; `run_usage` gets its own `id`, copies its [dimensions](#schema) when tracking starts, and keeps its rows when the run, ticket, agent or board is deleted; request and subagent rows hang off `run_usage.id`. | — | DB tests: every run type (ticket, plan, chat, compaction, connector check) stores its connector. Deleting a ticket, an agent and a board keeps their usage rows, with links set to NULL and names kept. Phase 1 endpoints return the same results as before for runs that still exist. |
| D6 | **Done.** **Per-model run totals.** `runTotal.byModel`, the `run_usage_models` migration written in the versioned snapshot, and the Claude Code reader emitting it. ([§ 3](#3-a-complete-event-vocabulary)) | D5 | Reader tests on the captures: `byModel` adds up to `tokens`, including a resumed process. DB test: a stale snapshot does not replace newer per-model rows. Integration: a MockProvider fixture with `byModel` stores its rows. |
| D7 | **Done. Codex reader.** Real captures (`codex-0.163.0-usage.jsonl`, resume), `UsageFormat::CodexJson`, coverage, reader and descriptor `usage`. `input_tokens` includes cached reads; usage is per turn (resume does not re-count earlier turns); no model or subagent usage in the stream. | D2, D3, D4 | The harness passes on the Codex captures. The diff touches only the reader, coverage, fixtures, descriptor and [What the CLIs report](#what-the-clis-report). Manual: a real Codex ticket run shows tokens in the Runs tab. |
| D8 | **Done.** **Context-window meter.** The `contextWindow` event, Claude Code's derived value, delivery on the run's WebSocket stream, and the console meter. ([Meter](#live-context-window-meter)) | D2, D4 | A reader test emits `contextWindow`. A stream test delivers it and stores nothing. Web tests: the meter with and without a window size, and hidden when coverage declares none. Manual: the meter grows during a real Claude Code run. |
| D9 | **Done.** **Slices and summary API.** The `usage_slices` view and `GET /api/usage/summary`. ([Slices](#slices)) | D5, D6 | DB tests with MockProvider fixtures at each level of detail (per request, per turn, run total only, subagent total only): each run's slices add up to its total. One test per `groupBy`. Day grouping is checked in two timezones around midnight. Untracked runs appear only in the coverage counts. Workspace runs are excluded by the board filter. Deleting a ticket leaves the totals unchanged. Tokens per Done ticket matches a hand-computed median over a split ticket tree. |
| D10 | **Done.** **Settings → Usage page.** Tiles, chart, grouped table, filters and the coverage line. ([Page](#page)) | D4, D9 | Web tests: for every group, the table rows add up to the Total tile. The coverage line names untracked connectors. "—" and "not reported" appear in the right cells. Filters change the query. Manual: a workspace with runs from two connectors. |
| D11 | **Done.** **Chat-session totals.** `GET /api/chat/sessions/{id}/usage` and the total in the chat header. | — | Integration test with MockProvider chat runs, where untracked runs are not counted. Web test for the header. |
| D12+ | **Cursor done** (run totals from `result.usage`, real capture `cursor-agent-2026.11.13-eacade1`). **OpenCode captures done** (2.0.20 ACP + CLI JSON under `fixtures/opencode-acp/` and `fixtures/opencode-cli/`; see [OpenCode findings](#opencode-findings)). **Deferred. One deliverable per further connector:** OpenCode readers (launch-site format selection), Kilo Code, and one ACP reader covering every ACP connector. Each follows the [checklist](#checklist-for-a-new-connector). | D2, D3, D4, that connector's captures ([deferred](#deferred)) | The same checks as D7, for that connector. |

D0, D1, D3, D5 and D11 can start now and run in parallel. D2 follows D1, D4 follows D3, and D6 follows D5. D8, D9 and D10 need only Claude Code and MockProvider, so none of them waits on the deferred readers.

D12+ is deferred until someone with the OpenCode or other connector CLIs records the [standard capture](#standard-capture). Claude Code, Cursor and Codex runs are tracked; the Usage page's coverage line names every other connector as not tracked. Codex is the per-turn format on the groundwork (D7); Cursor remains run-total-only.

## Adding connectors

Goal: adding a connector to usage tracking touches a captured fixture, a reader (only when its wire format is new) and the descriptor's `usage` field. It needs no adapter, schema, API or UI change.

Phase 1 already keys usage by wire format, but five parts of it were built around one connector. They landed with MockProvider fixtures first; Codex (D7) then proved the groundwork against a real per-turn format: `input_tokens` includes cached reads, `cache_write_input_tokens` is present, and a resumed process does not re-count earlier turns.

### 1. Usage hooks live in the shared runners

Today each adapter calls `stream.track_usage` when a process starts and `stream.observe_usage` on every event (`providers/claude_code.rs`). A new connector would have to repeat both calls and could get them wrong. Move them:

- `cli_runner.rs`: every CLI connector's JSON lines are parsed in one loop before `LineHandler::on_json`. The runner calls `track_usage` before the process starts and `observe_usage` on each parsed line. `CliInvocation` carries the descriptor's `UsageFormat` and the [process context](#2-a-process-context-instead-of-a-flag). `run_cli_interactive` already shares this loop (`run_cli_inner`).
- `acp_connector.rs`: one place for every ACP connector (`session/update` is handled once, in `acp.rs`). It passes `session/update` notifications (including `usage_update`) and prompt responses to `observe_usage`.

A connector with `UsageFormat::None` gets no reader, so both calls do nothing. Adapters no longer call usage methods.

### 2. A process context instead of a flag

`UsageReader::start_process(resumed: bool)` becomes `start_process(&ProcessContext)`, with `resumed` and `cli_version: Option<String>`. Claude Code's totals already depend on its version ([finding 8](#claude-code-findings)), and other CLIs will change too. A new fact becomes a new field on the struct, not a new trait signature.

### 3. A complete event vocabulary

Readers emit only `UsageEvent`s, and the tracker never learns a CLI's quirks. Two additions cover every format in [What the CLIs report](#what-the-clis-report):

| Event | Change | Used by |
|---|---|---|
| `runTotal` | Optional `byModel: [{ model, tokens }]`. `tokens` stays the sum. | Claude Code `modelUsage`, which the reader currently sums and drops; any CLI with per-model totals |
| `contextWindow` | New: `{ used, size? }`. Live only, never stored. | ACP `usage_update`; Claude Code, derived from the last request's input and cache counts |

The normalization contract every reader follows:

- Counts mean what `TokenCounts` says: `input` excludes cache reads and writes, and `output` includes `reasoning`. The reader converts. For example, a CLI whose input includes cached tokens has them subtracted.
- Cumulative counters become per-row values inside the reader.
- A count the CLI does not report is `None`, never 0.
- Rows are keyed by the CLI's id, and a later event with the same id updates the row.

### 4. Coverage declared per format

A static `UsageFormat::coverage()` in `connectors/src/lib.rs` states what a format can report:

```rust
pub struct UsageCoverage {
    pub granularity: CoverageGranularity,    // Request | Turn | RunTotalOnly
    pub model: ModelSource,                  // PerRequest | PerModelTotal | RunOnly | None
    pub fields: TokenFields,                 // which of input, cacheRead, cacheWrite, output, reasoning
    pub subagents: SubagentSource,           // Breakdown | TotalOnly | None
    pub context_window: ContextWindowSource, // WithSize | UsedOnly | None
}
```

The connector API serves it as `usageCoverage`. The UI uses it to tell "this connector never reports this count" (shown as "—") apart from "this run did not report it" ("not reported"), and to list which connectors are tracked.

### 5. One test harness for every reader

A table-driven test runs every capture through `reader_for(format)` and `UsageTracker`. Each capture `fixtures/<connector>/<name>.jsonl` has a `<name>.usage.json` beside it with the expected normalized result: requests, subagents, total and status. The Claude Code captures get expected files too. For every reader the harness also checks:

- No count is negative, and `reasoning` is at most `output`.
- With status `reported`, the rows and subagent totals add up to no more than the run total.
- `byModel` adds up to `tokens`.
- Emitted events match the declared coverage: no count the coverage leaves out, no `request` rows from a turn-only format, no subagents from a format declaring `None`.

These checks catch the meaning mismatches that are easy to miss, such as input that silently includes cached tokens.

### Standard capture

Every connector records the same scenario: one prompt that runs a shell command and, where the CLI supports it, starts one subagent, then a resumed second turn. Name the file `fixtures/<connector>/<connector>-<version>-usage-<scenario>.jsonl` and record the CLI version and capture date in [What the CLIs report](#what-the-clis-report). The recipe goes in [Providers § Token usage](../providers/README.md#token-usage).

### Checklist for a new connector

1. Record the standard scenario.
2. If the wire format is new, add a `UsageFormat` variant with its coverage, write the reader (one file under `server/src/providers/usage/`) and add the expected file. One ACP reader covers every ACP connector.
3. Set `usage` on the connector descriptor.
4. Update the connector's row in [What the CLIs report](#what-the-clis-report).

### Acceptance (connector groundwork)

- [x] No adapter calls `track_usage` or `observe_usage`; the shared runners do.
- [x] `start_process` takes a `ProcessContext` with the CLI version.
- [x] `runTotal.byModel` is stored in `run_usage_models`, and the Claude Code reader emits it.
- [x] Every `UsageFormat` declares coverage, and the connector API serves it.
- [x] The harness runs every capture, including the existing Claude Code ones.
- [x] D7: Codex usage is tracked with changes only to its reader, coverage, fixtures and descriptor.

## Phase 2: Settings → Usage

Goal: answer "where do our tokens go?" across the workspace, by model, agent, board, connector, job type and day, for every tracked connector, with tables that add up to the headline total.

Claude Code's own stats (`~/.claude/stats-cache.json`) keep per-model totals and tokens per day by model. The page follows that shape and adds what only a workspace has: agents, boards, job types and tokens per Done ticket. Streaks, activity heatmaps and hour-of-day counts are left out, because they describe one person's habits, not the work.

### Decisions (phase 2)

| Topic | Decision |
|---|---|
| Connector of a run | **`agent_runs.connector`, written when every run starts.** `session_connector` is written only when the CLI returns a session id, so it cannot serve as the dimension. Runs from before the column show "Unknown connector". The agent's current connector is never looked up. |
| Runs without a board | **A "Workspace" bucket.** Compaction batches, chat sessions and connector checks have no board, so their runs group under "Workspace", and the board filter excludes them. Compaction and connector checks together are **workspace overhead**. |
| Tokens per Done ticket | **Median over top-level tickets, each with its whole tree.** Count top-level tickets (no `parent_ticket_id`) that reached Done in the range. Each counts its whole history, its own runs plus every subticket's, whatever the dates of those runs. A subticket is never counted on its own, so nothing is counted twice. The tooltip shows the average next to the median. |
| Deleted tickets, agents, boards | **Usage outlives them.** `run_usage` copies the dimensions it is grouped by when tracking starts. Deleting a run, ticket, agent or board sets its link to NULL and keeps the tokens, so past totals do not shrink. Labels fall back to the copied name, or "Deleted ticket". A deleted ticket is left out of tokens per Done ticket. Untracked runs are counted from `agent_runs`, so deleting them removes them from the coverage line. |
| Access | **Same as the other Settings tabs.** Ticket and run token counts stay visible to everyone, as in phase 1. |

### Slices

Connectors report at different levels of detail: per request, per turn, or only a run total. If the page read only request rows, a connector that reports only totals would vanish from the model and day tables while still counting in the headline. Instead, the page reads one shape, the **slice**: a part of one run's total with a model, a time and a source. Each tracked run's total splits into:

| Source | Tokens | Model | Time |
|---|---|---|---|
| `request` | Each `run_llm_requests` row (a request or a turn) | The row's | The row's `created_at` |
| `subagent` | Each `run_subagents` row with a breakdown. One with only `total_tokens` is `unsplit`, unless the run's total was reported, which already holds it | The row's | The row's `created_at` |
| `remainder` | A reported total minus the rows above, per count | Per `run_usage_models` when the split covers every row's model and each model's rows fit inside it; else the run's reported model, else "Model not reported" | The run's last snapshot (`run_usage.updated_at`), when its final total arrived |

`unsplit` is a sixth column next to the five counts, so a slice's total still adds up. A remainder below zero (rows above the reported total) is clamped to 0 in the view, and the reader harness fails any capture that would produce one.

The slices of a run always add up to its total, for every connector and every level of detail. A connector that reports only totals shows up as `remainder` slices: coarser, but not missing. A partial run's total is the sum of its rows, so its remainder is 0.

Slices are a SQL view, `usage_slices`, that reads its dimensions from `run_usage` (not `agent_runs`, whose rows can be deleted) and is grouped at query time in the viewer's timezone. Add a daily rollup only if the page is measured to be slow. Storing days in UTC up front would split days in the wrong place.

### Schema

- `agent_runs.connector`: the connector id, written when the run starts.
- `run_usage` gains its own `id` primary key, equal to the run's id when written. `run_id` becomes a nullable, unique link with `ON DELETE SET NULL`. Every snapshot write copies the run's connector, `agent_id` and agent name, `job_type`, `ticket_id`, top-level ticket id, `board_id` and board name, `chat_session_id`, reported model and creation time (`run_created_at`); a write for a run that no longer exists writes nothing. Links to tickets, agents, boards and chat sessions use `ON DELETE SET NULL` (migration `074_usage_dimensions.sql`).
- `run_llm_requests` and `run_subagents` reference `run_usage.id` instead of `agent_runs.id`, so they survive with it.
- `run_usage_models` (`usage_id`, `model`, the five counts): replaced with every snapshot write of `run_usage` (`075_run_usage_models.sql`).
- `tickets.done_at`: set by a trigger when a ticket moves to Done and cleared when it leaves. Tickets already Done take their `updated_at`, an approximation (`076_usage_slices.sql`).
- `usage_slices`: a view, no stored data, with indexes on the request, subagent and usage times it filters by.

### Dimensions

| Dimension | Source |
|---|---|
| Model | The slice's model, grouped together with the connector. The same string from two connectors stays two rows; there is no table mapping model names across connectors. |
| Connector | `run_usage.connector`, copied from `agent_runs.connector` |
| Agent | `run_usage.agent_id`, labelled with the copied name when the agent is deleted |
| Board | `run_usage.board_id`. Runs without one group under **Workspace**. |
| Job type | `run_usage.job_type`. Compaction and connector checks together are **workspace overhead**. |
| Chat session | `run_usage.chat_session_id` |
| Day | The slice's time in the viewer's timezone |

### Page

- **Filters:** range (7, 30 or 90 days, or all time), board and connector.
- **Tiles:** total tokens, tracked runs, tokens per Done ticket (median, with the average in the tooltip) and active days.
- **Chart:** tokens per day, stacked by the selected group.
- **Table:** "Group by" Model, Agent, Board, Connector or Job type. Columns are In, Cached, Out, No breakdown, Total, % of total and Runs. Cached reads stay apart, as on the Runs tab. Rows with no model show as "Model not reported". Workspace overhead is its own row when grouping by job type, and Workspace is its own row when grouping by board.
- **Coverage line:** "Counts 42 of 60 runs in this range. Not tracked: Codex (12 runs), Kilo Code (6 runs)." Tracked runs are usage rows created in the range; untracked runs are runs that started on a connector (`agent_runs.connector` set) and have no `run_usage` row, grouped by connector. Runs that never started are in neither count. A column shows "—" for a row only when no connector in that row can report the count, according to its coverage.
- **Chat sessions:** a chat session's header shows its total. On this page, chat is a job type.

Numbers use the same compact format as the Runs tab, with exact counts in a tooltip. The page is `/settings/usage`, admins only like the other Settings tabs. The chart shows the seven largest groups and folds the rest into "Other"; its colors are `--chart-series-*` tokens, checked for color-vision deficiency in both themes, and the table below it carries the same numbers.

### Live context-window meter

The console header shows "Context 84k / 200k" from the latest `contextWindow` event, or "Context 84k" when the connector reports no window size. The run stream publishes it as a `usage.contextWindow` event and stores nothing. It appears once a reading arrives, so connectors whose coverage declares no context window never show it. Claude Code's reading is a main-thread request's prompt: its input plus cache reads and writes, without its output.

### API

- `GET /api/usage/summary?from&to&tz&groupBy=model|agent|board|connector|jobType&board&connector` (admins only) → `{ totals: { tokens, unsplit, total, runs, activeDays }, rows: [{ key, label, connector, tokens, unsplit, total, runs }], daily: [{ day, byKey }], coverage: { trackedRuns, untrackedRuns, untrackedByConnector }, perDoneTicket: { tickets, median, average } | null }`. Dates are days in `tz` (default `UTC`), both inclusive; a bad date, group or time zone is a 400.
- `GET /api/chat/sessions/{id}/usage` (the session's owner) → the same group shape as the ticket's `own`, read from the usage rows.
- Connector responses gain `usageCoverage`.

### Acceptance (phase 2)

- [x] For every group and range, the table rows add up to the Total tile, including for a connector that reports only run totals (MockProvider fixture).
- [x] Runs from untracked connectors are counted only in the coverage line.
- [x] Workspace overhead is shown apart and never included in a ticket total. The board filter excludes Workspace runs.
- [x] Deleting a ticket, agent or board does not change past totals.
- [x] Tokens per Done ticket is the median over top-level tickets, each counting its whole tree.
- [x] Days follow the viewer's timezone.
- [x] A chat session shows its total.
- [x] The console meter updates during a run and is hidden for connectors without a context window (web and stream tests; the manual check with a real run is part of D0).
- [x] No dollar amounts.

## Open questions

Settled questions are in [Decisions (phase 2)](#decisions-phase-2). Two were settled while building: a remainder below zero is clamped to 0 in the view and fails the harness ([Slices](#slices)), and Claude Code's context reading leaves output out ([Meter](#live-context-window-meter)).

### Deferred

These need connector CLIs that are not available yet, or a follow-up that only adds a reader. They block only D12+.

- **ACP captures beyond OpenCode (blocks a shared ACP reader claiming every ACP connector).** OpenCode 2.0.20 now has the standard capture (shell + subagent + resume) under `fixtures/opencode-acp/`. Prompt-result `totalTokens` is `inputTokens` + `outputTokens` + `cachedReadTokens` + optional `thoughtTokens`; thought is outside output; `usage_update` is the context meter (`used` / `size`). Copilot, Qwen Code, Kimi and Grok still need their own captures before one ACP reader can claim to cover them.
- **OpenCode format routing (blocks turning descriptor `usage` on).** On 2.x, **both** ACP and `opencode run --format json` are live: `opencode.rs` picks ACP when the model is in the ACP catalog, and CLI JSON when it is not (for example `opencode-go/*`). `UsageHook::for_connector` takes one `UsageFormat` from the descriptor, and OpenCode's descriptor stays `None` until readers land. **Choose launch-site format selection:** each path in `opencode.rs` passes an explicit `UsageFormat` into `UsageHook` (`UsageHook::new` or an override on `for_connector`). Do **not** derive the format from `ProcessContext.cli_version` alone — both wire formats exist on the same 2.x binary. A shared ACP reader (once other ACP connectors are captured) covers the ACP path; CLI JSON needs its own format if it stays distinct from Codex/Claude. 1.x serve/SSE stays untracked until a pinned 1.x capture exists (no `UsageHook` on that path today).

## Testing (phase 1)

- Unit: `domain::usage` (merge, totals, partial sums with subagent totals, `runTotalUnknown`, JSON shape); `providers::usage` against the Claude Code captures (exact per-request output, dedupe, subagent, `modelUsage` total, a resumed 2.1.300 process) and edge cases (no stream events, subagent stream events, summed results, several processes, empty `modelUsage`); `run_registry` (version-0 snapshot on `track_usage`, receiver closes on drop); `usage_service` rollup (untracked runs left out, active runs neither "not reported" nor "partial").
- DB: stale snapshots never replace newer ones, and version 0 is written only once; ticket usage counts subtickets recursively and apart, with partial runs and subagent totals.
- Integration (`integration_agent_runs.rs`): a mock run with a `usage` fixture stores requests, a merged subagent and the reported total, and both endpoints serve them (cost not served, 404 for unknown ids); an untracked run serves nulls and is not counted; a tracked run with `"usage": []` is "none"; counts follow stored rows whatever the agent's connector.
- Web: formatting, Tokens tab, ticket summary, Detail-tab Tokens row, Runs tab line for untracked / not reported / partial.

## Acceptance criteria (phase 1)

- [x] A Claude Code run records one row per LLM request with exact output tokens, plus one row per subagent, and its total equals the CLI's own total for that process, summed over models.
- [x] Runs from connectors without a reader, and runs from before M15, record nothing, show nothing and are not counted; a tracked run that reported nothing says "Tokens not reported", never 0.
- [x] A resumed run never counts an earlier run's tokens again, on any supported Claude Code version.
- [x] A total that is only the sum of what was streamed is labelled partial and includes subagents' own totals.
- [x] Runs tab shows tokens per run and expands to requests and subagents.
- [x] Token counts update while a run is active (polling).
- [x] Ticket summary shows totals by agent; a split parent shows its own and including-subtickets totals, refreshed when any run finishes.
- [x] A ticket's Detail tab shows its token total.
- [x] Compaction and connector-check runs are not counted in any ticket total.
- [x] No dollar amounts in the UI or API.
- [x] Manual: a real Claude Code ticket run shows tokens in the Runs tab. Claude Code 2.1.318 on Haiku, with one subagent, in desktop mode: the total moved from partial to reported as the run finished, with 9 requests and the subagent's own total, and the Runs tab, Detail tab and Settings → Usage showed the same 346k.
- [x] `make test`, clippy, `make web-test` pass on the branch.

## References

- `server/src/providers/reported_model.rs`, `server/src/sessions/run_registry.rs` (`note_reported_model`): the pattern this follows
- `server/src/providers/cli_runner.rs`: raw lines are parsed, not stored
- [Providers § Token usage](../providers/README.md#token-usage)
