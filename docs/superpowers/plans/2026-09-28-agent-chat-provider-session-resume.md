# Agent Chat provider session resume — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reuse vendor CLI/API sessions across Agent Chat turns (opencode, cursor, claude-code, codex) with slim context on resume and hybrid full-transcript fallback, while enabling chat for opencode and codex.

**Architecture:** Persist `provider_session_id` on `chat_sessions`; `execute_chat_turn` attempts resume when id matches bound agent connector, else full transcript. New `build_conversation_resume_context` for slim turns. OpenCode `run_session` reuses existing HTTP session via `prompt_async`. `ProviderError::ResumeSessionInvalid` triggers one fallback attempt. Mock provider emits session ids and supports `MOCK_CHAT_RESUME_FAIL` for integration tests.

**Tech Stack:** Rust (Axum, SQLx, Tokio), PostgreSQL migrations, MockProvider fixtures, Vitest (unchanged for this feature), `e2e/smoke/m09-chat.mjs`.

**Spec:** [docs/superpowers/specs/2026-09-28-agent-chat-provider-session-resume-design.md](../specs/2026-09-28-agent-chat-provider-session-resume-design.md)

---

## File map

| File | Responsibility |
|------|----------------|
| `server/migrations/025_chat_provider_session.sql` | `provider_session_id`, `provider_session_connector` on `chat_sessions` |
| `server/src/domain/chat_session.rs` | New optional fields on `ChatSession` |
| `server/src/services/chat_service.rs` | SELECT/INSERT mapping; `set_provider_session`, `clear_provider_session`, `get_message_body`; cutoff clears parent provider id |
| `server/src/services/context_builder.rs` | `build_conversation_resume_context` + unit tests |
| `server/src/providers/mod.rs` | `ProviderError::ResumeSessionInvalid`; `CHAT_RESUME_CONNECTORS`; `is_resume_session_invalid` helper |
| `server/src/providers/mock.rs` | Chat session id + resume-fail env for integration |
| `server/src/providers/opencode.rs` | Pass `resume_session_id`; remove read_only refusal |
| `server/src/providers/codex.rs` | Remove read_only refusal; map resume CLI errors to `ResumeSessionInvalid` where detectable |
| `server/src/providers/claude_code.rs` | Map obvious resume failures to `ResumeSessionInvalid` (stderr substring) |
| `server/src/providers/cursor.rs` | Same resume error mapping if not already |
| `server/src/sessions/opencode_client.rs` | `run_session` accepts `resume_session_id`; axum mock HTTP test |
| `server/src/workers/job_worker.rs` | Refactor `execute_chat_turn` (resume + fallback + logging + persist) |
| `server/tests/integration_chat.rs` | Multi-turn + fallback tests |
| `docs/providers/README.md` | Agent Chat multi-turn parity table |
| `docs/providers/{opencode,cursor,claude-code,codex}.md` | Session reuse + risk notes |
| `docs/superpowers/specs/2026-09-08-m09-agent-chat-design.md` | Footnote pointer to 2026-09-28 spec |
| `e2e/smoke/m09-chat.mjs` | Second human message in same session |

---

## Test matrix (must pass before done)

| ID | Case | Layer |
|----|------|-------|
| R1 | `build_conversation_resume_context` omits full transcript; includes single human line | unit (`context_builder`) |
| R2 | `is_resume_session_invalid` true for `ResumeSessionInvalid` | unit (`providers/mod`) |
| R3 | Mock accepts `read_only_tools: true` for opencode/codex (no `InvalidInput`) | unit (`providers`) |
| R4 | Claude/Cursor/Codex argv includes resume when id set | unit (existing + extend) |
| R5 | OpenCode resume path: no `POST /session`, uses `prompt_async` on existing id | unit (`opencode_client`) |
| R6 | Two chat messages → `chat_sessions.provider_session_id` set | integration |
| R7 | Turn 2 context file lacks second copy of turn-1 agent reply in transcript section (slim) | integration (env on mock) |
| R8 | `MOCK_CHAT_RESUME_FAIL=1` → turn still succeeds via fallback | integration |
| R9 | Cutoff child has `provider_session_id` NULL | integration |
| R10 | `make e2e-smoke-m09` two turns | smoke |
| R11 | `cargo clippy --workspace -- -D warnings` | CI |

---

### Task 1: Migration and domain model

**Files:**
- Create: `server/migrations/025_chat_provider_session.sql`
- Modify: `server/src/domain/chat_session.rs`
- Modify: `server/src/services/chat_service.rs` (all `chat_sessions` queries + `row_to_session`)

- [ ] **Step 1: Add migration**

```sql
ALTER TABLE chat_sessions
    ADD COLUMN provider_session_id TEXT NULL,
    ADD COLUMN provider_session_connector TEXT NULL;
```

- [ ] **Step 2: Extend `ChatSession` struct**

```rust
pub struct ChatSession {
    // ... existing fields ...
    pub provider_session_id: Option<String>,
    pub provider_session_connector: Option<String>,
}
```

- [ ] **Step 3: Update every `SELECT` on `chat_sessions` in `chat_service.rs`**

Add columns to SELECT lists and `row_to_session`:

```rust
provider_session_id: row.try_get("provider_session_id").ok().flatten(),
provider_session_connector: row.try_get("provider_session_connector").ok().flatten(),
```

`create_session_inner` INSERT does not set these (NULL default).

- [ ] **Step 4: Add ChatService helpers**

```rust
pub async fn set_provider_session(
    &self,
    session_id: Uuid,
    connector: &str,
    provider_session_id: &str,
) -> Result<(), ChatError> { /* UPDATE chat_sessions SET provider_session_id = $2,
    provider_session_connector = $3, updated_at = now() WHERE id = $1 */ }

pub async fn clear_provider_session(&self, session_id: Uuid) -> Result<(), ChatError> { /* SET both NULL */ }

pub async fn get_message_body(&self, message_id: Uuid) -> Result<String, ChatError> { /* SELECT body FROM chat_messages WHERE id = $1 */ }
```

- [ ] **Step 5: Run migration via tests**

Run: `cargo test -p coppice-server --features embedded-test-db integration_chat::post_message_runs_mock_chat_turn_and_persists_reply -- --nocapture`

Expected: PASS (or compile-only if test name unchanged).

- [ ] **Step 6: Commit**

```bash
git add server/migrations/025_chat_provider_session.sql server/src/domain/chat_session.rs server/src/services/chat_service.rs
git commit -m "feat(chat): add provider session columns on chat_sessions"
```

---

### Task 2: Provider error type and resume helpers

**Files:**
- Modify: `server/src/providers/mod.rs`

- [ ] **Step 1: Add error variant**

```rust
#[derive(Debug, Error)]
pub enum ProviderError {
    // ... existing ...
    #[error("resume session invalid: {0}")]
    ResumeSessionInvalid(String),
}
```

- [ ] **Step 2: Connector list constant**

```rust
pub const CHAT_RESUME_CONNECTORS: &[&str] = &["opencode", "claude-code", "cursor", "codex"];

pub fn connector_supports_chat_resume(connector: &str) -> bool {
    CHAT_RESUME_CONNECTORS.iter().any(|c| *c == connector)
}
```

- [ ] **Step 3: Classification helper**

```rust
pub fn is_resume_session_invalid(err: &ProviderError) -> bool {
    match err {
        ProviderError::ResumeSessionInvalid(_) => true,
        ProviderError::InvalidInput(msg) | ProviderError::InvalidFixture(msg) => {
            let m = msg.to_ascii_lowercase();
            m.contains("session not found")
                || m.contains("invalid resume")
                || m.contains("unknown session")
        }
        _ => false,
    }
}
```

- [ ] **Step 4: Unit test**

```rust
#[test]
fn resume_invalid_detection() {
    assert!(is_resume_session_invalid(&ProviderError::ResumeSessionInvalid("x".into())));
    assert!(is_resume_session_invalid(&ProviderError::InvalidInput("session not found".into())));
    assert!(!is_resume_session_invalid(&ProviderError::Cancelled));
}
```

Run: `cargo test -p coppice-server is_resume_invalid_detection --features embedded-test-db`

- [ ] **Step 5: Commit**

```bash
git add server/src/providers/mod.rs
git commit -m "feat(providers): add ResumeSessionInvalid for chat resume fallback"
```

---

### Task 3: Slim conversation context builder

**Files:**
- Modify: `server/src/services/context_builder.rs`

- [ ] **Step 1: Write failing unit test**

```rust
#[test]
fn conversation_resume_context_includes_only_latest_human_message() {
    let input = ContextInput {
        ticket_title: "Agent Chat",
        ticket_description: "",
        ticket_status: "n/a",
        ticket_substatus: None,
        agent_name: "BE",
        agent_key: "backend_engineer",
        agent_role: "Backend",
        agent_skills: &[],
        agent_responsibilities: &[],
        agent_system_prompt: "Be helpful.",
        repo_name: None,
        repo_remote_url: None,
        repo_default_branch: None,
        worktree_path: Some("/tmp/chat"),
        latest_comments: Some("What is the second question?"),
        project_rules: None,
        resume_context: None,
        context_profile: ContextProfile::Conversation,
        human_request: None,
        ticket_id: None,
        assignee_agent_key: None,
        thread_excerpt: None,
    };
    let md = build_conversation_resume_context(&input);
    assert!(md.contains("What is the second question?"));
    assert!(!md.contains("# Conversation transcript"));
    assert!(md.contains("# Latest human message"));
}
```

- [ ] **Step 2: Run test — expect FAIL** (undefined function)

Run: `cargo test -p coppice-server conversation_resume_context --features embedded-test-db`

- [ ] **Step 3: Implement `build_conversation_resume_context`**

Mirror `build_conversation_context` but replace transcript section header with `# Latest human message` and use `latest_comments` as that single body (caller passes human text only).

- [ ] **Step 4: Run test — PASS**

- [ ] **Step 5: Commit**

```bash
git add server/src/services/context_builder.rs
git commit -m "feat(chat): add slim resume context builder for chat turns"
```

---

### Task 4: Mock provider — session id + resume fail hook

**Files:**
- Modify: `server/src/providers/mock.rs`

- [ ] **Step 1: After fixture load, before return — chat_turn session + fail hook**

```rust
if input.job_type == "chat_turn" {
    if let Some(tx) = &input.session_created_tx {
        let sid = input
            .resume_session_id
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "mock-chat-session".to_string());
        let _ = tx.send(sid);
    }
    if std::env::var("MOCK_CHAT_RESUME_FAIL").as_deref() == Ok("1")
        && input
            .resume_session_id
            .as_ref()
            .is_some_and(|s| !s.is_empty())
    {
        return Err(ProviderError::ResumeSessionInvalid(
            "mock forced resume failure".into(),
        ));
    }
    if std::env::var("MOCK_CHAT_EXPECT_SLIM").as_deref() == Ok("1") {
        let body = std::fs::read_to_string(&input.context_path).map_err(ProviderError::Io)?;
        if body.contains("# Conversation transcript") {
            return Err(ProviderError::InvalidFixture(
                "expected slim resume context".into(),
            ));
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/providers/mock.rs
git commit -m "test(mock): support chat resume session id and failure env hooks"
```

---

### Task 5: OpenCode client resume

**Files:**
- Modify: `server/src/sessions/opencode_client.rs`
- Modify: `server/src/providers/opencode.rs`

- [ ] **Step 1: Extend `run_session` signature**

Add parameter: `resume_session_id: Option<&str>` after `model: Option<&str>`.

```rust
let session_id = if let Some(sid) = resume_session_id.filter(|s| !s.is_empty()) {
    // Optional: session_status or fetch_messages probe; on 404 return ResumeSessionInvalid
    sid.to_string()
} else {
    self.create_session(&directory, model_provider, model).await?
};
```

Only call `create_session` when `resume_session_id` is None/empty.

On `prompt_async` failure with HTTP 404, return `ProviderError::ResumeSessionInvalid(...)`.

- [ ] **Step 2: Wire provider**

Remove early `refuse_unsupported_read_only` block in `opencode.rs`. Pass `input.resume_session_id.as_deref()` into `run_session`.

- [ ] **Step 3: HTTP recording test (axum)**

In `opencode_client.rs` `#[cfg(test)]` module, spin a minimal axum server:

- Track `AtomicUsize` `create_session_posts`.
- Route `POST /session` increments counter, returns `{"id":"sess-new"}`.
- Route `POST /session/:id/prompt_async` returns 200.
- Route `GET /session/:id/message` returns `[]` + idle status routes stubbed minimally (copy patterns from existing client if any; otherwise stub `session/status` and SSE with immediate idle — follow `wait_idle` requirements).

Assert: with `resume_session_id: Some("sess-existing")`, `create_session_posts == 0`.

Run: `cargo test -p coppice-server opencode_resume --features embedded-test-db`

- [ ] **Step 4: Commit**

```bash
git add server/src/sessions/opencode_client.rs server/src/providers/opencode.rs
git commit -m "feat(opencode): resume chat sessions without recreating HTTP session"
```

---

### Task 6: Codex + Claude + Cursor chat gates and resume errors

**Files:**
- Modify: `server/src/providers/codex.rs`
- Modify: `server/src/providers/claude_code.rs` (optional stderr mapping on non-zero exit when `--resume` used)
- Modify: `server/tests/integration_chat.rs` (replace `conversation_profile_refuses_write_capable_connectors`)

- [ ] **Step 1: Remove codex read_only refusal**

Delete the block:

```rust
if input.read_only_tools {
    return Err(refuse_unsupported_read_only(self.id()));
}
```

- [ ] **Step 2: Map resume failures**

When `input.resume_session_id.is_some()` and process fails, if stderr contains `session` + `not found` (or codex-specific strings), return `ProviderError::ResumeSessionInvalid(stderr_snippet)`.

- [ ] **Step 3: Replace integration test**

Remove cursor-from-refuses test as the sole check. Add:

```rust
#[tokio::test]
async fn conversation_profile_accepts_read_only_for_codex_and_opencode() {
    // CodexProvider::new + OpenCodeProvider::new with enabled config
    // run(AgentRunInput { job_type: "chat_turn", read_only_tools: true, ... minimal paths })
    // For opencode, skip if requires serve — unit test OpenCodeProvider only checks no InvalidInput at start:
    // actually call run and expect FixtureNotFound or Io, NOT InvalidInput "read-only"
}
```

Split into two tests in `codex.rs` / `opencode.rs` `#[cfg(test)]` modules:

```rust
#[tokio::test]
async fn chat_turn_does_not_refuse_read_only_tools() {
    let err = provider.run(chat_turn_input_read_only()).await;
    assert!(!matches!(err, Err(ProviderError::InvalidInput(msg)) if msg.contains("read-only")));
}
```

- [ ] **Step 4: Commit**

```bash
git add server/src/providers/codex.rs server/src/providers/claude_code.rs server/tests/integration_chat.rs
git commit -m "feat(chat): allow codex/opencode conversation turns; classify resume errors"
```

---

### Task 7: Refactor `execute_chat_turn` (core)

**Files:**
- Modify: `server/src/workers/job_worker.rs`

- [ ] **Step 1: Extract `spawn_session_created_tx` helper** (shared with ticket path)

```rust
fn spawn_session_created_tx(
    pool: &PgPool,
    run_id: Uuid,
    connector_name: &str,
) -> Option<watch::Sender<String>> {
    if !connector_supports_chat_resume(connector_name) {
        return None;
    }
    // same tokio::spawn + RunService::set_session_id as ticket worker
}
```

- [ ] **Step 2: Extract `run_chat_provider_attempt`**

Parameters: state, pool, run, session, agent, cwd, context_input, resume_session_id, session_created_tx.

Returns `Result<AgentRunResult, ProviderError>`.

- [ ] **Step 3: Rewrite `execute_chat_turn` body**

Pseudocode to implement literally:

```rust
let human_body = if let Some(mid) = run.chat_message_id {
    ChatService::new(pool).get_message_body(mid).await?
} else {
    String::new()
};

let stored_resume = session.provider_session_id.clone().filter(|s| !s.is_empty());
let connector_matches = session.provider_session_connector.as_deref() == Some(connector_name.as_str());
let mut chat_resume_attempted = false;
let mut chat_resume_used = false;
let mut chat_resume_fallback = false;

let mut result = if stored_resume.is_some() && connector_matches {
    chat_resume_attempted = true;
    chat_resume_used = true;
    let slim_input = ContextInput { latest_comments: Some(&human_body), /* ... */ };
    let md = build_conversation_resume_context(&slim_input);
    write_context_document(&cwd, &md)?;
    let tx = spawn_session_created_tx(pool, run.id, connector_name);
    match run_chat_provider_attempt(..., stored_resume.clone(), tx).await {
        Ok(r) => r,
        Err(e) if is_resume_session_invalid(&e) => {
            chat_resume_used = false;
            chat_resume_fallback = true;
            tracing::info!(chat_resume_fallback = true, "chat resume invalid, full transcript fallback");
            // fall through to full path below
            run_full_chat_attempt(...)
        }
        Err(e) => return Err(e.into()),
    }
} else {
    run_full_chat_attempt(...)
};

// on success:
if let Some(sid) = run_session_id(pool, run.id).await {
    ChatService::new(pool).set_provider_session(session_id, connector_name, &sid).await?;
}
// existing append_agent_message, finish_run, artifacts, publish
tracing::info!(
    connector = %connector_name,
    chat_resume_attempted,
    chat_resume_used,
    chat_resume_fallback,
    "chat turn finished"
);
```

Implement `run_full_chat_attempt` using existing transcript + `build_conversation_context`, `resume_session_id: None`, with `session_created_tx` enabled.

On final failure after fallback, `clear_provider_session(session_id)`.

- [ ] **Step 4: Cutoff clears parent provider session**

In `ChatService::cutoff_session`, after patching parent to cutoff, call `clear_provider_session(session_id)`.

- [ ] **Step 5: Manual smoke**

Run: `cargo test -p coppice-server --features embedded-test-db integration_chat::post_message_runs_mock_chat_turn_and_persists_reply`

- [ ] **Step 6: Commit**

```bash
git add server/src/workers/job_worker.rs server/src/services/chat_service.rs
git commit -m "feat(chat): resume provider sessions with full-transcript fallback"
```

---

### Task 8: Integration tests (multi-turn + fallback)

**Files:**
- Modify: `server/tests/integration_chat.rs`

- [ ] **Step 1: `chat_second_turn_sets_provider_session_id`**

After first `post_message` + poll reply, post second message with distinct body, poll again.

```rust
let row: (Option<String>, Option<String>) = sqlx::query_as(
    "SELECT provider_session_id, provider_session_connector FROM chat_sessions WHERE id = $1"
).bind(session.id).fetch_one(&pool).await?;
assert_eq!(row.0.as_deref(), Some("mock-chat-session"));
assert_eq!(row.1.as_deref(), Some("mock"));
```

- [ ] **Step 2: `chat_second_turn_uses_slim_context`**

```rust
std::env::set_var("MOCK_CHAT_EXPECT_SLIM", "1");
// first turn without env; second turn with env — must not error
```

Use a guard to unset env after test (pattern from `mock_env_lock`).

- [ ] **Step 3: `chat_resume_fallback_succeeds`**

```rust
std::env::set_var("MOCK_CHAT_RESUME_FAIL", "1");
// session must already have provider_session_id from a synthetic UPDATE or first turn
// post second message — should still get agent reply
```

Seed `provider_session_id = 'stale-mock'` via SQL after first turn before enabling env.

- [ ] **Step 4: `cutoff_child_has_no_provider_session`**

Extend existing cutoff test or add assertion on `child.provider_session_id.is_none()`.

Run: `cargo test -p coppice-server --features embedded-test-db integration_chat -- --nocapture`

- [ ] **Step 5: Commit**

```bash
git add server/tests/integration_chat.rs
git commit -m "test(chat): multi-turn resume, slim context, and fallback"
```

---

### Task 9: Connector argv parity tests

**Files:**
- Modify: `server/src/providers/claude_code.rs` (tests module)
- Modify: `server/src/providers/cursor.rs` (existing resume test — ensure documented)
- Modify: `server/src/providers/codex.rs` (tests module)

- [ ] **Step 1: Claude unit test for `--resume`**

If missing, add test building command args with `resume_session_id: Some("sess-1")` — mirror cursor’s `cursor_cli_args` test style.

- [ ] **Step 2: Codex unit test for `resume` subcommand**

Assert argv contains `resume` and session id when `resume_session_id` set (may require extracting `build_codex_argv` helper from `run()` for testability).

- [ ] **Step 3: Static parity test in `providers/mod.rs`**

```rust
#[test]
fn chat_resume_connectors_include_all_vendor_chat_clis() {
    for id in ["opencode", "claude-code", "cursor", "codex"] {
        assert!(connector_supports_chat_resume(id));
    }
}
```

- [ ] **Step 4: Commit**

```bash
git add server/src/providers/claude_code.rs server/src/providers/cursor.rs server/src/providers/codex.rs server/src/providers/mod.rs
git commit -m "test(providers): chat resume parity for four connectors"
```

---

### Task 10: Documentation

**Files:**
- Modify: `docs/providers/README.md`
- Modify: `docs/providers/opencode.md`, `docs/providers/codex.md`, `docs/providers/cursor.md`, `docs/providers/claude-code.md`
- Modify: `docs/superpowers/specs/2026-09-08-m09-agent-chat-design.md` (chat-capable table footnote)
- Modify: `docs/testing.md` (one paragraph: multi-turn chat integration tests + env vars)

- [ ] **Step 1: README parity table**

Four rows: resume supported, fallback behavior, read-only enforceability.

- [ ] **Step 2: Per-provider “Agent Chat multi-turn” subsection**

Codex/opencode: document write-tool risk and bypass flags.

- [ ] **Step 3: M09 spec footnote**

Link to `2026-09-28-agent-chat-provider-session-resume-design.md`.

- [ ] **Step 4: Commit**

```bash
git add docs/providers docs/superpowers/specs/2026-09-08-m09-agent-chat-design.md docs/testing.md
git commit -m "docs: agent chat provider session resume for four connectors"
```

---

### Task 11: E2E smoke second message

**Files:**
- Modify: `e2e/smoke/m09-chat.mjs`

- [ ] **Step 1: After first agent reply, post second message**

```javascript
await postHumanMessage(session.id, auth, 'Second question for smoke.');
await pollUntilAgentReply(session.id, auth, { minMessages: 4 }); // 2 human + 2 agent
```

Adjust `pollUntilAgentReply` if needed to accept optional min count.

- [ ] **Step 2: Run smoke**

Run: `make e2e-smoke-m09` (requires compose stack).

- [ ] **Step 3: Commit**

```bash
git add e2e/smoke/m09-chat.mjs
git commit -m "test(e2e): m09 chat smoke covers second turn"
```

---

### Task 12: Final verification

- [ ] **Step 1: Clippy**

Run: `cargo clippy --workspace -- -D warnings`

- [ ] **Step 2: Targeted tests**

Run: `make test-smoke` and `cargo test -p coppice-server --features embedded-test-db integration_chat`

- [ ] **Step 3: Full test (before merge)**

Run: `make test` then `make clean` per AGENTS.md.

---

## Spec self-review (plan vs spec)

| Spec requirement | Task |
|------------------|------|
| `chat_sessions` provider columns | Task 1 |
| Slim + full context | Task 3, 7 |
| Hybrid fallback C | Task 4, 7, 8 |
| Four-connector parity | Tasks 5, 6, 9, 10 |
| OpenCode + Codex chat enabled | Task 5, 6 |
| Logging fields | Task 7 |
| Cutoff child no inherit | Task 1, 7, 8 |
| Docs README + providers | Task 10 |
| Integration + e2e | Tasks 8, 11 |

No TBD placeholders in task steps.
