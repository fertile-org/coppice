# Chat UI — Transcript streaming & session list Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show agent replies as a single growing moss bubble inside the chat transcript (no below-list live panel) and enrich the session sidebar with preview, active-run indicator, and client search.

**Architecture:** Server adds computed list fields on chat sessions (preview + `hasActiveRun`). Web extracts WS logic from `ChatLiveTurn` into `useChatRunLiveStream`, feeds text into `ChatMessageList` as a synthetic virtual row, and updates `ChatPage` sidebar + polling. `ChatLiveTurn` remains testable or becomes a thin wrapper around the hook for ticket reuse later — chat pane stops mounting it.

**Tech Stack:** Rust (Axum, SQLx), React 19, TanStack Query, TanStack Virtual, Zod, Vitest, existing `/ws/agent-runs/:runId/live`.

**Spec:** [docs/superpowers/specs/2026-09-28-chat-ui-transcript-and-session-list-design.md](../specs/2026-09-28-chat-ui-transcript-and-session-list-design.md)

---

## File map

| File | Responsibility |
|------|----------------|
| `server/src/services/chat_service.rs` | `truncate_message_preview`, enrich `list_sessions` / `get_session` queries |
| `server/src/domain/chat_session.rs` | Optional `ChatSessionSummary` fields or extend `ChatSession` |
| `server/src/api/chat.rs` | `SessionResponse` new fields + mapping |
| `server/tests/integration_chat.rs` | U1, U3 API assertions |
| `web/src/lib/schemas/chat.ts` | Zod + types for new session fields |
| `web/src/lib/chatSessionSearch.ts` | `filterChatSessions(sessions, agents, query)` pure function |
| `web/src/features/chat/useChatRunLiveStream.ts` | WS hook extracted from `ChatLiveTurn` |
| `web/src/features/chat/ChatMessageList.tsx` | `streamingAgent` prop, streaming bubble |
| `web/src/features/chat/ChatMessageList.test.tsx` | U5 streaming row |
| `web/src/features/chat/ChatSessionPane.tsx` (or inline in `ChatPage.tsx`) | Wire hook, remove `ChatLiveTurn` sibling |
| `web/src/features/chat/ChatPage.tsx` | Search input, row layout, poll sessions |
| `web/src/features/chat/useChat.ts` | `refetchInterval` when any `hasActiveRun`; invalidate sessions on post/finish |
| `web/src/features/chat/ChatPage.test.tsx` | U6 search + session row fields |
| `web/src/lib/schemas/chat.test.ts` | Parse sessions with new fields |
| `docs/web/DESIGN.md` | One paragraph on chat transcript pattern |

---

## Test matrix

| ID | Case | Layer |
|----|------|-------|
| U1 | List sessions includes `lastMessagePreview` after message | integration |
| U2 | Preview helper truncates at 120 chars | unit (`chat_service`) |
| U3 | `hasActiveRun` true while chat run queued | integration |
| U4 | `filterChatSessions` matches name and preview | Vitest |
| U5 | Streaming row visible in `ChatMessageList` | Vitest |
| U6 | Chat search hides non-matching sessions | Vitest |
| U7 | No `data-testid` for below-list live turn in session pane | Vitest |

---

### Task 1: Server preview helper + unit test

**Files:**
- Modify: `server/src/services/chat_service.rs`
- Test: inline `#[cfg(test)]` in same file or `server/src/services/chat_service_preview.rs` (prefer inline mod tests)

- [ ] **Step 1: Add pure helper**

```rust
pub const CHAT_MESSAGE_PREVIEW_MAX_LEN: usize = 120;

pub fn truncate_message_preview(body: &str) -> String {
    let collapsed = body.replace(['\r', '\n'], " ").split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= CHAT_MESSAGE_PREVIEW_MAX_LEN {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(CHAT_MESSAGE_PREVIEW_MAX_LEN).collect();
    out.push('…');
    out
}
```

- [ ] **Step 2: Failing unit test**

```rust
#[test]
fn truncate_message_preview_caps_at_120() {
    let long = "a".repeat(150);
    let preview = truncate_message_preview(&long);
    assert!(preview.chars().count() <= 121);
    assert!(preview.ends_with('…'));
}
```

Run: `cargo test -p coppice-server truncate_message_preview --features embedded-test-db`

- [ ] **Step 3: Commit**

```bash
git add server/src/services/chat_service.rs
git commit -m "feat(chat): add message preview truncation helper"
```

---

### Task 2: Session list API fields

**Files:**
- Modify: `server/src/domain/chat_session.rs`
- Modify: `server/src/services/chat_service.rs` (`list_sessions`, `get_session_by_id`, `get_session`)
- Modify: `server/src/api/chat.rs`

- [ ] **Step 1: Extend domain struct**

```rust
pub struct ChatSession {
    // ... existing ...
    pub last_message_preview: String,
    pub last_message_at: Option<OffsetDateTime>,
    pub last_message_role: Option<String>,
    pub has_active_run: bool,
}
```

Default empty preview / false for `create_session` RETURNING — use subquery in SELECT for list/get.

- [ ] **Step 2: Single SQL pattern for list (example)**

```sql
SELECT
  cs.id, cs.project_id, ... cs.updated_at,
  COALESCE(lm.body, '') AS last_message_body,
  lm.role AS last_message_role,
  lm.created_at AS last_message_at,
  EXISTS (
    SELECT 1 FROM agent_runs ar
    WHERE ar.chat_session_id = cs.id
      AND ar.status IN ('queued', 'running')
  ) AS has_active_run
FROM chat_sessions cs
LEFT JOIN LATERAL (
  SELECT body, role, created_at
  FROM chat_messages
  WHERE session_id = cs.id
  ORDER BY seq DESC
  LIMIT 1
) lm ON true
WHERE cs.owner_user_id = $1
ORDER BY cs.updated_at DESC
```

Map `last_message_body` through `truncate_message_preview` in `row_to_session`.

- [ ] **Step 3: Extend `SessionResponse`**

```rust
struct SessionResponse {
    // ... existing ...
    last_message_preview: String,
    last_message_at: Option<String>,
    last_message_role: Option<String>,
    has_active_run: bool,
}
```

Update `session_response()` to copy from `ChatSession`.

- [ ] **Step 4: Integration tests U1 + U3**

In `server/tests/integration_chat.rs`:

```rust
#[tokio::test]
async fn list_sessions_includes_message_preview() {
    // bootstrap, create session, post message, poll agent reply
    let list = app.oneshot(common::json_request("GET", "/api/chat/sessions", "", &cookie, &csrf)).await.unwrap();
    let body: serde_json::Value = common::json_body(list).await;
    let preview = body["sessions"][0]["lastMessagePreview"].as_str().unwrap_or("");
    assert!(preview.contains("cwd") || !preview.is_empty());
}

#[tokio::test]
async fn list_sessions_has_active_run_while_turn_queued() {
    // post message, immediately GET sessions before worker finishes
    // assert hasActiveRun == true (may need MOCK delay env or race — alternatively enqueue and check DB)
}
```

If race is flaky, assert `hasActiveRun` via SQL right after POST returns 201 and before polling completes.

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_chat list_sessions_includes`

- [ ] **Step 5: Commit**

```bash
git add server/src/domain/chat_session.rs server/src/services/chat_service.rs server/src/api/chat.rs server/tests/integration_chat.rs
git commit -m "feat(chat): expose session preview and active run on list API"
```

---

### Task 3: Web Zod schemas

**Files:**
- Modify: `web/src/lib/schemas/chat.ts`
- Modify: `web/src/lib/schemas/chat.test.ts`

- [ ] **Step 1: Extend schema**

```typescript
export const chatSessionSchema = z.object({
  // ... existing ...
  lastMessagePreview: z.string().default(''),
  lastMessageAt: z.string().nullable().optional(),
  lastMessageRole: chatMessageRoleSchema.nullable().optional(),
  hasActiveRun: z.boolean().default(false),
});
```

- [ ] **Step 2: Test fixture parse**

```typescript
it('parses session list with preview fields', () => {
  chatSessionListSchema.parse({
    sessions: [{
      id: '00000000-0000-4000-8000-000000000001',
      projectId: null,
      ownerUserId: '00000000-0000-4000-8000-000000000002',
      agentId: '00000000-0000-4000-8000-000000000010',
      repoId: null,
      status: 'active',
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
      lastMessagePreview: 'Hello',
      hasActiveRun: true,
    }],
  });
});
```

Run: `cd web && yarn test src/lib/schemas/chat.test.ts`

- [ ] **Step 3: Commit**

```bash
git add web/src/lib/schemas/chat.ts web/src/lib/schemas/chat.test.ts
git commit -m "feat(web): extend chat session schema for list preview"
```

---

### Task 4: Client session search helper

**Files:**
- Create: `web/src/lib/chatSessionSearch.ts`
- Create: `web/src/lib/chatSessionSearch.test.ts`

- [ ] **Step 1: Implement filter**

```typescript
import type { ChatSession } from './schemas/chat';

export function filterChatSessions(
  sessions: ChatSession[],
  agentNameById: Map<string, string>,
  query: string,
): ChatSession[] {
  const q = query.trim().toLowerCase();
  if (!q) return sessions;
  return sessions.filter((session) => {
    const name = (agentNameById.get(session.agentId) ?? '').toLowerCase();
    const preview = session.lastMessagePreview.toLowerCase();
    return name.includes(q) || preview.includes(q);
  });
}
```

- [ ] **Step 2: Tests U4**

```typescript
expect(filterChatSessions([session], map, 'backend')).toHaveLength(1);
expect(filterChatSessions([session], map, 'nomatch')).toHaveLength(0);
```

Run: `cd web && yarn test src/lib/chatSessionSearch.test.ts`

- [ ] **Step 3: Commit**

```bash
git add web/src/lib/chatSessionSearch.ts web/src/lib/chatSessionSearch.test.ts
git commit -m "feat(web): client-side chat session search filter"
```

---

### Task 5: Extract `useChatRunLiveStream`

**Files:**
- Create: `web/src/features/chat/useChatRunLiveStream.ts`
- Modify: `web/src/features/chat/ChatLiveTurn.tsx` (delegate to hook — keeps existing tests green)
- Create: `web/src/features/chat/useChatRunLiveStream.test.ts` (optional; or move WS tests from `ChatLiveTurn.test.tsx`)

- [ ] **Step 1: Hook API**

```typescript
export interface ChatRunLiveState {
  text: string;
  connection: 'connecting' | 'open' | 'closed' | 'reconnecting';
  error: string | null;
  finished: boolean;
}

export function useChatRunLiveStream(
  runId: string | null,
  options?: { onFinished?: () => void },
): ChatRunLiveState
```

Move from `ChatLiveTurn.tsx`:
- WS URL `/ws/agent-runs/${runId}/live`
- OpenCode reducers OR simplify to **text accumulator only** for chat:
  - On `snapshot` / `event` (OpenCode): extract assistant text from parts (copy `AssistantMessage` text extraction or incremental deltas)
  - On `event` (`*.console.*`): append `text` / `result.summary` from `applyClaudeConsoleEvent` state — expose only merged `text` string, not full console UI
- On `end` / terminal `runStatus`: call `onFinished`, set `finished: true`
- Reconnect: copy `reconnectToken` / `recoverableRef` behavior from `ChatLiveTurn`

- [ ] **Step 2: Refactor `ChatLiveTurn` to use hook**

Keep rendering `ChatStreamView` / `ChatConsolePreview` for ticket drawer if still used elsewhere; grep usages.

Run: `cd web && yarn test src/features/chat/ChatLiveTurn.test.tsx`

- [ ] **Step 3: Commit**

```bash
git add web/src/features/chat/useChatRunLiveStream.ts web/src/features/chat/ChatLiveTurn.tsx
git commit -m "refactor(web): extract chat run live WebSocket hook"
```

---

### Task 6: Streaming row in `ChatMessageList`

**Files:**
- Modify: `web/src/features/chat/ChatMessageList.tsx`
- Modify: `web/src/features/chat/ChatMessageList.test.tsx`

- [ ] **Step 1: Extend `ChatMessageBubble`**

```typescript
export function ChatMessageBubble({
  message,
  streaming,
  streamingText,
}: {
  message?: ChatMessage;
  streaming?: boolean;
  streamingText?: string;
}) {
  // When streaming without message, render agent moss bubble with streamingText or ThinkingIndicator inside
}
```

- [ ] **Step 2: `ChatMessageList` props**

```typescript
streamingAgent?: { runId: string; text: string; error?: string | null };
```

Virtualizer `count = messages.length + (streamingAgent ? 1 : 0)`. Last index renders streaming bubble with `aria-busy`, `aria-live="polite"`.

Remove standalone `{thinking ? <ThinkingIndicator /> : null}` below list when `streamingAgent` provided; keep `thinking` only when awaiting without runId.

- [ ] **Step 3: Test U5**

```typescript
render(<ChatMessageList messages={[]} streamingAgent={{ runId: 'run-1', text: 'Partial…' }} />);
expect(screen.getByText(/Partial/)).toBeInTheDocument();
```

Run: `cd web && yarn test src/features/chat/ChatMessageList.test.tsx`

- [ ] **Step 4: Commit**

```bash
git add web/src/features/chat/ChatMessageList.tsx web/src/features/chat/ChatMessageList.test.tsx
git commit -m "feat(web): in-list streaming agent bubble for chat"
```

---

### Task 7: Wire `ChatSessionPane` (remove below-list live)

**Files:**
- Modify: `web/src/features/chat/ChatPage.tsx` (`ChatSessionPane` section)
- Modify: `web/src/features/chat/useChat.ts`

- [ ] **Step 1: Replace `ChatLiveTurn` block**

```tsx
const live = useChatRunLiveStream(activeRunId, {
  onFinished: () => {
    setActiveRunId(null);
    void refetch();
    void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
  },
});

const streamingAgent = activeRunId
  ? { runId: activeRunId, text: live.text, error: live.error }
  : undefined;

<ChatMessageList
  messages={messages}
  streamingAgent={streamingAgent}
/>
// DELETE: {activeRunId ? <ChatLiveTurn ... /> : null}
```

- [ ] **Step 2: On mount when session `hasActiveRun`**

If opening a session with active run but no `activeRunId`, GET messages for last human `agentRunId` or add optional `GET /api/chat/sessions/:id` field `activeRunId` — **YAGNI:** poll messages + `useChatSession().hasActiveRun`; when true and no agent message after last human, fetch `GET /api/agent-runs?chatSessionId=` if exists — **simpler v1:** extend session GET with `activeRunId: Option<Uuid>` in same Task 2 query (`SELECT id FROM agent_runs WHERE ... LIMIT 1`).

Add `activeRunId` to session response when `has_active_run` (optional enhancement in Task 2 if not done).

- [ ] **Step 3: `useChatSessions` polling**

```typescript
export function useChatSessions(projectId?: string | null) {
  const query = useQuery({ ... });
  const anyActive = query.data?.some((s) => s.hasActiveRun) ?? false;
  return useQuery({
    ...
    refetchInterval: anyActive ? 3000 : false,
  });
}
```

- [ ] **Step 4: Commit**

```bash
git add web/src/features/chat/ChatPage.tsx web/src/features/chat/useChat.ts
git commit -m "feat(web): unified chat transcript streaming in message list"
```

---

### Task 8: Sidebar search + preview UI

**Files:**
- Modify: `web/src/features/chat/ChatPage.tsx` (`SessionListItem`, aside)
- Modify: `web/src/features/chat/ChatPage.test.tsx`

- [ ] **Step 1: Search state + input**

```tsx
const [search, setSearch] = useState('');
const visibleSessions = useMemo(
  () => filterChatSessions(sessions, agentNameById, search),
  [sessions, agentNameById, search],
);
```

- [ ] **Step 2: Update `SessionListItem`**

```tsx
{session.hasActiveRun ? (
  <span className="text-moss-700" data-testid="chat-session-active">Replying…</span>
) : null}
<p className="truncate text-xs text-text-muted">
  {prefix}{session.lastMessagePreview || 'No messages yet'}
</p>
```

Prefix: `lastMessageRole === 'human' ? 'You: ' : ''`

- [ ] **Step 3: Tests U6 + U7**

Update mock `sessions` in `ChatPage.test.tsx` with `lastMessagePreview`, `hasActiveRun`. Assert search filters list; open session pane and assert `queryByTestId('chat-live-turn')` null if that testid existed.

Run: `cd web && yarn test src/features/chat/ChatPage.test.tsx`

- [ ] **Step 4: Commit**

```bash
git add web/src/features/chat/ChatPage.tsx web/src/features/chat/ChatPage.test.tsx
git commit -m "feat(web): chat session list preview, active state, and search"
```

---

### Task 9: Docs + final verification

**Files:**
- Modify: `docs/web/DESIGN.md`

- [ ] **Step 1: Add under Components or new subsection**

> Agent Chat transcript uses the same moss agent bubbles for streaming and final text; live runs do not use a separate console block in the chat column.

- [ ] **Step 2: Run verification**

```bash
cargo test -p coppice-server --features embedded-test-db --test integration_chat list_sessions
cd web && yarn test
cargo clippy -p coppice-server --features embedded-test-db -- -D warnings
```

- [ ] **Step 3: Commit**

```bash
git add docs/web/DESIGN.md
git commit -m "docs(web): note unified chat transcript streaming pattern"
```

---

## Spec self-review (plan vs spec)

| Spec requirement | Task |
|------------------|------|
| Preview 120 chars | Task 1–2 |
| `hasActiveRun` | Task 2 |
| Client search only | Task 4, 8 |
| In-list streaming bubble | Task 5–7 |
| Remove below-list `ChatLiveTurn` in pane | Task 7 |
| WS reuse | Task 5 |
| Poll sessions 3s when active | Task 7 |
| Invalidate sessions on post/finish | Task 7 |
| Tests U1–U7 | Tasks 2, 4, 6, 8 |
| DESIGN.md note | Task 9 |

**Optional explicit addition:** `activeRunId` on `GET /api/chat/sessions/:id` when `hasActiveRun` — include in Task 2 Step 2 to simplify reattach on navigation (recommended).

No TBD placeholders in task steps.
