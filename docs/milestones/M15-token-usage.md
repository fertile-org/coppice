# M15 — Token usage

## Goal

Show how many tokens agents spend, per LLM request, per run, per ticket and per agent, so a human can answer "what did this ticket cost to deliver, and which agent spent it?" without reading consoles.

Status: **phase 1 implemented for Claude Code** (storage, API, Runs tab). The other connectors are a follow-up; phase 2 (Settings → Usage, chat-session rollup) is open. Sequencing against M12–M14 is not decided.

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
| Knowledge compaction | **Board overhead**, not charged to tickets. Compaction and connector-check runs have no `ticket_id`, so no ticket total includes them. Chat turns roll up to their chat session (phase 2). |
| Honesty | Same rule as `reported_model`: only what the CLI reported. Missing is `NULL` and shows "not reported", never 0. A total that is only the sum of what was streamed (`partial`) says so. |
| Tracked runs | A run is tracked when a reader was attached to it, and that is **recorded at run time**: the stream handle stores a `run_usage` row (status `none`) as soon as tracking starts. Runs without a row (connectors without a reader, runs from before M15) are left out of every count. No connector, current or past, is looked up afterwards. |
| Coupling | Usage is keyed by **wire format**, not connector. Storage, API and UI only see normalized events; a connector joins by declaring its format and, if the format is new, adding one reader. |
| Scope | **Claude Code first.** Other connectors declare `UsageFormat::None` until a capture and reader exist; their runs show nothing. |

## What the CLIs report

Verified against Claude Code 2.1.318 on 2026-11-05: `fixtures/claude-code/claude-code-2.1.318-usage-subagent.jsonl` (a Bash call and one subagent) and `...-usage-partial.jsonl` (with `--include-partial-messages`). Resume behaviour checked on 11 releases from 2.0.0 to 2.1.318 on 2026-11-08 (finding 8; `claude-code-2.1.300-usage-resume.jsonl`). Other rows come from the existing parsers and are notes for the follow-up; each needs a captured run before its reader is written.

| Connector | Per request | Run total | Subagents | Verified |
|---|---|---|---|---|
| Claude Code | `stream_event` → `message_delta.usage` (needs `--include-partial-messages`); `assistant.message.usage` without it | `result.usage` (main thread), `result.modelUsage` (main + subagents, per model) | Agent `tool_result` → `tool_use_result.usage`, `system/task_notification.usage` | **yes** |
| Codex | — (per turn: `turn.completed.usage`: input, cached input, output) | sum of turns | unknown | no |
| Cursor | unknown | `result` event, fields unknown | unknown | no |
| OpenCode (CLI) | `step_finish` part, likely `tokens` (input, output, reasoning, cache read/write) | sum of steps | unknown | no |
| ACP connectors (OpenCode, Copilot, Qwen Code, Kimi, Grok) and Kilo | ACP `usage_update` (`used` / `size`, context window); session messages likely carry `tokens` | prompt response or sum | unknown | no |

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
| Domain | `server/src/domain/usage.rs`: `TokenCounts`, `UsageEvent` (`request` / `subagent` / `runTotal` / `runTotalUnknown`), `UsageTracker` | normalized usage only |
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

1. **Done:** descriptor `UsageFormat`, reader layer, Claude Code reader with `--include-partial-messages`, tracker, stream handle, writer and final flush on every run path (ticket, plan, chat, compaction, connector check), migration, API, Runs tab and Tokens tab.
2. **Follow-up (other connectors):** for each of Codex, Cursor, OpenCode, Kilo Code and the ACP connectors, capture a small real run (a shell call and a subagent where supported) into `fixtures/<connector>/`, add a `UsageFormat` variant and reader (one ACP reader covers every ACP connector), set the descriptor's `usage`, and add the two calls in the adapter. No schema, API or UI change.
3. **Phase 2:** Settings → Usage (by agent, model, board, day; board overhead from compaction and connector checks; tokens per Done ticket), chat-session rollup, a live context-window meter in the console.

## Testing

- Unit: `domain::usage` (merge, totals, partial sums with subagent totals, `runTotalUnknown`, JSON shape); `providers::usage` against the Claude Code captures (exact per-request output, dedupe, subagent, `modelUsage` total, a resumed 2.1.300 process) and edge cases (no stream events, subagent stream events, summed results, several processes, empty `modelUsage`); `run_registry` (version-0 snapshot on `track_usage`, receiver closes on drop); `usage_service` rollup (untracked runs left out, active runs neither "not reported" nor "partial").
- DB: stale snapshots never replace newer ones, and version 0 is written only once; ticket usage counts subtickets recursively and apart, with partial runs and subagent totals.
- Integration (`integration_agent_runs.rs`): a mock run with a `usage` fixture stores requests, a merged subagent and the reported total, and both endpoints serve them (cost not served, 404 for unknown ids); an untracked run serves nulls and is not counted; a tracked run with `"usage": []` is "none"; counts follow stored rows whatever the agent's connector.
- Web: formatting, Tokens tab, ticket summary, Detail-tab Tokens row, Runs tab line for untracked / not reported / partial.

## Acceptance criteria

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
- [ ] Manual: a real Claude Code ticket run shows tokens in the Runs tab.
- [ ] `make test`, clippy, `make web-test` pass on the branch.

## References

- `server/src/providers/reported_model.rs`, `server/src/sessions/run_registry.rs` (`note_reported_model`): the pattern this follows
- `server/src/providers/cli_runner.rs`: raw lines are parsed, not stored
- [Providers § Token usage](../providers/README.md#token-usage)
