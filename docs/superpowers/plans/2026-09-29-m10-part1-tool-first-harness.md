# M10 Part 1 — Tool-first Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every agent run talks to Coppice through a per-run MCP gateway (`/mcp`) — reading tickets/knowledge/skills via tools and finishing with `result_submit` — with a slim `context.md` instead of today's fat context.

**Architecture:** A stateless MCP Streamable HTTP endpoint on the existing Axum server authenticates a per-run bearer token, resolves a profile-scoped tool set, dispatches to thin adapters over existing services, and logs every call. `job_worker` mints the token before invoking a connector, passes `McpAccess` through `AgentRunInput`, prefers the stored `result_submit` payload at finish, and revokes the token on every exit. Built-in `coppice` skills replace role/collaboration prose.

**Tech Stack:** Rust (Axum 0.8, SQLx 0.8, Tokio), Postgres, `sha2` (existing), `reqwest` (existing, used by mock tool calls). No new MCP library in Part 1.

**Spec:** [docs/superpowers/specs/2026-09-29-m10-plugins-design.md](../specs/2026-09-29-m10-plugins-design.md) — this plan implements delivery steps 1–4. Part 2 (plugin dirs, git install, proxy, UI) is a separate plan.

## Global Constraints

- Server owns state: tool handlers call existing services; no workflow rules in `mcp/`.
- CI and automated tests use `MockProvider` only; real CLIs are manual acceptance.
- `cargo clippy --workspace -- -D warnings`, `make test`, `make web-test` must pass at the end; iterate with targeted `cargo test -p coppice-server --features embedded-test-db <filter>`.
- New migration: `server/migrations/028_mcp_gateway.sql`.
- Token: 32 random bytes, hex-encoded; only `sha256(hex)` stored; sent only as `Authorization: Bearer`; passed to connectors only via env `COPPICE_MCP_URL` / `COPPICE_MCP_TOKEN` or a per-run file under `<artifacts_dir>/runs/<run_id>/`.
- Never write connector config into a worktree, a registered repo checkout, or the user's global CLI config.
- Default gateway URL: `http://127.0.0.1:{server.port}/mcp` (config `mcp.base_url` overrides).
- Limits (config `[mcp]`): `call_timeout_secs = 60`, `max_output_bytes = 32768`, `comment_post_limit = 5`, token TTL = run timeout + 10 min (use `token_ttl_secs = 14400` default).
- Protocol versions accepted: `2025-06-18`, `2025-03-26` (respond with the client's if supported, else `2025-06-18`).
- Target: slim `full` context ≥50% smaller than the recorded legacy baseline on the same fixture input.
- M05 semantics unchanged: unknown/disabled/self/duplicate/over-limit targets are ignored by the workflow; `result_submit` reports them as warnings only.

## Review Focus

1. **Run stopped mid-flight** — after cancel/stop/failure the token is revoked (next call → 401) and any stored `submitted_result` is not applied. Test in Task 8.
2. **Invalid resubmission after a valid one** — an invalid `result_submit` returns errors and leaves the previous valid submission in place. Test in Task 7.
3. **Cross-scope access** — a token for run A cannot read a ticket on another board or comment on a ticket other than its own. Test in Task 5 and Task 6.
4. **Huge threads** — a ticket with 500 comments returns a bounded page and a truncation hint, never an oversize response. Test in Task 5.
5. **Chat turn without a ticket** — ticket-scoped tools called without `ticketId` in a chat turn return a clear tool error (not a 500). Test in Task 5.

## File Structure

```text
server/src/mcp/
  mod.rs              # module wiring
  protocol.rs         # JSON-RPC types, ToolDefinition, ToolOutput, ToolHost trait, handle_rpc()
  token.rs            # RunToolScope, NewRunToolScope, TokenService (mint/verify/revoke)
  grant.rs            # McpAccess, RunToolGrant (mint + revoke-on-drop), grant_for_run()
  catalog.rs          # profile → tool matrix, tool definitions (schemas + annotations)
  host.rs             # RunToolHost: dispatch, timeout, truncation, run_tool_calls logging
  server.rs           # Axum routes: POST /mcp, GET|DELETE /mcp → 405
  tools/
    mod.rs            # ToolCtx, ToolError
    agents.rs         # board_agents
    tickets.rs        # ticket_get, ticket_comments, ticket_runs, ticket_search
    knowledge.rs      # knowledge_search
    comments.rs       # comment_post
    result.rs         # result_submit
    skills.rs         # skill_list, skill_load
server/src/plugins/
  mod.rs
  builtin.rs          # embedded coppice skills, materialize to data dir
  skills.rs           # SkillCatalog (built-in only in Part 1)
server/builtin-plugins/coppice/skills/<name>/SKILL.md   # 6 platform skills
server/examples/mcp_probe.rs                             # dev probe for connector verification
server/migrations/028_mcp_gateway.sql
config/src/lib.rs                                        # McpConfig
server/src/providers/{mod,mock,claude_code,codex,cursor,kilo_code,opencode}.rs
server/src/services/context_builder.rs                   # slim builders; legacy removed in Task 11
server/src/services/result_contract.rs                   # validate_for_profile
server/src/workers/job_worker.rs, job_worker/compaction.rs
server/tests/integration_mcp.rs
```

---

### Task 1: MCP protocol core + connector verification probe

**Files:**
- Create: `server/src/mcp/mod.rs`, `server/src/mcp/protocol.rs`, `server/examples/mcp_probe.rs`
- Modify: `server/src/lib.rs` (add `pub mod mcp;`)
- Modify: `docs/superpowers/specs/2026-09-29-m10-plugins-design.md` (Connector wiring table → verified table)

**Interfaces:**
- Produces:
  - `pub struct ToolDefinition { pub name: String, pub description: String, pub input_schema: serde_json::Value, pub read_only: bool }` — serialized as MCP `{name, description, inputSchema, annotations: {readOnlyHint}}`.
  - `pub struct ToolOutput { pub text: String, pub is_error: bool }` — serialized as `{content: [{type: "text", text}], isError}`.
  - `#[async_trait] pub trait ToolHost: Send + Sync { fn list(&self) -> Vec<ToolDefinition>; async fn call(&self, name: &str, args: serde_json::Value) -> ToolOutput; }`
  - `pub async fn handle_rpc(body: serde_json::Value, host: &dyn ToolHost) -> Option<serde_json::Value>` — `None` for notifications (no `id`); supports `initialize`, `ping`, `tools/list`, `tools/call`; unknown method → JSON-RPC error `-32601`; batch arrays → `-32600`.
  - `pub const SERVER_NAME: &str = "coppice";`

- [ ] **Step 1: Write failing unit tests in `protocol.rs`**

```rust
#[tokio::test] async fn initialize_echoes_supported_version() // params.protocolVersion "2025-03-26" → result.protocolVersion == "2025-03-26", result.capabilities.tools is an object, result.serverInfo.name == "coppice"
#[tokio::test] async fn initialize_unknown_version_falls_back() // "1999-01-01" → "2025-06-18"
#[tokio::test] async fn notification_returns_none()           // {"jsonrpc":"2.0","method":"notifications/initialized"} → None
#[tokio::test] async fn tools_list_serializes_annotations()   // fake host with one read_only tool → result.tools[0].annotations.readOnlyHint == true, inputSchema present
#[tokio::test] async fn tools_call_wraps_output()             // fake host returns ToolOutput{text:"hi",is_error:true} → result.content[0].text=="hi", result.isError==true
#[tokio::test] async fn unknown_method_is_32601()
```

- [ ] **Step 2: Run** `cargo test -p coppice-server --lib mcp::protocol` — Expected: FAIL (module missing).

- [ ] **Step 3: Implement `protocol.rs`** with serde types and `handle_rpc` exactly as the interfaces above. `tools/call` params: `{name, arguments}` (missing `arguments` → `{}`).

- [ ] **Step 4: Run** the same command — Expected: PASS.

- [ ] **Step 5: Write `examples/mcp_probe.rs`** — a standalone Axum server on `127.0.0.1:${PORT:-5099}` that requires `Authorization: Bearer probe-token`, serves `POST /mcp` via `handle_rpc` with one tool `ping` (returns `"pong <args.message>"`), returns 405 on `GET`, and logs every request method. Run: `cargo run -p coppice-server --example mcp_probe` — Expected: listening log line.

- [ ] **Step 6: Manual verification per connector (record results)**

For each of `claude-code`, `codex`, `cursor`, `kilo-code`, `opencode` (installed via `coppice connector install` or locally): configure the CLI per the spec's wiring rules (per-run flag/env/file only, token via `COPPICE_MCP_TOKEN`) and prompt it: "Call the coppice ping tool with message hello and print the result." Record for each: exact flags/env/file used, whether `pong hello` came back, whether its read-only mode (`claude --allowedTools` read list, `cursor --mode ask`) still allows `mcp__coppice__*` tools, and the tool-name form it shows. Apply the spec's decision rules (cursor not tool-first if no per-run mechanism; opencode switches to per-run `opencode run` if serve cannot carry per-run auth).

- [ ] **Step 7: Replace the spec's "Expected mechanism" table** with a "Verified mechanism" table from Step 6 (one row per connector, exact flags/env, read-only notes).

- [ ] **Step 8: Commit**

```bash
git add server/src/lib.rs server/src/mcp server/examples/mcp_probe.rs docs/superpowers/specs/2026-09-29-m10-plugins-design.md
git commit -m "Add MCP protocol core and record connector verification."
```

---

### Task 2: Config, migration, run tokens

**Files:**
- Modify: `config/src/lib.rs` (add `McpConfig` + `AppConfig.mcp`), `config.example.toml`, `deploy/config/config.example.toml`
- Create: `server/migrations/028_mcp_gateway.sql`, `server/src/mcp/token.rs`
- Modify: `server/src/db/pool.rs` (add new tables to test TRUNCATE list)

**Interfaces:**
- Produces:
  - `pub struct McpConfig { pub base_url: Option<String>, pub call_timeout_secs: u64 /*60*/, pub max_output_bytes: usize /*32768*/, pub comment_post_limit: u32 /*5*/, pub token_ttl_secs: u64 /*14400*/, pub builtin_plugins_dir: String /*"./data/builtin-plugins"*/ }` with `#[serde(default)]` on `AppConfig.mcp`; `impl McpConfig { pub fn gateway_url(&self, server_port: u16) -> String }`.
  - `pub struct NewRunToolScope { pub run_id: Uuid, pub agent_id: Uuid, pub ticket_id: Option<Uuid>, pub chat_session_id: Option<Uuid>, pub board_id: Option<Uuid>, pub profile: ContextProfile, pub job_type: String, pub compaction_ticket_ids: Vec<Uuid> }`
  - `pub struct RunToolScope { pub token_id: Uuid, /* all NewRunToolScope fields */ }`
  - `pub struct TokenService<'a>`; `new(pool: &'a PgPool)`; `async fn mint(&self, scope: &NewRunToolScope, ttl: Duration) -> Result<String, sqlx::Error>` (returns plaintext hex token); `async fn verify(&self, token: &str) -> Result<Option<RunToolScope>, sqlx::Error>` (None if unknown/expired/revoked); `async fn revoke_for_run(&self, run_id: Uuid) -> Result<(), sqlx::Error>`.

Migration `028_mcp_gateway.sql`:

```sql
CREATE TABLE run_tool_tokens (
    id UUID PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    subject_kind TEXT NOT NULL DEFAULT 'run' CHECK (subject_kind IN ('run', 'personal')),
    run_id UUID REFERENCES agent_runs(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL,
    ticket_id UUID,
    chat_session_id UUID,
    board_id UUID,
    context_profile TEXT NOT NULL,
    job_type TEXT NOT NULL,
    compaction_ticket_ids UUID[] NOT NULL DEFAULT '{}',
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX run_tool_tokens_run_idx ON run_tool_tokens (run_id);

CREATE TABLE run_tool_calls (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    tool TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('core', 'skill', 'plugin')),
    plugin_id UUID,
    args_summary TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('ok', 'error', 'denied', 'timeout')),
    error TEXT,
    duration_ms INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX run_tool_calls_run_idx ON run_tool_calls (run_id, created_at);

ALTER TABLE agent_runs ADD COLUMN submitted_result JSONB;
```

- [ ] **Step 1: Write failing tests** — config unit test `mcp_defaults_apply_when_section_missing` (parse a config without `[mcp]` → `call_timeout_secs == 60`, `gateway_url(5000) == "http://127.0.0.1:5000/mcp"`; with `base_url = "http://x/mcp"` → that value). Integration tests in `server/tests/integration_mcp.rs`:

```rust
#[tokio::test] async fn token_mint_verify_roundtrip()      // verify(minted) → Some(scope) with same run_id/profile; stored token_hash != plaintext
#[tokio::test] async fn token_revoked_is_rejected()        // revoke_for_run → verify → None
#[tokio::test] async fn token_expired_is_rejected()        // ttl 0s → verify → None
#[tokio::test] async fn token_unknown_is_rejected()
```

Tests create an agent run row via existing helpers (`bootstrap_and_login_with_state`, `create_test_ticket`, then insert a run through `RunService`).

- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_mcp token_` and the config test — Expected: FAIL.

- [ ] **Step 3: Implement** `McpConfig`, migration, `TokenService` (`rand::rngs::OsRng` 32 bytes → `hex`; `sha2::Sha256` of the hex string → hex for `token_hash`), and add `run_tool_calls, run_tool_tokens` to the test TRUNCATE list.

- [ ] **Step 4: Run** the tests — Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "Add MCP config, gateway tables, and run tokens."`

---

### Task 3: Gateway endpoint, tool catalog, host with logging

**Files:**
- Create: `server/src/mcp/catalog.rs`, `server/src/mcp/host.rs`, `server/src/mcp/server.rs`, `server/src/mcp/tools/mod.rs`, `server/src/mcp/tools/agents.rs`
- Modify: `server/src/api/mod.rs` (merge `crate::mcp::server::routes()` into the **public** router — token auth, no session/CSRF)

**Interfaces:**
- Consumes: `ToolHost`, `handle_rpc` (Task 1); `TokenService`, `RunToolScope` (Task 2).
- Produces:
  - `pub enum CoreTool { BoardAgents, TicketGet, TicketComments, TicketRuns, TicketSearch, KnowledgeSearch, CommentPost, ResultSubmit, SkillList, SkillLoad }` with `fn name(self) -> &'static str` (snake_case names from the spec) and `fn definition(self) -> ToolDefinition`.
  - `pub fn core_tools_for(profile: ContextProfile) -> Vec<CoreTool>` — spec matrix: `Full`/`HumanAgent` all; `HumanChat` all except `CommentPost`; `Conversation` all except `CommentPost`; `KnowledgeCompaction` = `TicketGet, TicketComments, TicketRuns, KnowledgeSearch, SkillList, SkillLoad, ResultSubmit`.
  - `pub struct ToolCtx<'a> { pub state: &'a AppState, pub pool: &'a PgPool, pub scope: &'a RunToolScope }`
  - `pub enum ToolError { InvalidArgs(String), NotFound(String), Denied(String), Limit(String), Internal(anyhow::Error) }` with `fn message(&self) -> String` (Internal → `"internal error"`, details only in tracing).
  - `pub struct RunToolHost { state: Arc<AppState>, scope: RunToolScope }` implementing `ToolHost`: `list` = `core_tools_for(scope.profile)` definitions; `call` = reject names not in the list (`denied`), run handler under `tokio::time::timeout(call_timeout_secs)`, serialize `Ok(Value)` as pretty JSON text, truncate text over `max_output_bytes` with suffix `"\n…[truncated: use limit/before to page]"`, insert one `run_tool_calls` row (`source = 'core'` or `'skill'` for skill tools; `args_summary` = compact args JSON cut to 200 chars; status ok/error/denied/timeout).
  - `pub async fn call_board_agents(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError>` → `{agents: [{key, name, role}]}` for enabled agents, key = `preset_source` or `slugify(name)`.
  - `server::routes() -> Router<Arc<AppState>>`: `POST /mcp` — missing/invalid bearer → 401 with `WWW-Authenticate: Bearer`; valid → `handle_rpc(body, &RunToolHost)`; `Some(v)` → 200 JSON, `None` → 202 empty. `GET /mcp`, `DELETE /mcp` → 405.

- [ ] **Step 1: Write failing tests** — unit: `catalog_matrix_matches_spec` (asserts each profile's tool names exactly, e.g. `HumanChat` lacks `comment_post`, `KnowledgeCompaction` lacks `ticket_search`/`board_agents`). Integration (`integration_mcp.rs`, using `spawn_test_server` and `reqwest` POSTs):

```rust
#[tokio::test] async fn mcp_requires_bearer()                   // no header → 401; wrong token → 401
#[tokio::test] async fn mcp_get_is_405()
#[tokio::test] async fn mcp_tools_list_is_profile_scoped()      // HumanChat token → names lack "comment_post"
#[tokio::test] async fn mcp_board_agents_lists_enabled_agents() // tools/call board_agents → text JSON contains preset key "backend_engineer"; one run_tool_calls row status 'ok'
#[tokio::test] async fn mcp_denied_tool_is_logged()             // HumanChat calls comment_post → isError true; run_tool_calls status 'denied'
```

Add a test helper `mint_test_token(state, run_id, profile) -> String` in `tests/common/mod.rs`.

- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_mcp mcp_` and the catalog unit test — Expected: FAIL.

- [ ] **Step 3: Implement** catalog, host, server routes, `board_agents`; handler stubs for other tools return `ToolError::Internal(anyhow!("not implemented"))` until their tasks.

- [ ] **Step 4: Run** — Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "Serve the Coppice MCP gateway with profile-scoped tools."`

---

### Task 4: `McpAccess` plumbing, token lifecycle in all run paths, mock tool calls

**Files:**
- Create: `server/src/mcp/grant.rs`
- Modify: `server/src/providers/mod.rs` (add `pub mcp: Option<McpAccess>` to `AgentRunInput`), `server/src/providers/mock.rs`, `server/src/workers/job_worker.rs` (`execute_job`, `invoke_chat_provider`/`execute_chat_turn`), `server/src/workers/job_worker/compaction.rs`, every other `AgentRunInput { .. }` literal (tests included)

**Interfaces:**
- Consumes: `TokenService`, `NewRunToolScope` (Task 2); gateway (Task 3).
- Produces:
  - `#[derive(Clone)] pub struct McpAccess { pub url: String, pub token: String }` with `fn env(&self) -> [(&'static str, String); 2]` → `COPPICE_MCP_URL`, `COPPICE_MCP_TOKEN`.
  - `pub struct RunToolGrant { pub access: McpAccess, run_id: Uuid, pool: PgPool, revoked: bool }`; `pub async fn grant_for_run(state: &AppState, pool: &PgPool, scope: NewRunToolScope) -> anyhow::Result<RunToolGrant>`; `pub async fn revoke(mut self)`; `impl Drop` → if not revoked, `tokio::spawn` a `revoke_for_run`.
  - Mock fixture extension: optional top-level `"toolCalls": [{"tool": "...", "args": {...}}]`. `MockProvider::run` strips it before deserializing `AgentRunResult`, and if `input.mcp` is `Some`, performs `initialize` then `tools/call` for each entry in order via `reqwest` with the bearer token; a transport failure → `ProviderError::InvalidInput("mcp_unavailable: …")`. Each call's text result is appended to the mock stdout sidecar when `MOCK_AGENT_STDOUT=1`. Optional `"delayMsAfterToolCalls": <u64>` makes the mock wait that long after the tool calls while watching `cancel_rx`, returning `ProviderError::Cancelled` if cancelled (used by Task 8).

- [ ] **Step 1: Write failing integration tests** (`integration_mcp.rs`, using `bootstrap_and_login_with_workers` + a temp fixtures dir via `MOCK_AGENT_RESPONSE` pointing at a fixture written by the test):

```rust
#[tokio::test] async fn mock_run_executes_tool_calls_through_gateway() // fixture toolCalls [board_agents] + done → run succeeded; run_tool_calls has 1 row for the run, status 'ok'
#[tokio::test] async fn token_revoked_after_run_finishes()           // after success, verify(token captured from run_tool_tokens by run_id is revoked_at NOT NULL)
#[tokio::test] async fn chat_turn_and_compaction_get_scoped_tokens() // chat turn run → run_tool_tokens row with context_profile 'conversation', chat_session_id set; compaction run → 'knowledge_compaction' with compaction_ticket_ids non-empty
```

- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_mcp -- mock_run_ token_revoked chat_turn_and` — Expected: FAIL.

- [ ] **Step 3: Implement** `grant.rs`; in each run path mint right after `mark_running` (ticket runs: `ticket_id`, `board_id = ticket.board_id`; chat: `chat_session_id`, `board_id = session.board_id`; compaction: `compaction_ticket_ids` = the batch's ticket ids), pass `mcp: Some(grant.access.clone())`, and call `grant.revoke().await` on every return path after the connector returns (success, error, cancel). Update all `AgentRunInput` literals with `mcp: None` where no run exists (tests, model probes).

- [ ] **Step 4: Run** — Expected: PASS. Also run `cargo test -p coppice-server --lib providers` — Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "Mint per-run MCP tokens in all run paths and let mock fixtures call tools."`

---

### Task 5: Ticket read tools

**Files:**
- Create: `server/src/mcp/tools/tickets.rs`
- Modify: `server/src/mcp/host.rs` (dispatch)

**Interfaces:**
- Consumes: `ToolCtx`, `ToolError` (Task 3); `TicketService::get`, `CommentService::list_by_ticket`, `RunService::list_for_ticket`, existing `build_ticket_json` shape (move `build_ticket_json`/`build_comments_json`/`build_runs_json` from `job_worker.rs` into `tickets.rs` as `pub(crate)` and reuse).
- Produces:
  - `fn resolve_ticket(ctx, args) -> Result<Uuid, ToolError>` — `args.ticketId` or `scope.ticket_id`; neither → `InvalidArgs("ticketId is required outside a ticket run")`; ticket on another board than `scope.board_id` (when `board_id` is Some) → `NotFound("ticket not on this board")`; for `KnowledgeCompaction`, ticket not in `compaction_ticket_ids` → `Denied`.
  - `call_ticket_get` → ticket JSON (title, description, acceptanceCriteria, status, substatus, assignee key, repo, branch).
  - `call_ticket_comments` args `{ticketId?, limit? (default 20, max 50), before? (comment id)}` → `{comments: [...newest first], nextBefore: id|null}`.
  - `call_ticket_runs` args `{ticketId?, limit? (default 10, max 30)}` → `{runs: [...]}`.
  - `call_ticket_search` args `{query?, status?, limit? (default 20, max 50)}` within `scope.board_id` (all boards when None) → `{tickets: [{id, title, status, assignee}]}`; case-insensitive `ILIKE` on title/description; excludes archived tickets.

- [ ] **Step 1: Write failing integration tests**

```rust
#[tokio::test] async fn ticket_get_defaults_to_run_ticket()
#[tokio::test] async fn ticket_get_other_board_is_not_found()        // Review Focus 3
#[tokio::test] async fn ticket_comments_pages_large_threads()        // 500 comments → 20 returned, nextBefore set; limit 1000 → 50 returned (Review Focus 4)
#[tokio::test] async fn chat_ticket_tool_without_ticket_id_errors()  // Conversation token, ticket_get {} → isError true, text contains "ticketId is required" (Review Focus 5)
#[tokio::test] async fn compaction_token_limited_to_batch_tickets()
#[tokio::test] async fn ticket_search_matches_title_on_board()
```

- [ ] **Step 2: Run** `... --test integration_mcp ticket_` — Expected: FAIL.
- [ ] **Step 3: Implement** the handlers per interfaces.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "Add ticket read tools to the MCP gateway."`

---

### Task 6: `knowledge_search` and `comment_post`

**Files:**
- Create: `server/src/mcp/tools/knowledge.rs`, `server/src/mcp/tools/comments.rs`

**Interfaces:**
- Consumes: `knowledge::retrieval::retrieve`, `has_eligible`, `context_budget::{render_knowledge, record_usage}`, `CommentService::create`.
- Produces:
  - `call_knowledge_search` args `{query: string (required, non-empty), limit? (default 5, max 20)}`; if `state.config.knowledge.enabled` is false or no board → `{items: []}`; else `retrieve(pool, board_id, agent_id, query, "", &retrieval_cfg)` → top `limit` → `{items: [{id, revisionId, title, type, content}]}` and records usage for revisions not already logged for this run (query `knowledge_usage_logs` by `run_id` first).
  - `call_comment_post` args `{body: string (required)}` → creates `AuthorType::Agent` comment by `scope.agent_id` on `scope.ticket_id` with `CommentIntent` = the existing progress/default intent used for agent comments; publishes `AppEvent::CommentCreated`; more than `comment_post_limit` successful posts for the run (count `run_tool_calls` rows `tool='comment_post' AND status='ok'`) → `Limit("comment_post limit reached for this run")`. No `ticket_id` in scope → `Denied`.

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test] async fn knowledge_search_returns_only_approved_and_logs_once() // approved + pending items; search twice → only approved returned; knowledge_usage_logs rows for run == 1 per revision
#[tokio::test] async fn comment_post_creates_agent_comment()
#[tokio::test] async fn comment_post_capped_per_run()                          // 6th call → isError, text contains "limit reached"
#[tokio::test] async fn comment_post_cannot_target_other_ticket()              // args with extra ticketId are ignored; comment lands on scope ticket (Review Focus 3)
```

- [ ] **Step 2: Run** `... --test integration_mcp -- knowledge_search comment_post` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "Add knowledge_search and comment_post tools."`

---

### Task 7: `result_submit` and finish-time preference

**Files:**
- Create: `server/src/mcp/tools/result.rs`
- Modify: `server/src/services/result_contract.rs`, `server/src/providers/mod.rs` (`ProviderError::MissingResult(String)`), `claude_code.rs`, `codex.rs`, `cursor.rs`, `kilo_code.rs`, `sessions/opencode_client.rs` (return `MissingResult` instead of `InvalidFixture` when no JSON result is found), `server/src/workers/job_worker.rs`, `job_worker/compaction.rs`

**Interfaces:**
- Consumes: `apply_agent_result`, `apply_consultation_result`, `mention_service::resolve_agent_keys`, `MAX_MENTIONS_PER_RUN`.
- Produces:
  - `pub fn validate_for_profile(result: &AgentRunResult, profile: ContextProfile, job_type: &str) -> Result<(), String>` — `Full` + `respond_to_mention` → `apply_consultation_result`; other `Full`/`HumanAgent` → `apply_agent_result`; `HumanChat`/`Conversation` → reject `continued`, reject non-empty `splitTickets`/`updatedDescription`/`assignTo`; `KnowledgeCompaction` → only `done` accepted.
  - `call_result_submit` args = the `AgentRunResult` JSON object. Deserialize error → `InvalidArgs(serde message)`; `validate_for_profile` error → `InvalidArgs`. On success: `UPDATE agent_runs SET submitted_result = $1 WHERE id = $2 AND status = 'running'` and return `{accepted: true, warnings: [...]}` where warnings list each `assignTo`/`mentionAgents`/`agentRequests[].agentKey` that is unknown or disabled (not in `resolve_agent_keys`), equals the run's own agent, is duplicated, or exceeds `MAX_MENTIONS_PER_RUN` combined.
  - `pub async fn take_submitted_result(pool: &PgPool, run_id: Uuid) -> anyhow::Result<Option<AgentRunResult>>` in `result.rs`.
  - `job_worker` rule at finish (ticket, chat, compaction paths): if the provider returned `Ok(r)` or `Err(MissingResult)`, use `take_submitted_result` when `Some`, else `Ok(r)`; `Err(MissingResult)` with no submission → existing failure message. Cancel/other errors ignore the submission.

- [ ] **Step 1: Write failing tests** — unit in `result_contract.rs`: `validate_for_profile_rejects_continued_in_chat`, `validate_for_profile_compaction_requires_done`, `validate_for_profile_consultation_uses_consultation_rules`. Integration:

```rust
#[tokio::test] async fn result_submit_invalid_returns_errors()
#[tokio::test] async fn result_submit_warns_on_unknown_targets()        // assignTo "nobody" → accepted true, warnings contains "nobody"
#[tokio::test] async fn invalid_resubmission_keeps_previous_valid()    // valid then invalid → submitted_result still the valid one (Review Focus 2)
#[tokio::test] async fn submitted_result_wins_over_final_json()        // fixture toolCalls submit summary "via tool"; fixture final JSON summary "via stdout" → agent comment body contains "via tool"
#[tokio::test] async fn final_json_fallback_without_submission()       // no toolCalls → behaves as today
```

- [ ] **Step 2: Run** `cargo test -p coppice-server --lib result_contract::` and `... --test integration_mcp result_` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS; also `... --test integration_agent_runs` — Expected: PASS (no regression).
- [ ] **Step 5: Commit** — `git commit -m "Add result_submit with finish-time preference over final JSON."`

---

### Task 8: Stop/cancel hardening

**Files:**
- Modify: `server/src/workers/job_worker.rs`, `server/src/services/run_service.rs` (cancel path clears `submitted_result`)

**Interfaces:**
- Consumes: `RunToolGrant::revoke`, `TokenService::revoke_for_run`.
- Produces: `RunService::stop` and `RunService::finish_failed` execute `revoke_for_run(run_id)` and `UPDATE agent_runs SET submitted_result = NULL`.

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test] async fn stopped_run_revokes_token_and_discards_submission() // Review Focus 1: fixture has toolCalls [result_submit] and "delayMsAfterToolCalls": 10000; once submitted_result is non-null the test stops the run through the existing stop endpoint; afterwards gateway call with that token → 401; ticket has no agent result comment; submitted_result IS NULL
```

- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "Revoke MCP tokens and discard submissions when runs stop."`

---

### Task 9: Built-in `coppice` skills and skill tools

**Files:**
- Create: `server/builtin-plugins/coppice/skills/{coppice-collaboration,coppice-splitting,coppice-pm-refinement,coppice-tech-lead-review,coppice-qc-verification,coppice-git}/SKILL.md`
- Create: `server/src/plugins/mod.rs`, `server/src/plugins/builtin.rs`, `server/src/plugins/skills.rs`, `server/src/mcp/tools/skills.rs`
- Modify: `server/src/lib.rs` (`pub mod plugins;`), server startup (materialize built-ins), `AppState` (add `skills: Arc<SkillCatalog>`)

**Interfaces:**
- Produces:
  - Each `SKILL.md` has frontmatter `name`, `description` (one line) and a body containing the prose moved **verbatim** from `context_builder.rs`: collaboration fields block (lines under "Coppice platform rules — collaboration fields"), split/continued guidance, PM/Tech Lead/QC branches of `format_contract_guidance`, `format_git_rules` + `format_verification_guidance`.
  - `pub fn materialize_builtin(dir: &Path) -> std::io::Result<()>` — writes the `include_str!`-embedded files to `<dir>/coppice/skills/<name>/SKILL.md`, overwriting.
  - `pub struct SkillInfo { pub id: String /* "coppice-git" */, pub description: String, pub path: PathBuf }`
  - `pub struct SkillCatalog`; `pub fn load_builtin(dir: &Path) -> anyhow::Result<SkillCatalog>`; `pub fn skills_for(&self, agent_id: Uuid) -> Vec<SkillInfo>` (Part 1: built-ins for every agent; Part 2 adds agent plugins); `pub fn get(&self, agent_id: Uuid, id: &str) -> Option<(SkillInfo, String /*body*/)>`.
  - `call_skill_list` → `{skills: [{id, description}]}`; `call_skill_load` args `{name}` → `{id, path, body}`; unknown → `NotFound`. `host.rs` logs these with `source = 'skill'`.
  - `pub fn required_skill(profile: ContextProfile, job_type: &str, ticket_status: Option<TicketStatus>, agent_key: &str, agent_role: &str) -> Option<&'static str>` in `plugins/skills.rs`, reusing the existing predicates (`is_pm_agent`, `is_ready_tech_lead_task`, `is_in_review_review_task`, `is_in_qa_qc_task` — move them to a shared `pub(crate)` location if needed).

- [ ] **Step 1: Write failing tests** — unit: `builtin_skills_parse_frontmatter` (all six present, non-empty description), `required_skill_for_qc_in_qa_is_qc_verification`, `required_skill_none_for_chat`. Integration: `skill_load_returns_body_and_logs_skill_source` (run_tool_calls row source 'skill', tool 'skill_load').
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "Ship built-in coppice skills with skill_list and skill_load tools."`

---

### Task 10: Slim context builders and new run prompt

**Files:**
- Modify: `server/src/services/context_builder.rs`, `server/src/services/context_budget.rs`, `server/src/sessions/opencode_events.rs` (`COPPICE_RUN_PROMPT`), `server/src/workers/job_worker.rs`, `job_worker/compaction.rs`, `server/src/knowledge/compaction_context.rs`

**Interfaces:**
- Consumes: `SkillCatalog::skills_for`, `required_skill` (Task 9).
- Produces:
  - `pub fn build_tool_first_context(input: &ContextInput<'_>, skills: &[SkillInfo], required_skill: Option<&str>) -> String` — sections in order: `# Agent` (name, role, system prompt), `# Task` (job type label; human request block if any; ticket title/status/substatus/assignee, or "Agent Chat" transcript tail for conversation — keep the existing latest-human-message behaviour), `# Repository` (name, worktree path, one-line git rule: "Commit before finishing; Coppice syncs and auto-commits — see skill coppice-git"), `# Skills` (`- id — description` list; "Load `<required>` before starting." when set), `# Coppice tools` (fixed lines: "Use `ticket_get` / `ticket_comments` / `ticket_runs` for ticket details.", "Use `knowledge_search` for approved project knowledge.", "Use `board_agents` to find agent keys for handoff.", "Finish by calling `result_submit` with your result; fix and resubmit if it returns errors.").
  - Compaction keeps its batch listing (the batch is the task) but drops the embedded JSON contract in favour of the `result_submit` line.
  - `COPPICE_RUN_PROMPT` = `"Read .agent/context.md and complete the task described there. When finished, call the coppice result_submit tool with your result."`
  - `pub const LEGACY_FULL_CONTEXT_BYTES: usize` in a test module = the byte length of `build_context_md` on the fixture below, measured once in Step 1 and hard-coded.

- [ ] **Step 1: Record the baseline** — add `fn fixture_full_input()` in `context_builder` tests (reuse `full_profile_defaults()` with a 2 KB description, 10 comments in `latest_comments`, 1 KB project rules, PM agent) and a test that prints `build_context_md(&input).len()`; run `cargo test -p coppice-server --lib context_builder::tests::print_legacy_baseline -- --nocapture`; hard-code the printed number as `LEGACY_FULL_CONTEXT_BYTES`.
- [ ] **Step 2: Write failing tests**

```rust
#[test] fn tool_first_full_context_is_at_least_half_smaller() // build_tool_first_context(&fixture_full_input(), &skills, None).len() * 2 <= LEGACY_FULL_CONTEXT_BYTES
#[test] fn tool_first_context_lists_skills_and_required_skill() // contains "coppice-qc-verification — " and "Load `coppice-qc-verification` before starting."
#[test] fn tool_first_context_has_no_json_contract()            // !contains("```json") && !contains(".agent/ticket.json")
#[test] fn tool_first_context_mentions_result_submit()
```

- [ ] **Step 3: Run** `cargo test -p coppice-server --lib context_builder::` — Expected: FAIL.
- [ ] **Step 4: Implement** `build_tool_first_context`; switch `job_worker` (all profiles), chat, and compaction to it; stop writing `.agent/{ticket,comments,runs}.json`; stop pre-injecting knowledge (remove the `retrieve`/`render_knowledge` block from `execute_job` — knowledge now arrives via `knowledge_search`); update the prompt constant.
- [ ] **Step 5: Run** the unit tests, then `... --test integration_agent_runs --test integration_workflow --test integration_chat --test integration_knowledge --test integration_knowledge_compaction --test integration_agent_mentions --test integration_splits` — Expected: PASS after updating assertions that inspected old context text (assert on tool-first sections instead; do not weaken workflow assertions).
- [ ] **Step 6: Commit** — `git commit -m "Switch all runs to the slim tool-first context."`

---

### Task 11: Wire real connectors to the gateway; delete legacy context code

**Files:**
- Modify: `server/src/providers/{claude_code,codex,cursor,kilo_code,opencode}.rs` (+ `sessions/opencode_*` if OpenCode moves to per-run processes per Task 1), `server/src/providers/mod.rs` (`CHAT_READ_ONLY_TOOLS` adds `mcp__coppice__*` if Task 1 confirmed the syntax), `server/src/services/context_builder.rs` (delete legacy builders), `docs/providers/README.md`, `docs/architecture.md`, `AGENTS.md`

**Interfaces:**
- Consumes: `McpAccess` (Task 4); verified table (Task 1).
- Produces, per connector, a pure helper tested without the CLI:
  - `fn claude_mcp_args(access: &McpAccess, run_dir: &Path) -> std::io::Result<Vec<String>>` — writes `<run_dir>/mcp.json` `{"mcpServers":{"coppice":{"type":"http","url":…,"headers":{"Authorization":"Bearer ${COPPICE_MCP_TOKEN}"}}}}` and returns `["--mcp-config", path, "--strict-mcp-config"]`.
  - `fn codex_mcp_args(access: &McpAccess) -> Vec<String>` — the `-c` overrides from the verified table.
  - `cursor`, `kilo-code`, `opencode`: the verified mechanism as a pure `fn …_mcp_setup(access, run_dir) -> …` returning args/env; if Task 1 decided a connector cannot be tool-first, it returns `Err(ProviderError::InvalidInput("mcp_unavailable: <connector> has no per-run MCP configuration"))` and the run fails loudly.
  - Every connector sets `cmd.envs(access.env())`; `run_dir` = `<artifacts_dir>/runs/<run_id>/`.
  - Legacy removal: delete `build_context_md` / `build_full_context` / `format_full_output_contract` / `format_contract_guidance` / `format_on_demand_section` / `write_agent_context_files` and their tests; keep `LEGACY_FULL_CONTEXT_BYTES` as a plain constant with its measurement comment.

- [ ] **Step 1: Write failing unit tests** in each provider file: `claude_mcp_args_write_run_file_outside_worktree` (file under run_dir, JSON contains `"coppice"` and no plaintext token), `codex_mcp_args_use_env_bearer`, and one per remaining connector for its verified mechanism (or its `mcp_unavailable` error).
- [ ] **Step 2: Run** `cargo test -p coppice-server --lib providers::` — Expected: FAIL.
- [ ] **Step 3: Implement** wiring and legacy deletion; update docs (`docs/providers/README.md` per-connector MCP notes; `docs/architecture.md` gateway section; `AGENTS.md` point 9 mentions `/mcp` and tool-first runs).
- [ ] **Step 4: Run** `cargo clippy --workspace -- -D warnings` then `make test` — Expected: PASS. Then `make web-test` — Expected: PASS.
- [ ] **Step 5: Run existing smokes** — `make e2e-smoke-m03 && make e2e-smoke-m06 && make e2e-smoke-m06-knowledge && make e2e-smoke-m09` — Expected: all PASS on the default Compose stack.
- [ ] **Step 6: Manual acceptance** — with managed connectors in Compose, one ticket per tool-first connector: run_tool_calls shows `ticket_get` and `result_submit`; ticket ends in the expected column. Record outcomes in the PR description.
- [ ] **Step 7: Commit** — `git commit -m "Wire connectors to the Coppice MCP gateway and remove the legacy fat context."` Then `make clean`.
