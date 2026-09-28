# Chat UI — Unified transcript streaming & session list

**Status:** Accepted design gate  
**Date:** 2026-09-28  
**Related:** [M09 Agent Chat](../../milestones/M09-agent-chat.md), [web DESIGN](../../web/DESIGN.md)

## Decision summary

Improve Agent Chat perceived quality without changing product identity (warm Coppice workshop, not consumer “AI app” chrome):

1. **Unified streaming (A):** While a `chat_turn` runs, show **one growing agent bubble inside the virtualized transcript** — same styling as persisted agent messages. Retire the separate `ChatLiveTurn` block below the message list. Tool/console detail stays minimal in v1.
2. **Richer session list (B):** Sidebar shows **last-message preview**, **active-run indicator**, and **client-side search** over agent name + preview. Extend `GET /api/chat/sessions` with preview/active fields; no server-side search in v1.

## Goals

- Transcript reads as a single conversation thread (no duplicate live stack under history).
- Session list helps users find conversations at a glance.
- Visual language stays consistent with `ChatMessageBubble` and `docs/web/DESIGN.md`.
- Reuse existing run live WebSocket (`/ws/agent-runs/:runId/live`); no second chat token protocol.

## Non-goals

- Server-side session search (`q=` query param).
- Unread badges / cross-tab notification semantics.
- Rich tool-step UI inside chat bubbles (ticket Live Session remains the deep view).
- Chat focus mode / full-bleed layout changes.
- Changing chat turn execution, provider resume, or action contracts.

## Current behavior (facts)

| Area | Today |
|------|--------|
| Transcript | `ChatMessageList` virtualizes persisted messages. |
| Live turn | `ChatLiveTurn` renders **below** the list with OpenCode/console components — different look from moss bubbles. |
| Thinking | `thinking` prop shows `ThinkingIndicator` under list when awaiting but no `activeRunId`. |
| Session API | `ChatSession` has id, agent, status, timestamps — no preview or active run. |
| Session UI | Agent name + `updatedAt` + status string; no search. |

## Design — Session list

### API

Extend each session in `GET /api/chat/sessions` (and single-session `GET` for consistency):

| Field | Type | Semantics |
|-------|------|-----------|
| `lastMessagePreview` | `string` | Latest message by `seq`: strip newlines, trim, truncate to **120** chars with ellipsis; `""` if no messages |
| `lastMessageAt` | `string` (RFC3339) | `created_at` of that message; omit or null if no messages — client may fall back to `updatedAt` |
| `lastMessageRole` | `human` \| `agent` \| `system` | Optional; enables “You:” / “Agent:” prefix in sidebar |
| `hasActiveRun` | `boolean` | `true` if any `agent_runs` for this `chat_session_id` with `status` ∈ (`queued`, `running`) |

**Implementation notes (server):**

- Compute in `ChatService::list_sessions` (and `get_session`) via subquery or lateral join — avoid N+1 per session in one list query where possible.
- `hasActiveRun` uses existing active-chat index on `(chat_session_id, agent_id)` scope; any active run for the session suffices.
- Owner scoping unchanged.

**Zod:** Extend `chatSessionSchema` in `web/src/lib/schemas/chat.ts`; tolerate optional fields during rollout if needed, then require in tests.

### Client sidebar (`ChatPage`)

- Add search input above session list (`aria-label` “Search chats”).
- Filter: case-insensitive match on **agent display name** OR `lastMessagePreview`.
- Row layout:
  - Primary: agent name
  - Secondary: preview line (muted, truncated one line CSS)
  - Tertiary: time (`lastMessageAt` or `updatedAt`)
- **Active run:** moss dot or “Replying…” label when `hasActiveRun`; when user is on another route, **poll session list every 3s** while any loaded session has `hasActiveRun` (or while current session awaiting — keep simple: poll when `hasActiveRun` on open session or global flag from list).
- Empty filter: “No matching chats.”

Invalidate `CHAT_SESSIONS_QUERY_KEY` when posting a message, on turn finish, and on cutoff/archive actions.

## Design — Unified streaming

### UX

1. User sends message → human row appears (POST response + query invalidation).
2. Set `activeRunId` from `runId`; initialize `streamingTurn = { runId, text: '' }`.
3. Virtual list renders **all persisted messages** plus **one synthetic trailing row** when `streamingTurn` is set:
   - Same alignment and moss bubble classes as agent `ChatMessageBubble`.
   - `aria-busy="true"` while streaming.
   - Body: accumulated text rendered with `MarkdownContent` when non-empty; else compact “Thinking…” inside bubble (not a second row below the list).
4. On WS end / `onFinished`: clear `streamingTurn` and `activeRunId`, refetch messages, scroll to end once.
5. Remove mounting `ChatLiveTurn` below `ChatMessageList` in `ChatSessionPane`.

### Data flow

```
ChatComposer onPosted(runId)
  → activeRunId + streamingTurn
  → useChatRunLiveStream(runId)  // extract from ChatLiveTurn
  → append text from OpenCode deltas + console text/result.summary
  → on close: refetch messages, invalidate sessions, clear state
```

**WS parsing:** Reuse logic from `ChatLiveTurn.tsx` (OpenCode events, `*.console.*` for Claude/Cursor/Codex/Kilo). v1 **does not** render tool cards in the bubble — only text accumulation. Ignore tool-only events unless no text yet (optional single “Working…” — default ignore per YAGNI).

### Component boundaries

| Unit | Responsibility |
|------|----------------|
| `useChatRunLiveStream(runId)` | WS connect, reconnect policy, expose `text`, `connectionState`, `error` |
| `ChatMessageList` | New optional prop `streamingAgent?: { text: string; runId: string }` — renders extra virtual row or fixed footer row inside scroll container |
| `ChatMessageBubble` | Optional `streaming?: boolean` for pulse border / busy semantics |
| `ChatSessionPane` | Orchestrate; delete sibling `ChatLiveTurn` |

**Virtualizer:** Append `+1` to count when streaming row present; stable key `streaming-{runId}`; `scrollToIndex` on text growth throttled (e.g. requestAnimationFrame).

### Edge cases

| Case | Behavior |
|------|----------|
| User leaves session mid-run | WS disconnect; server continues; on return, `hasActiveRun` true until complete; refetch messages — if run still active, reattach WS using latest `activeRunId` from GET run or poll messages for new agent row |
| Cutoff / archived | No composer; clear streaming state |
| WS error | Inline error under bubble; poll messages every 1.5s until agent message or run terminal |
| Duplicate agent row | Clear streaming before refetch; dedupe by not appending streaming row if last message is agent with same `agentRunId` (optional guard) |
| Empty agent body blocked run | Still clear streaming on finish; show blocked summary from refetch |

### Accessibility

- Streaming bubble: `aria-live="polite"` on text container.
- Session search: label + keyboard focus order before list.

## Testing

| ID | Case | Layer |
|----|------|-------|
| U1 | `list_sessions` returns preview + `hasActiveRun` | integration |
| U2 | Preview truncation 120 chars | unit (server helper) |
| U3 | `hasActiveRun` true when chat run queued | integration |
| U4 | Client filter matches agent name and preview | Vitest |
| U5 | `ChatMessageList` shows streaming row when prop set | Vitest |
| U6 | `ChatPage` search hides non-matching sessions | Vitest |
| U7 | Existing `ChatPage` / `ChatMessageList` tests updated; no `ChatLiveTurn` below list in DOM | Vitest |

Regression: `make web-test`; optional `integration_chat` if API assertions added.

## Rollout

1. Server migration not required (computed fields only).
2. API + schemas + integration test U1–U3.
3. `useChatRunLiveStream` + `ChatMessageList` streaming row.
4. Remove below-list `ChatLiveTurn` from pane.
5. Sidebar preview, active indicator, search.
6. Docs: short note in `docs/web/DESIGN.md` or M09 doc pointer — chat transcript is single-column bubbles.

## Success criteria

- During a turn, user sees **one** agent bubble growing in the transcript (no second live panel).
- Finished turn matches final persisted bubble without jarring layout swap.
- Session list shows meaningful preview and visible active state during runs.
- Client search filters sessions without new API parameters.
