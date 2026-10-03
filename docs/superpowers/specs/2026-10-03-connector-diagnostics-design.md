# Connector diagnostics (Tools → Connectors) — design

**Status:** implemented 2026-10-03 (branch `connector-diagnostics`); manual acceptance with real `claude-code` / `kilo-code` CLIs pending. Follow-up to [M10](../../milestones/M10-plugins.md): the UI tool used to record connector verification ("All six connectors run tool-first").

## Why

Coppice is moving to a desktop app installed from a `.dmg` / `.deb`. Desktop users will not open a terminal to run `coppice connector doctor`. Today the only real connector checks (binary on PATH, auth present, a probe command) live in the CLI binary crate, and the server's agent health only checks config. Admins need one page that answers "is this connector installed, logged in, and does it actually reach the Coppice MCP gateway?"

## Decisions

| Topic | Decision |
|---|---|
| Scope (v1) | Diagnostics only. Enable, install, login, and API-key storage stay in the CLI (kept for advanced use, not replaced). |
| Where CLIs live | The user's own install and login (their PATH, `~/.claude`, `~/.codex`, …). In Docker that is the managed `/home/coppice`. Coppice never installs a CLI or runs a login flow from the page; it shows the descriptor's hints and a vendor docs link. |
| End-to-end proof | Both: a "last real run" summary per connector from existing data, and a **Test connection** button that runs a real check run through the production path. |
| Location | New **Connectors** tab on the Tools page (admin-only), next to the existing Backup content. |
| Probe code | Moves out of `cli/` into a shared library used by both the CLI `doctor` and the server. CLI output is unchanged. |

## Page

Tools gets two tabs: **Backup** (the existing export/import, unchanged) and **Connectors**. Tabs use `role=tablist/tab/tabpanel`; the selected tab is kept in the URL (`/tools?tab=connectors`).

One card per descriptor in `coppice_connectors::all()` except `mock`. Each card shows:

- **Enabled** — from config (`AgentConnectorsConfig::enabled(id)`). Read-only; when disabled, the card shows the CLI command that enables it (`coppice connector enable <id>`) and that the server must restart.
- **CLI** — `found` with the resolved absolute path and the first line of the probe output (e.g. version), or `missing`.
- **Auth** — `detected` (lists which `auth_env` names are set and which `auth_paths` exist — names only, never values or file contents), `not found`, or `verified by probe` for connectors whose probe proves auth.
- **Probe** — `ok` or `failed` with up to 500 chars of the probe's stderr/stdout.
- **Last real run** — the most recent finished non-check run whose agent uses this connector: relative time, run status, and `ticket_get ✓/✗ · result_submit ✓/✗` from `run_tool_calls` (✓ = at least one `ok` call). Links to the run's ticket. "No runs yet" otherwise.
- **Last test** — the most recent connector check: time, pass/fail, and failure reason.
- **Help** — when CLI or auth is missing: the descriptor `auth_hint` plus a vendor install docs link (new descriptor field).

Buttons:

- **Run check** — re-runs the local probes for that connector (server side, ~1 s, bounded by a 10 s timeout).
- **Test connection** — opens a small picker of agents whose `connector` is this id (name + model). With no such agent, the button is disabled and explains "Create an agent with this connector first" with a link to Agents. Starting the test shows live status (queued → running → passed/failed) by polling.

On page load the server returns cached probe results (probed at startup and on every Run check); the page does not probe on every load.

## Local probes (shared library)

New module `coppice_connectors::probe` (std only; no new crate dependencies beyond std):

```rust
pub struct ProbeEnv<'a> {
    pub home: &'a Path,
    pub path: &'a OsStr,             // PATH used for lookup and for the probe child
    pub command_override: Option<&'a str>, // config `command` (cursor, kilo-code, opencode)
}

pub struct ProbeReport {
    pub binary: Option<PathBuf>,     // resolved absolute path
    pub auth_env_set: Vec<&'static str>,
    pub auth_paths_found: Vec<&'static str>,
    pub probe: ProbeOutcome,         // NotRun (binary missing) | Ok { first_line } | Failed { message } | TimedOut
    pub probe_proves_auth: bool,
}

pub fn probe(d: &ConnectorDescriptor, env: &ProbeEnv, timeout: Duration) -> ProbeReport;
```

- The probe command moves into the descriptor: `InstallInfo` gains `probe_args: &'static [&'static str]` (cursor `["models"]`, claude-code / codex / kilo-code `["--version"]`, opencode `["auth","list"]`), `probe_proves_auth: bool` (cursor, opencode), and `docs_url: &'static str`. Adding a connector stays a descriptor entry.
- Binary resolution: `command_override` if set, else `d.binary`; absolute or relative paths are used as given, bare names are searched in `env.path`.
- The probe child gets `env.path` and `HOME=env.home`, stdin null, and is killed at `timeout`. Output is lossy-decoded and capped (first line ≤ 200 chars for `Ok`; message ≤ 500 chars for `Failed`).
- `auth_present` (from `cli/src/commands/connector/registry.rs`) moves here unchanged in behavior; `ProbeReport` exposes which names matched instead of a bool.
- The CLI `doctor` and `list` call `probe` and keep their current output text and exit codes. Their unit tests move or are kept alongside.

### Desktop PATH

macOS apps launched from Finder (and some Linux launchers) do not inherit the login shell's PATH, so a `claude` in `~/.local/bin` or Homebrew is invisible. At server startup, `augment_path(home)` prepends every existing directory from a fixed list that is not already on PATH: `~/.local/bin`, `~/.opencode/bin`, `~/.npm-global/bin`, `~/.bun/bin`, `/opt/homebrew/bin`, `/usr/local/bin`. The server sets its own process PATH once, so probes and real runs resolve binaries identically ("found" on the page means a run finds it too). The list lives in `coppice_connectors::probe::COMMON_BIN_DIRS`.

## Test connection (check runs)

A check is a real agent run, owned by a new `connector_checks` row the same way compaction runs are owned by a batch. It exercises the production path end to end: provider adapter, per-run MCP wiring, token, gateway, `run_tool_calls`, result submission.

### Data

Migration:

- `connector_checks (id UUID PK, connector TEXT NOT NULL, agent_id UUID NOT NULL REFERENCES agents ON DELETE CASCADE, status TEXT NOT NULL CHECK (status IN ('queued','running','passed','failed')), failure TEXT, created_by UUID REFERENCES users ON DELETE SET NULL, created_at, finished_at)`; index `(connector, created_at DESC)`.
- `agent_runs.connector_check_id UUID REFERENCES connector_checks ON DELETE CASCADE`; owner check becomes `num_nonnulls(ticket_id, chat_session_id, compaction_batch_id, connector_check_id) = 1`; unique index on `connector_check_id` where not null.
- Context profile check gains `'connector_check'`.

### Flow

1. `POST /api/tools/connectors/{id}/test {agentId}` — admin, CSRF. Validates the agent exists, `agent.connector == id`, the connector is enabled, and no `queued`/`running` check exists for this connector (409 otherwise). Inserts the check (`queued`) and an agent run with `job_type = "connector_check"`, profile `connector_check`, `plugin_ids = []`, enqueued on the normal job queue. Returns `{ checkId, runId }`.
2. The job worker dispatches `connector_check` to `workers/job_worker/connector_check.rs` (mirroring `compaction.rs`): creates a scratch worktree under `<artifacts_dir>/runs/<run id>/check/` with `.agent/context.md`, runs the agent's provider with the slim check context, then deletes the scratch dir. No repository, branch, or ticket is touched. Run timeout is `min(connector run timeout, 180 s)`.
3. Check context (short, fixed): identity line, "This is a Coppice connection check. Call the `ticket_get` tool, then call `result_submit` with outcome `done` and summary `connection ok`."
4. Profile `connector_check` exposes only `ticket_get` and `result_submit` (core source; no skills, no plugins). `ticket_get` in this profile returns a fixed synthetic ticket (`{ "title": "Coppice connection check", "description": "Submit a done result with summary 'connection ok'." }`) and never reads the database tickets. `result_submit` accepts any outcome; the first submission wins and later ones are denied, and a non-`done` outcome fails the check.
5. On run finish the worker sets the check: **passed** iff the run succeeded **and** `run_tool_calls` has an `ok` `ticket_get` and an `ok` `result_submit`; otherwise **failed** with the first applicable reason: the run's error message (e.g. `mcp_unavailable`, timeout), `"ticket_get was not called"`, `"result_submit was not called"`, or `"result was <outcome>"`. Failure text is ≤ 500 chars and never includes the MCP token or env values.
6. Check runs bypass board/workflow logic: no ticket comments, no status transitions, no notifications, no knowledge capture, no handoffs.

`GET /api/tools/connector-checks/{id}` returns `{ id, connector, agentId, status, failure, runId, createdAt, finishedAt, toolCalls: [{tool, status}] }`.

## API

All under `/api/tools`, admin-only (non-admin → 403, as the backup routes), mutations require `X-CSRF-Token`.

| Route | Returns |
|---|---|
| `GET /connectors` | `[{ id, displayName, enabled, cli: {found, path?, probe: {status: ok\|failed\|timed_out\|not_run, detail?}}, auth: {status: detected\|not_found\|verified_by_probe, envSet: [names], pathsFound: [names]}, authHint, docsUrl, lastRun?: {runId, ticketId?, status, finishedAt, ticketGet: bool, resultSubmit: bool}, lastCheck?: {id, status, failure?, createdAt}, probedAt }]` |
| `POST /connectors/{id}/check` | the same object for one connector, freshly probed |
| `POST /connectors/{id}/test` | `{ checkId, runId }` (400 wrong agent/disabled, 404 unknown connector/agent, 409 check already active) |
| `GET /connector-checks/{id}` | check detail above |

Probe results are cached in memory (`AppState.connector_probes`, keyed by id) and filled by a startup task; probes run in `spawn_blocking`. Unknown connector id → 404; `mock` → 404.

## Security

- Responses carry env variable **names** only, never values; auth paths are names relative to HOME, never contents.
- The probe runs only the descriptor's fixed args — no user input reaches the command line.
- Check runs use a token scoped to profile `connector_check` (two tools, no ticket/board ids), so a check cannot read or write real tickets.
- Probe and failure text are capped; the MCP token is never part of any response or log line.

## Out of scope

Enable toggle / runtime config reload, CLI install, in-app login, storing vendor API keys, changing agent health on the Agents page, scheduled background re-probing beyond startup, check history UI beyond the latest check.

## Testing

- **Library unit tests** (`connectors`): binary resolution (override, bare name on PATH, missing), auth env/paths detection with a temp HOME, probe ok / failed / timed out using a temp script as the binary, output caps. `augment_path` adds only existing dirs, once.
- **CLI**: existing `doctor`/`list` tests keep passing with unchanged output.
- **Server integration** (`--features embedded-test-db`): `GET /connectors` shape and non-admin 403; `check` re-probes (temp script binary via config `command`); `test` validation (wrong connector agent 400, disabled 400, active check 409, CSRF required); a check run with `MockProvider` and fixture `fixtures/agent-responses/mcp/connector_check.json` (`ticket_get`, `result_submit done`) → `passed`; a fixture that skips `ticket_get` → `failed` with `"ticket_get was not called"`; the check run creates no ticket comment and touches no ticket; the `connector_check` profile lists exactly two tools and `ticket_get` returns the synthetic ticket.
- **Web**: Tools tabs and URL state; connector cards for found/missing/auth states; Run check refreshes one card; Test connection picker (disabled with no agents), polling to passed/failed; last-run line.
- **Manual acceptance**: from the page, Test connection passes for `claude-code` and `kilo-code` with real CLIs; results recorded in `docs/providers/README.md` (fixing kilo-code's config variable if needed).

## Acceptance criteria

- [x] Tools page has Backup and Connectors tabs; Connectors shows enabled / CLI / auth / probe / last run / last test per non-mock connector.
- [x] Run check re-probes one connector; Test connection runs a real check run and reports passed/failed with a reason.
- [x] Probe code lives in `coppice_connectors::probe`; CLI `doctor`/`list` use it with unchanged output.
- [x] Server PATH is augmented at startup with existing common bin dirs.
- [x] No env values, file contents, or tokens in any response or log.
- [x] `make test`, clippy, `make web-test` pass; existing smokes unaffected.
