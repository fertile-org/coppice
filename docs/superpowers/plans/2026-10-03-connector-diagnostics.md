# Connector Diagnostics (Tools → Connectors) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An admin-only Tools → Connectors tab that shows, per connector, whether it is enabled, installed, and authenticated, and proves it end to end with a real "Test connection" check run through the Coppice MCP gateway.

**Architecture:** Probe logic moves from the CLI into `coppice_connectors::probe` (std only), shared by `coppice connector doctor/list` and the server. The server caches probe results, exposes them under `/api/tools/connectors`, and runs check runs: real agent runs owned by a `connector_checks` row (like compaction runs owned by a batch) with a two-tool `connector_check` profile. The web Tools page gains Backup and Connectors tabs.

**Tech Stack:** Rust (Axum, SQLx, tokio), React + TanStack Query + zod, Vitest, embedded Postgres tests.

**Spec:** `docs/superpowers/specs/2026-10-03-connector-diagnostics-design.md`

## Global Constraints

- Responses and logs carry env variable **names** only, never values; auth paths are names relative to HOME, never contents; the MCP token never appears in any response or log.
- `coppice_connectors` gains no dependencies beyond std (serde stays).
- Probe output caps: `Ok` first line ≤ 200 chars; `Failed` message ≤ 500 chars; check `failure` ≤ 500 chars.
- Probe timeout 10 s; check run timeout `min(connector run timeout, 180 s)`.
- `COMMON_BIN_DIRS` (relative ones under HOME): `.local/bin`, `.opencode/bin`, `.npm-global/bin`, `.bun/bin`; absolute: `/opt/homebrew/bin`, `/usr/local/bin`.
- Probe args: cursor `["models"]`; claude-code, codex, kilo-code `["--version"]`; opencode `["auth","list"]`. `probe_proves_auth`: cursor, opencode.
- All `/api/tools/connectors*` and `/api/tools/connector-checks/*` routes are admin-only (non-admin 403, unauthenticated 401); POSTs require `X-CSRF-Token`. `mock` and unknown ids → 404.
- Check runs never touch tickets, comments, workflow, notifications, knowledge, or repositories.
- CLI `doctor`/`list` output text and exit codes are unchanged.
- Migration number: `032_connector_checks.sql`.
- Verification during work: targeted tests only; `make test` once in Task 6. Never `docker compose down -v`; default compose stack only.

## Review Focus

1. **A probe that hangs** (e.g. `agent models` waiting on network or a login prompt) must be killed at 10 s and reported `timed_out`, not freeze Run check — test in Task 1.
2. **A "binary" that is a directory or a non-executable file** on PATH must count as missing — test in Task 1.
3. **Probe output echoing an auth env value** (e.g. a CLI printing the API key it found) must have that value replaced with `[redacted]` before it is stored or returned — test in Task 1.
4. **Server restart during a check** must not leave a check `queued`/`running` forever (which would 409 every later Test) — startup marks them `failed` with `server restarted`; test in Task 4.
5. **Agent deleted, connector disabled, or provider error mid-check** must end the check `failed` with a reason, never stuck `running` — the executor finalizes the check on every exit path; test in Task 4.

---

### Task 1: Probe library in `coppice_connectors`

**Files:**
- Create: `connectors/src/probe.rs`
- Modify: `connectors/src/lib.rs` (`pub mod probe;`, `InstallInfo` fields, descriptor values)

**Interfaces:**
- Produces:
  - `InstallInfo` gains `probe_args: &'static [&'static str]`, `probe_proves_auth: bool`, `docs_url: &'static str` (vendor install docs; mock `""`).
  - `pub const COMMON_BIN_DIRS_HOME: &[&str]`, `pub const COMMON_BIN_DIRS_ABS: &[&str]`.
  - `pub fn augment_path(home: &Path, current: &OsStr) -> OsString` — prepends existing dirs not already present, order as listed (home ones first).
  - `pub fn resolve_binary(name: &str, path: &OsStr) -> Option<PathBuf>` — a name containing `/` is checked as given; bare names searched in `path`; only regular files with an exec bit (unix) count.
  - `pub struct ProbeEnv<'a> { pub home: &'a Path, pub path: &'a OsStr, pub command_override: Option<&'a str>, pub env_lookup: &'a dyn Fn(&str) -> Option<String> }`
  - `pub enum ProbeOutcome { NotRun, Ok { first_line: String }, Failed { message: String }, TimedOut }`
  - `pub struct ProbeReport { pub binary: Option<PathBuf>, pub auth_env_set: Vec<&'static str>, pub auth_paths_found: Vec<&'static str>, pub probe: ProbeOutcome, pub probe_proves_auth: bool }` with `pub fn auth_ok(&self) -> bool` (env set, path found, or proves-auth and probe `Ok`).
  - `pub fn probe(d: &ConnectorDescriptor, env: &ProbeEnv, timeout: Duration) -> ProbeReport`.
  - `pub fn auth_path_present(path: &Path) -> bool` (moved from CLI `path_looks_like_auth`, same behavior: non-empty file or non-empty dir).

- [ ] **Step 1: Write failing tests** in `connectors/src/probe.rs` `#[cfg(test)]` (use `std::env::temp_dir()` + unique subdir; write shell scripts with `#!/bin/sh`, chmod 755):
  - `resolve_binary_finds_executable_on_path`, `resolve_binary_rejects_directory_and_non_executable`, `resolve_binary_uses_path_with_slash_as_given`.
  - `augment_path_adds_existing_dirs_once` — temp HOME with `.local/bin` existing, `.bun/bin` absent: result starts with `<home>/.local/bin`, has no `.bun/bin`; calling again on the result adds nothing.
  - `probe_ok_reports_first_line` — script prints `claude 2.1.0\nmore` → `Ok { first_line: "claude 2.1.0" }`.
  - `probe_failed_reports_stderr_capped` — script writes 2000 chars to stderr, exits 1 → `Failed`, message len ≤ 500.
  - `probe_times_out` — script `sleep 30`, timeout 300 ms → `TimedOut`, returns in < 5 s.
  - `probe_missing_binary_not_run` → `binary: None`, `NotRun`.
  - `probe_redacts_auth_env_values` — `env_lookup` returns `sk-secret-123456` for `ANTHROPIC_API_KEY`; script echoes it; outcome text contains `[redacted]`, not the value.
  - `auth_detects_env_names_and_paths` — env set + temp HOME with a non-empty `auth_paths` file → names listed; empty file not listed.
  - `descriptors_have_probe_args` — every non-mock descriptor has non-empty `probe_args` and a `docs_url` starting with `https://`; cursor/opencode `probe_proves_auth`.
- [ ] **Step 2: Run** `cargo test -p coppice-connectors` → FAIL (module missing).
- [ ] **Step 3: Implement** `probe.rs` and the descriptor fields/values (Global Constraints). Spawn with `std::process::Command`, `env_clear()` not used — set `PATH` and `HOME` explicitly, stdin null, stdout/stderr piped; poll `try_wait` every 50 ms until timeout, then `kill` + `wait`. Read pipes on threads to avoid deadlock. Lossy-decode, redact each non-empty auth env value, then cap.
- [ ] **Step 4: Run** `cargo test -p coppice-connectors` → PASS; `cargo clippy -p coppice-connectors -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(connectors): shared probe library (binary, auth, probe command)`

### Task 2: CLI uses the probe library

**Files:**
- Modify: `cli/src/commands/connector/{registry,doctor,list}.rs`, `config/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 `probe`, `resolve_binary`, `auth_path_present`, `augment_path`.
- Produces: `AgentConnectorsConfig::command(&self, id: &str) -> Option<&str>` (config `command` for opencode, kilo-code, cursor; `None` otherwise) in `config/src/lib.rs`.

- [ ] **Step 1: Write failing test** `command_for_connectors` in `config/src/lib.rs` tests: default config → `command("kilo-code") == Some("kilo")`, `command("cursor") == Some("cursor-agent")`, `command("claude-code") == None`.
- [ ] **Step 2: Run** `cargo test -p coppice-config command_for` → FAIL.
- [ ] **Step 3: Implement** `command`; replace CLI `auth_present`/`binary_on_path`/`probe_models`/`probe_proves_auth` with calls to the library (`doctor` builds `ProbeEnv` from `home_dir()`, `augment_path(home, $PATH)`, `config.agent.connectors.command(id)`, `std::env::var`). Keep every printed line and exit code identical; delete the moved helpers and move their tests to Task 1's module if not already covered. Drop `which` from `cli/Cargo.toml` if unused.
- [ ] **Step 4: Run** `cargo test -p coppice-cli` and `cargo test -p coppice-config` → PASS; `cargo clippy --workspace -- -D warnings` clean.
- [ ] **Step 5: Commit** `refactor(cli): connector doctor/list use coppice_connectors::probe`

### Task 3: Server probes, PATH, and `GET/POST /api/tools/connectors`

**Files:**
- Create: `server/src/services/connector_probe_service.rs`, `server/src/api/tool_connectors.rs`, `server/tests/integration_connector_diagnostics.rs`
- Modify: `server/src/lib.rs` (`AppState.connector_probes`), `server/src/main.rs` (PATH augment before anything spawns; startup probe task), `server/src/api/mod.rs` (routes), `server/src/services/mod.rs`, `server/tests/common/mod.rs` (construct the new field)

**Interfaces:**
- Consumes: Task 1, Task 2 `command(id)`.
- Produces:
  - `pub struct ConnectorProbes` (in-memory, `RwLock<HashMap<String, CachedProbe>>`); `CachedProbe { report: ProbeReport, probed_at: DateTime<Utc> }`; `ConnectorProbes::refresh(&self, config: &AppConfig, id: &str) -> Option<CachedProbe>` (runs `probe` in `spawn_blocking`, 10 s); `refresh_all`; `get(id)`.
  - `AppState.connector_probes: Arc<ConnectorProbes>`.
  - `pub async fn last_real_runs(pool) -> HashMap<String, LastRun>` — latest finished run with `connector_check_id IS NULL` per `agents.connector`, with `ticket_get`/`result_submit` = exists `ok` call. (`connector_check_id` column arrives in Task 4; until then omit that predicate and add it in Task 4.)
  - JSON (camelCase) per spec API table: `ConnectorStatusResponse { id, displayName, enabled, cli: { found, path, probe: { status: "ok"|"failed"|"timed_out"|"not_run", detail } }, auth: { status: "detected"|"not_found"|"verified_by_probe", envSet, pathsFound }, authHint, docsUrl, lastRun, lastCheck, probedAt }`. `lastCheck` is `null` until Task 4.
  - Routes: `GET /api/tools/connectors`, `POST /api/tools/connectors/{id}/check`.

- [ ] **Step 1: Write failing tests** in `integration_connector_diagnostics.rs` (config `command` for kilo-code pointing at a temp script printing `kilo 9.9.9`, kilo-code enabled):
  - `connectors_lists_non_mock_descriptors` — admin GET → ids equal all descriptors minus `mock`; kilo-code `enabled: true`, `cli.found: true`, `cli.probe.status == "ok"`, `cli.probe.detail == "kilo 9.9.9"`.
  - `connectors_requires_admin` — member 403, no session 401.
  - `check_reprobes_one_connector` — rewrite script to print `kilo 10.0.0`, POST check → detail `kilo 10.0.0`; GET reflects it.
  - `check_requires_csrf`, `check_unknown_or_mock_404`.
  - `connectors_never_return_env_values` — set an auth env var for a descriptor in the test process to a sentinel; response body contains the name, not the sentinel.
  - `last_run_reports_gateway_calls` — run `fixtures/agent-responses/mcp/ticket_submit_result.json` with a `mock` agent (run helpers in `integration_mcp.rs`), then call `last_real_runs(pool)` directly (the API hides `mock`): entry `"mock"` has `ticket_get == true && result_submit == true`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_connector_diagnostics` → FAIL.
- [ ] **Step 3: Implement.** `main.rs`: `std::env::set_var("PATH", augment_path(home, current))` as the first statement of `main`, before config load, logging, or any spawned task (nothing reads env concurrently yet); spawn `refresh_all` after state build. Handlers thin; admin check as in `api/tools.rs`.
- [ ] **Step 4: Run** the test file → PASS; `cargo test -p coppice-server --features embedded-test-db --lib` PASS; clippy clean.
- [ ] **Step 5: Commit** `feat(tools): connector probe status API`

### Task 4: Check runs (profile, data, executor, API)

**Files:**
- Create: `server/migrations/032_connector_checks.sql`, `server/src/services/connector_check_service.rs`, `server/src/workers/job_worker/connector_check.rs`, `server/src/domain/connector_check.rs`, `fixtures/agent-responses/mcp/connector_check.json`, `fixtures/agent-responses/mcp/connector_check_skip_ticket.json`
- Modify: `server/src/domain/context_profile.rs` (`ConnectorCheck`, `"connector_check"`), `server/src/mcp/catalog.rs` (`ConnectorCheck => vec![TicketGet, ResultSubmit]`), `server/src/mcp/tools/tickets.rs` (synthetic ticket), `server/src/services/result_contract.rs` (Done only), `server/src/services/context_builder.rs` (label + `connector_check_context`), `server/src/mcp/proxy/source.rs` and `server/src/plugins/skills.rs` (no plugins/skills for the profile), `server/src/mcp/registry.rs` and `server/src/workers/job_worker.rs` (exhaustive match arms; dispatch `JOB_TYPE_CONNECTOR_CHECK`), `server/src/services/run_service.rs` (`create_connector_check_run`), `server/src/api/tool_connectors.rs` (test + check detail routes, `lastCheck`), `server/src/main.rs` (stale check cleanup), Task 3's `last_real_runs` predicate
- Test: `server/tests/integration_connector_diagnostics.rs`, `server/src/mcp/catalog.rs` tests

**Interfaces:**
- Consumes: Task 3 routes/state; existing `RunService`, `TokenService`, `write_context_document`, `prefer_submitted_result`, connector registry (pattern: `job_worker/compaction.rs`).
- Produces:
  - Migration per spec "Data" (table, `agent_runs.connector_check_id`, owner check with four columns, unique index, profile check adds `'connector_check'`).
  - `pub const JOB_TYPE_CONNECTOR_CHECK: &str = "connector_check";` and `enum CheckStatus { Queued, Running, Passed, Failed }` in `domain/connector_check.rs`.
  - `ConnectorCheckService::new(pool)`: `start(connector: &str, agent_id: Uuid, created_by: Uuid) -> Result<(Uuid, Uuid), CheckError>` (409 `ActiveCheck`, 400 `WrongConnector`/`Disabled`, 404 `AgentNotFound`); `mark_running(check_id)`; `finish(check_id, passed: bool, failure: Option<&str>)`; `get(check_id) -> CheckDetail`; `latest_by_connector() -> HashMap<String, CheckSummary>`; `fail_stale() -> u64` (sets `queued`/`running` → `failed`, `server restarted`).
  - `pub fn connector_check_context(agent_name: &str) -> String` containing exactly: `This is a Coppice connection check. Call the \`ticket_get\` tool, then call \`result_submit\` with outcome \`done\` and summary \`connection ok\`.`
  - Synthetic `ticket_get` for `ConnectorCheck`: `{ "title": "Coppice connection check", "description": "Submit a done result with summary 'connection ok'." }`, no DB read.
  - Routes: `POST /api/tools/connectors/{id}/test {agentId}` → `{ checkId, runId }`; `GET /api/tools/connector-checks/{id}` → spec detail incl. `toolCalls: [{tool, status}]`.
  - Pass rule and failure reasons exactly as spec Flow step 5: `"ticket_get was not called"`, `"result_submit was not called"`, `"result was <outcome>"`, else the run error message.

- [ ] **Step 1: Write failing tests:**
  - `catalog.rs`: `connector_check_profile_has_two_tools` → names `["ticket_get","result_submit"]`.
  - Integration. `ConnectorCheckService::start` accepts any connector id the config knows (`mock` is always enabled); only the HTTP route rejects `mock` with 404. So run-flow tests use a `mock` agent, call `start` directly, then drive the job worker with the same helper `integration_knowledge_compaction.rs` uses for compaction runs:
    - `check_run_passes_with_gateway_calls` — `MOCK_AGENT_RESPONSE`-equivalent fixture `mcp/connector_check` (`toolCalls`: `ticket_get {}`, `result_submit` done summary `connection ok`) → check `passed`, detail `toolCalls` has both `ok`; no rows added to `ticket_comments`, ticket count unchanged.
    - `check_run_fails_without_ticket_get` — fixture `mcp/connector_check_skip_ticket` → `failed`, failure `ticket_get was not called`.
    - `check_ticket_get_is_synthetic` — during the run the `ticket_get` call result title is `Coppice connection check` (assert via `run_tool_calls.status == ok` and the gateway response in a direct `/mcp` call with a minted `connector_check` token).
    - `check_provider_error_marks_failed` — agent whose connector is not configured → check `failed` with a non-empty failure, not `running`.
    - `test_route_validation` — POST for kilo-code with a mock-connector agent → 400; disabled connector → 400; second POST while one is `queued` → 409; missing CSRF → 403; non-admin → 403.
    - `stale_checks_failed_on_startup` — insert a `running` check, call `fail_stale()` → `failed`, failure `server restarted`.
    - `last_real_run_ignores_check_runs` — after a passing check, `last_real_runs` has no entry from it.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_connector_diagnostics` and `--lib catalog` → FAIL.
- [ ] **Step 3: Implement.** Executor mirrors `compaction.rs`: scratch dir `<artifacts_dir>/runs/<run id>/check/`, write context, `mark_running`, mint token (`profile ConnectorCheck`, `ticket_id/board_id None`, `plugin_ids []`, `job_type "connector_check"`), run provider with `tokio::time::timeout(min(run timeout,180s))`, `prefer_submitted_result`, finish run, compute pass, `finish` check, remove scratch dir. Wrap the body so **every** exit (error, timeout, cancel) calls `finish(check, false, reason)` before returning the error. `main.rs` calls `fail_stale()` once at startup before workers start.
- [ ] **Step 4: Run** the same commands → PASS; also `--test integration_mcp` and `--test integration_knowledge_compaction` (shared match arms) PASS; clippy clean.
- [ ] **Step 5: Commit** `feat(tools): connector check runs through the MCP gateway`

### Task 5: Web — Tools tabs and Connectors tab

**Files:**
- Create: `web/src/features/tools/BackupTab.tsx` (existing content moved from `ToolsPage.tsx`), `web/src/features/tools/ConnectorsTab.tsx`, `web/src/features/tools/ConnectorCard.tsx`, `web/src/features/tools/useConnectorDiagnostics.ts`, `web/src/lib/schemas/connectorDiagnostics.ts`, `web/src/features/tools/ConnectorsTab.test.tsx`
- Modify: `web/src/features/tools/ToolsPage.tsx` (tabs), `web/src/features/tools/ToolsPage.test.tsx`

**Interfaces:**
- Consumes: Task 3/4 API shapes.
- Produces: zod `connectorStatusSchema` (probe/auth status enums with `.catch` fallbacks `'not_run'` / `'not_found'`), `connectorCheckSchema`; hooks `useConnectorStatuses()` (key `['tool-connectors']`), `useRecheckConnector(id)`, `useStartConnectorTest(id)`, `useConnectorCheck(checkId)` (polls every 2 s while `queued|running`, stops otherwise; invalidates `['tool-connectors']` on finish).

- [ ] **Step 1: Write failing tests** (stub `fetch` with real `apiFetch` + `setCsrfToken`, as in `PluginCard.test.tsx`):
  - `ToolsPage.test.tsx`: `tools page has backup and connectors tabs` — default Backup selected; clicking Connectors sets `?tab=connectors` and shows the connectors panel; existing backup tests still pass.
  - `ConnectorsTab.test.tsx`:
    - `shows status per connector` — found + ok probe shows path and `kilo 9.9.9`; missing shows `Not installed`, the `authHint`, and a `docsUrl` link; auth `detected` lists env names; disabled shows `coppice connector enable kilo-code`.
    - `shows last real run` — `ticket_get ✓ · result_submit ✗`.
    - `run check posts with csrf and refreshes card`.
    - `test connection disabled without agents` — text `Create an agent with this connector first`.
    - `test connection polls to passed` — pick agent → POST body `{agentId}` → polling shows `Running…` then `Passed`.
    - `test connection shows failure reason` — `failed` + `ticket_get was not called`.
- [ ] **Step 2: Run** `cd web && yarn test src/features/tools` → FAIL.
- [ ] **Step 3: Implement.** Tabs with `role=tablist/tab/tabpanel`, linked ids, selected tab from `useSearchParams`. Agents for the picker come from the existing agents query filtered by `connector === id`. Follow `docs/web/DESIGN.md` tokens and the plugin card layout.
- [ ] **Step 4: Run** `yarn test src/features/tools` → PASS; `npx tsc -b --noEmit`; eslint on touched files; full `yarn test`.
- [ ] **Step 5: Commit** `feat(web): Tools → Connectors diagnostics tab`

### Task 6: Docs and final verification

**Files:**
- Modify: `docs/providers/README.md` (verification section: "Verify from Tools → Connectors → Test connection"; status table unchanged until manual acceptance), each `docs/providers/<connector>.md` "If something goes wrong" (point at the Connectors tab), `docs/architecture.md` (connector layer: probe library, PATH augmentation, check runs and the `connector_check` profile; add `connector_check` to the profile/tool matrix), `docs/milestones/M10-plugins.md` (Open note mentions the page), spec status line.

- [ ] **Step 1: Update docs** as listed.
- [ ] **Step 2: Verify** `cargo clippy --workspace -- -D warnings`, `make web-test`, `make test` → pass (a failure in `knowledge_query_plan_has_relational_indexes` or `ready_tech_lead_auto_handoff_queues_exactly_one_implementer_run` is a known flake: rerun and record).
- [ ] **Step 3: Verify stack** `make compose-up`, `make e2e-smoke`, `make e2e-smoke-m10` → pass (existing smokes unaffected). Record results.
- [ ] **Step 4: Commit** `docs: connector diagnostics`
- [ ] **Step 5 (manual, user):** Test connection for `claude-code` and `kilo-code` from the page; update the README status table with versions; fix kilo-code's config env var if the check fails with `mcp_unavailable`.
