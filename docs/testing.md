# Testing

## Suite baseline and cleanup rubric

Measured on **2026-11-21** on `hung-nguyen-thinkpad` (Linux x86_64, 16 cores, 29 GiB RAM). Rust and web runs used a **warm** build (binaries and `node_modules` already present). Counts come from runners (`cargo nextest list`, `yarn vitest list` / `vitest run`, desktop `node:test`), not from grep of `#[test]` / `it(` alone — those under-count `rstest` / `.each` expansions.

### Counts

| Layer | How counted | Cases | Files / binaries |
|-------|-------------|------:|------------------|
| Rust unit (workspace libs + CLI/server bin tests + migrations lib) | `cargo nextest list --workspace --features embedded-test-db` (non-integration binaries) | 1580 | `coppice-server` 1396, `coppice-config` 103, `coppice-connectors` 53, `coppice-cli` 25, `coppice-server` bin 2, `coppice-migrations` 1 |
| Rust integration (`server/tests`) | same list, `coppice-server::integration_*` / `health` / `migration_*` / `real_*` | 752 | 64 test binaries under `server/tests/` |
| Rust workspace total | nextest list | **2332** | — |
| Web (Vitest) | `yarn vitest list` / `vitest run` | **1110** | 151 files under `web/src/**/*.test.*` |
| Desktop (Node) | `cd desktop && yarn test` | **169** | 29 files under `desktop/test/` (+ root smoke) |
| E2E smoke | scripts on disk | **9** | `e2e/smoke/*.mjs` (`e2e/full/` empty) |
| **Suite total (approx.)** | sum of layers above | **~3620** | — |

Largest Rust integration binaries by case count: `integration_plugins` (85), `integration_mcp` (80), `integration_designs` (49), `integration_knowledge` (39), `integration_git_flow` (38). Largest web files: `TicketDrawer.test.tsx` (41), `PluginDetails.test.tsx` (34), `MarketplaceSection.test.tsx` (29), `ChatPage.test.tsx` (28), `AppShell.test.tsx` (26).

### Timings (warm)

| Command | Wall | Notes |
|---------|------|-------|
| `make web-test` | **54 s** | 151 files / 1110 tests passed; Vitest reported Duration 52.51 s |
| `make test` (default fail-fast) | *(re-measure after 077 renumber)* | Duplicate version **077** was fixed by moving `design_set_kind_and_picks` to **083** (idempotent); re-run for a fresh wall time. |
| `cargo nextest run --features embedded-test-db --workspace --no-fail-fast` | **406 s** wall / **359 s** test execution | Historical measurement while 077 was duplicated (embedded migrate failed). Re-measure after the **083** renumber for a green baseline. |

After measurement, reclaim disk with `make clean` (see [AGENTS.md](../AGENTS.md) rule 6).

### Coverage

| Layer | Result |
|-------|--------|
| Rust (`cargo llvm-cov`) | **Not measured** — `cargo-llvm-cov` is not installed on this machine; installing it and a full coverage rebuild was out of scope for a docs-only baseline (no coverage config committed). |
| Web (`vitest --coverage`) | **Not measured** — `@vitest/coverage-v8` is not a dependency; adding it would change `package.json` / lockfile, which this ticket does not do. |

### Cleanup rubric

Use this when deleting or merging tests in follow-on cleanup tickets. Prefer fewer cases that still pin behavior over a large suite that pins wording.

**Delete**

- Assertions that only pin UI copy, button labels, or CSS class names.
- Snapshot tests that fail on harmless markup churn.
- Duplicated schema-default or “parses the default config” cases once one canonical table-driven test remains.

**Merge**

- The same domain rule covered once in a unit test and again in an HTTP integration test — keep the layer that owns the rule (domain → unit; wire/auth/CSRF → integration).
- Per-connector, per-route, or per-status copies of the same path — collapse into one table-driven test.
- Clusters such as `ready_tech_lead_*`, `pick_is_refused_*`, or per-tool MCP happy paths that differ only by a parameter.

**Keep**

- Domain rules (status transitions, permission checks, validation).
- Bug-regression tests that name the failure they catch.
- The last remaining test of a behavior after merges — do not delete the only pin.

### Cleanup candidates (estimated cuts)

Historical estimates from the baseline ticket (case counts below are from **2026-11-21**, before tickets 2–4). See [Cleanup done](#cleanup-done-tickets-24–44) for after counts. Percentages were of that file’s then-current case count.

**Ticket-named starters**

| File | Cases | Action | Est. cut | Why |
|------|------:|--------|--------:|-----|
| `server/tests/integration_plugins.rs` | 85 | merge | 21 | Table-drive plugin-dir / enable / MCP-proxy HTTP paths; keep discovery and security regressions |
| `server/tests/integration_mcp.rs` | 80 | merge | 24 | Merge per-tool auth/token happy paths; keep `result_submit` and auth failures |
| `server/tests/integration_designs.rs` | 49 | merge | 10 | Table-drive `pick_is_refused_*` and publish/list copies; keep `selectedDesign` / artifact path rules |
| `server/src/services/workflow_service.rs` | 48 | merge | 12 | Collapse `ready_tech_lead_*` and overlap with `integration_workflow`; keep one test per unique edge |
| `server/src/services/run_orchestrator.rs` | 41 | merge | 8 | Table-drive `successful_work_request_*` and spawn/env wiring; keep finalize/git and failure classes |
| `server/src/providers/claude_code.rs` | 31 | merge | 8 | Table-drive CLI flag/env matrices; keep `mcp_unavailable` and resume regressions |
| `web/src/features/tickets/TicketDrawer.test.tsx` | 41 | delete | 14 | Drop pure copy/CSS pins; keep status actions and git/conflict flows |
| `web/src/features/plugins/PluginDetails.test.tsx` | 34 | delete | 12 | Drop enablement label/copy pins; keep Test button and MCP tool listing |
| `web/src/features/plugins/MarketplaceSection.test.tsx` | 29 | delete | 10 | Drop install-state text pins; keep install/enable error paths |
| **Subtotal** | | | **~119** | **~3% of ~3620** |

**Additional high-yield files** (same rubric)

| File | Cases | Action | Est. cut |
|------|------:|--------|--------:|
| `server/src/copy/plan.rs` | 22 | delete | 10 |
| `server/src/services/result_contract.rs` | 31 | merge | 6 |
| `server/src/services/context_builder.rs` | 16 | delete | 6 |
| `server/tests/integration_knowledge.rs` | 39 | merge | 6 |
| `server/tests/integration_git_flow.rs` | 38 | merge | 6 |
| `server/tests/integration_chat.rs` | 27 | merge | 4 |
| `web/src/components/AppShell.test.tsx` | 26 | delete | 9 |
| `web/src/features/tools/ConnectorsTab.test.tsx` | 23 | merge | 7 |
| `web/src/features/chat/ChatPage.test.tsx` | 28 | delete | 7 |
| `web/src/features/agents/AgentForm.test.tsx` | 23 | delete | 7 |
| `web/src/features/board/BoardPage.test.tsx` | 20 | delete | 6 |
| `web/src/features/board/TicketCard.test.tsx` | 20 | delete | 6 |
| `web/src/features/plugins/PluginsPage.test.tsx` | 22 | delete | 7 |
| `web/src/features/knowledge/KnowledgePage.test.tsx` | 19 | delete | 6 |
| `web/src/features/plan/PlanReviewActions.test.tsx` | 19 | delete | 6 |
| `config/src/lib.rs` | 47 | merge | 7 |
| **Named + additional** | | | **~225 (~6%)** |

**Why this is not ~30% yet.** Thirty percent of ~3620 is ~1086 cases. The largest files alone only yield a low-single-digit percent under conservative Delete/Merge rates that still keep domain pins. A suite-wide pass that applies the rubric at plausible layer rates can approach the target in follow-on tickets — for example web ~35% (~390), integration ~25% (~190), unit ~15% (~240), desktop ~10% (~17) ≈ **~840 (~23%)**; pushing web toward ~40% and unit toward ~20% lands near **~30%**. Tickets 2–4 should keep walking medium-sized files with the same Delete/Merge/Keep rules rather than only the nine starters above. Fix the duplicate migration **077** before trusting a green `make test` wall time.

### Cleanup done (tickets 2/4–4/4)

Tickets 2–4 applied the rubric above. Baseline numbers in the tables stay as measured on **2026-11-21**; this section is the before/after report.

#### Counts (before → after)

| Layer | Before (baseline) | After | Δ |
|-------|------------------:|------:|--:|
| Rust (`cargo nextest list --workspace --features embedded-test-db`) | 2332 | **2251** (unitish ~1553 / integration ~698) | −81 |
| Web (`make web-test` / Vitest) | 1110 (ticket 3/4 started from 1127) | **740** | −370 vs baseline (−387 vs 1127) |
| Desktop (`cd desktop && yarn test`) | 169 | **123** | −46 (~27%) |
| E2E smoke line count (`wc -l e2e/smoke/*.mjs`) | 3603 | **3486** | −117 |
| E2E smoke scripts | 9 | 9 | 0 |
| **Suite total (approx.)** | **~3620** | **~3114** | **≈ −506 (~14%)** |

What changed in ticket 4/4 (this ticket):

- **Desktop:** Folded release/version and update-path suites into table-driven cases (`release-version`, `stable-installer-name`, `select-published-release`, `render-notes`, `deb-version` left as-is for dpkg pins, `apt-repo` fingerprint merge, `updateCheck` / `updatePlan` / `updateSettings` / `macUpdater`, plus small collapses in `dist` / `logsDir` / `readyLine` / `startupError` / `userDataPath`). Behavior pins kept; per-row `it()` expansions merged into one case with an internal table.
- **E2E:** No new helper (none existed). Trimmed duplicated bootstrap verbosity, skipPlanning echo checks, m03 post-poll `listRuns`, m04 agent-comment assert (owned by m03), m02 optional web ping, and m06-knowledge `/agents` SPA check. Unique board names in m02 so re-runs do not 500 on a fixed name.

Tickets 2/4 and 3/4 already cut Rust (~81) and web (~370). Surviving owners for deleted/merged pins live in those commits (`a39e0335`, `3e5c1b15`) and in the desktop/e2e files listed above.

#### Timings (warm, 2026-11-22)

| Command | Before | After | Notes |
|---------|--------|-------|-------|
| `make web-test` | 54 s | **51 s** wall (Vitest Duration 46.15 s; 740 passed / 153 files) | Faster mainly from fewer cases |
| `make desktop-test` | *(not in baseline)* | **8.5 s** wall (123 passed; Node duration ~1.5 s) | Includes `yarn install --frozen-lockfile` |
| `make test` | *(invalid while 077 duplicated)* | **187 s** wall / **148 s** nextest Summary (2251 passed, 10 skipped) | First green measurement after the 083 renumber; not a delta against the failed historical run. One earlier fail-fast attempt hit a flaky `migration_082_upgrades_uuid_rows_fks_and_checks` embedded-DB race (`database … does not exist`); re-run was green. |

#### Coverage

Still **not measured**. `cargo-llvm-cov` is not installed; `@vitest/coverage-v8` is not a dependency. Adding either is out of scope for cleanup — this ticket kept pins by merging tables rather than claiming coverage %.

#### Cleanup candidates — status

The ticket-named starters and additional high-yield files in the estimate tables above were the targets for tickets 2/4 (Rust) and 3/4 (web). Ticket 4/4 covered desktop release/update suites and e2e duplicate assertions. Remaining estimate rows that still list high case counts are stale relative to the after table; re-list from `cargo nextest list` / `yarn vitest list` before another cleanup pass.

## CI (`.github/workflows/ci.yml`)

Jobs on every push/PR to `main` (Rust, Web, and Desktop below, plus a Docker toolchain check):

### Rust

Rust tests use **embedded PostgreSQL** (`pg-embed`) — no Docker Postgres service in CI:

```bash
export SESSION_SECRET=ci-test-secret
export COPPICE_BOOTSTRAP_PASSWORD=changeme

cargo nextest run --workspace --features embedded-test-db --profile ci
cargo clippy --workspace -- -D warnings
```

Locally, use `make test` (same flags; install [cargo-nextest](https://nexte.st) with `make tools` or it falls back to the slow serial runner). First run may download Postgres binaries (network once); later runs use cache. The desktop app-role import test runs `psql` from `PATH` (`postgresql-client`); CI installs that package.

For host `make migrate` / dev server, ensure `config.toml` (or `DATABASE_URL`) matches the Postgres you started: `compose-up` → `:5432`, `compose-local-up` → `:5433`.

### Web

```bash
cd web && yarn install --frozen-lockfile && yarn test
```

Vitest — schemas, API helpers, board column logic. No browser.

### Desktop

On `ubuntu-22.04`: release server + web build, fetch the pinned Postgres bundle, assemble resources, `yarn dist:dir` (unpacked, no installer), the **headless smoke** against the packaged resources (two `coppice-server desktop` runs on one data dir: ready line, `/health`, SPA `index.html`, SIGTERM exits 0 with Postgres stopped), then `make desktop-test` (Node unit tests for the shell and release scripts). The same job then runs `yarn dist` and `desktop/scripts/deb-cursor-models-e2e.sh`: install that `.deb` with `apt-get install ./file.deb` in a clean `ubuntu:22.04` container so `tzdata`, `libgbm1`, and `libasound2` come from the package Depends, start `/opt/Coppice/coppice` under Xvfb with `env -i HOME PATH=/usr/bin:/bin DISPLAY SHELL=/bin/zsh`, a `~/.zshrc` that prepends `$HOME/.local/bin`, and a fake `agent` there. The script asks the installed server for Cursor model providers and `models?refresh=true` twice (the second call waits out the 2s list cache) and checks that Auto and the other fixture models come back, that `agent` runs again, and that the request log records both refreshes. It also checks `coppice-server --version` against the build commit, a fresh profile whose `config.toml` is the desktop template with no connector lines and still lists Cursor Auto, and a one-time allowlist import of `~/.config/coppice/config.toml` that leaves a forge token decryptable. Locally, after `yarn dist`: `bash desktop/scripts/deb-cursor-models-e2e.sh desktop/dist/Coppice-*.deb`. `make desktop-test`, or `make desktop-dist-dir desktop-smoke` (`POSTGRES_DIR=` skips the download). The tag-triggered release workflow runs the same headless smoke on every installer target. Rust coverage of desktop mode: `server/tests/integration_desktop.rs`.

## Rust test layers

| Layer | Location | Notes |
|-------|----------|-------|
| Unit | `server/src/**` (`#[cfg(test)]`) | Domain validation, config, provider fixtures |
| Integration | `server/tests/integration_*.rs` | Full HTTP stack against real Postgres; `integration_knowledge.rs` covers M06 lifecycle/retrieval/jobs/plans |
| Health | `server/tests/health.rs` | Smoke without DB |

### Agent Chat multi-turn (provider resume)

`server/tests/integration_chat.rs` includes `chat_second_turn_*` and `chat_resume_fallback_succeeds`. Optional env for mock provider: `MOCK_CHAT_EXPECT_SLIM=1`, `MOCK_CHAT_RESUME_FAIL=1`.

```bash
cargo test -p coppice-server --features embedded-test-db --test integration_chat chat_second
```

### Integration test conventions

- Shared helpers: `server/tests/common/mod.rs`
- **No Docker Postgres required** for `cargo test` / `make test`. Tests start in-process PostgreSQL via `pg-embed` (real SQL, same migrations).
- Escape hatch for debugging against compose: `COPPICE_TEST_USE_EXTERNAL_DB=1` + `DATABASE_URL=postgres://coppice:coppice@127.0.0.1:5433/coppice`. This path uses the caller's shared database, so run database tests serially.
- One embedded PostgreSQL process is shared across test binaries. Migrations run once per fingerprinted template; each pool clones a fresh database from that template, so library tests are safe under Rust's parallel runner.
- `make test` and CI use cargo-nextest, which runs every test in its own process, so tests that set process environment (`MOCK_AGENT_RESPONSE`, …) don't race. `DB_TEST_LOCK` only matters under plain `cargo test`, which runs a binary's tests as threads of one process; `truncate_workspace()` preserves the external-database escape hatch.
- `.config/nextest.toml` caps default/CI concurrency at 8 test threads (sized for 16 GB) and kills any test still running after 2 minutes, so a hang fails the run instead of stalling it.
- Auth: `login_and_csrf()` performs bootstrap login, returns session cookie + CSRF token
- Artifact dir: `/tmp/coppice-test-artifacts`

Run all server tests:

```bash
make test
```

### Test speed

| Cause | Effect |
|-------|--------|
| **Many integration binaries** | Each links the full server; cold compile is ~2 min |
| **Serial runner** | `cargo test -- --test-threads 1` runs the suite one by one (~6 min); nextest defaults to 8 threads (see [Memory caps](#memory-caps-16-gb-default)) |
| **Password hashing** | Argon2 is unusably slow unoptimized and every integration test logs in, so the root `Cargo.toml` builds `argon2`/`blake2` at `opt-level = 3` in the dev profile |
| **Git-heavy tests** (`integration_plugins`, `integration_repo_git`) | Create and clone real repos, ~1 s each |

**Typical wall times** (warm build, 16 cores, default 8 nextest threads): see [Suite baseline and cleanup rubric](#suite-baseline-and-cleanup-rubric) for a dated measurement. Historically ~2.5 min wall / ~2 min of test execution when green; serial fallback ~6 min.

### Memory caps (16 GB default)

Unbounded nextest (`test-threads = num-cpus`) plus uncapped `rustc` jobs OOM a 16 GB machine. Measured on a 16-core / 29 GB host (process-tree RSS sampled every 250 ms):

| Run | Wall | Test execution | Peak RSS | Result |
|-----|------|----------------|----------|--------|
| Before (unbounded jobs + threads, full debug) | 5:42 | 71 s (fail-fast after 1592/1727) | **21.6 GiB** | would OOM 16 GB |
| After (defaults below), including test-profile rebuild | 6:14 | 131 s (1727 passed) | 5.7 GiB | fits 16 GB |
| After, warm | 2:45 | 129 s (1727 passed) | 4.3 GiB | fits 16 GB |

The wall-clock cost is extra test execution (~71 s unbounded vs ~129 s capped, ~1.8×) from `test-threads = 8` and `threads-required` on heavy binaries. That is the trade for dropping peak RSS from 22 GiB to ~4–6 GiB. Defaults:

| Cap | Default | Where |
|-----|---------|--------|
| nextest `test-threads` | 8 | `.config/nextest.toml` profiles `default` and `ci` |
| `threads-required` | 4 | Heavy integration binaries: `integration_plugins`, `integration_repo_git`, `integration_ticket_git`, `integration_knowledge`, `integration_knowledge_compaction`, `integration_workflow`, `integration_mcp`, `integration_desktop` |
| cargo build jobs | 4 | `make test` / `make test-smoke` / `make test-unit` via `CARGO_BUILD_JOBS` (CI passes `--build-jobs 4`) |
| test debuginfo | `line-tables-only` | root `Cargo.toml` `[profile.test]` — cuts rustc/linker RSS; backtraces still have line numbers |

**Low-memory override** (~8 GB):

```bash
make test CARGO_BUILD_JOBS=2 NEXTEST_PROFILE=low-mem
# or
CARGO_BUILD_JOBS=2 cargo nextest run --features embedded-test-db --workspace --profile low-mem --build-jobs 2
```

`low-mem` uses `test-threads = 2` and `threads-required = 2` for those same heavy binaries (one heavy test at a time). `NEXTEST_TEST_THREADS` also overrides the profile thread cap.

Agents should not run the full suite while iterating — use targeted tests ([AGENTS.md](../AGENTS.md) rule 7):

```bash
make test-unit              # parallel lib tests only (~seconds)
make test-smoke             # lib + 3 integration smoke files (~10s warm with nextest)
cargo test -p coppice-server result_contract   # one module
cargo test -p coppice-server --test integration_tickets  # one integration file
make web-test               # frontend unit tests
```

When finished with a task (after tests pass), run `make clean` to reclaim disk. See [operations.md](operations.md#disk-usage--cleanup).

Run one integration file:

```bash
cargo test -p coppice-server --test integration_tickets
```

## Web tests

```bash
make web-test
# or
make web-test
```

`web/vitest.config.ts` sets `maxWorkers: 4` so default `yarn test` / `npm test` stays within ~16 GB alongside jsdom (same intent as the nextest caps above).

Focus: Zod schemas (`lib/schemas/`), pure helpers (`features/board/columns.ts`), `lib/api.ts`.

## E2E smoke

```bash
make e2e-smoke   # compose up + node e2e/smoke/m02-board.mjs
```

Browser script against the compose stack: login → create ticket → drag column → comment. CI may run a subset; full suite grows per milestone in `e2e/`.

M06 keeps the existing context smoke and adds a distinct knowledge smoke:

```bash
make e2e-smoke-m06              # context continuation + pending split behavior
make e2e-smoke-m06-knowledge    # governance → Done ticket → agent compaction → approve → index in a Full run's context → `knowledge_search`, `knowledge_get`, `knowledge_flag` → usage audit and Inbox flag
```

Both use the default `deploy/docker-compose.yml` stack. The knowledge smoke recreates the server with `MOCK_AGENT_RESPONSE` cleared so agent-keyed fixtures apply (`backend_engineer/compact_knowledge.json` for the compactor; `m06-knowledge-search-worker/work_on_ticket.json` for a preset-less worker agent whose run calls `knowledge_search`, `knowledge_get` and `knowledge_flag` through the gateway at `http://127.0.0.1:5000/mcp`, inside the server container). Knowledge text is not injected into the run context; only the start-of-run index of titles is, and usage is logged only by `knowledge_get`. It configures the compaction agent, moves a ticket to Done, triggers or waits for compaction, approves the candidate, and restores the previous setting. Compaction integration tests live in `server/tests/integration_knowledge_compaction.rs` and use the root `compact_knowledge_*.json` fixtures for empty, invalid-source, and malformed output.

M10 plugin MCP smoke:

```bash
make e2e-smoke-m10   # plugin dir → setting → Test → enable → assign → mock run → tool-call log
```

It recreates the server with `MOCK_AGENT_RESPONSE=mcp/m10_smoke`, adds `/app/fixtures/plugins/m10-smoke` (baked into the server image) as a plugin dir, and runs a ticket whose mock run calls `ticket_get`, `skill_load` (`m10-smoke:greet`), `m10-smoke__echo` (a `node` stdio MCP server), and `result_submit`; it then checks all four are `ok` in `GET /api/agent-runs/{id}/tool-calls`, with `source = plugin` for the echo call. It is rerunnable on an existing database (timestamped board and agent names). After that flow, the target recreates the server with `MOCK_AGENT_RESPONSE` cleared and runs `e2e/smoke/m10-plugin-builder.mjs`: mock PM `plan_plugin_build` → approve/build via `backend_engineer/build_plugin.json` (`copyFixtureDir` + content map) → disabled install + Test → Accept with enable.

The supported 10,000-eligible-row retrieval envelope has a separate, non-CI
default-Compose benchmark. It seeds rows inside a rolled-back transaction, runs
the production retrieval query 20 times after warmup, and fails unless measured
p95 PostgreSQL execution time remains below 250 ms:

```bash
make benchmark-m06-knowledge-retrieval
```

Keep this benchmark separate from routine integration tests because seeding
10,000 revisions and their full-text GIN entries is intentionally heavier than
the representative mixed-cardinality query-plan assertion.

## Agent / provider testing

- **Always `MockProvider` in automated tests.** Returns JSON from `fixtures/agent-responses/` (`done.json`, `blocked.json`, …).
- Contract: `AgentRunResult` in `server/src/providers/mod.rs` — must match product design §17.
- Do not wire real CLI tools (Claude Code, Codex, etc.) into CI.
- **QA evidence:** QC `criteriaResults` may include `screenshots` under `.agent/qa-evidence/` (png/jpeg/gif/webp). On record, the server copies them to `artifacts/tickets/{id}/qa-evidence/` and serves them at `GET /api/tickets/{id}/qa-evidence/{file}` (signed-in only). See `integration_qa_evidence` and the `coppice-qc-verification` skill.

## What to test when adding features

1. **Domain rules** — unit tests on enums/validation (status + substatus combos, comment intents).
2. **API behavior** — integration test for happy path + key error cases (401 without session, 403 without CSRF, validation 400).
3. **Web schemas** — Vitest for form/column helpers touched by the change.
4. **Milestone acceptance** — check boxes in the relevant `docs/milestones/M0N-*.md` spec.

## Pre-push checklist

```bash
make test          # embedded Postgres — no compose required
make clippy
make web-test
make desktop-test  # when touching desktop/ or desktop mode
```

Optional before E2E: `make compose-up` (Docker stack for browser smoke only).

Optional before UI-heavy changes: `make e2e-smoke`.

Marketing assets are outside these smokes and CI. `make hero-video` renders the Remotion homepage hero (mp4/webm/gif/poster under `website/public/assets/` and `static/marketing.gif`). `make screenshot` uses Compose project `coppice-screenshot`, forces desktop mode, and refreshes docs stills only — it does not overwrite the Remotion hero binaries. See [development.md — Marketing screenshots](development.md#marketing-screenshots).
