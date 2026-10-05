# Testing

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

Locally, use `make test` (same flags; install [cargo-nextest](https://nexte.st) with `make tools` or it falls back to the slow serial runner). First run may download Postgres binaries (network once); later runs use cache.

For host `make migrate` / dev server, ensure `config.toml` (or `DATABASE_URL`) matches the Postgres you started: `compose-up` → `:5432`, `compose-local-up` → `:5433`.

### Web

```bash
cd web && yarn install --frozen-lockfile && yarn test
```

Vitest — schemas, API helpers, board column logic. No browser.

### Desktop

On `ubuntu-22.04`: release server + web build, fetch the pinned Postgres bundle, assemble resources, `yarn dist:dir` (unpacked, no installer), the **headless smoke** against the packaged resources (two `coppice-server desktop` runs on one data dir: ready line, `/health`, SPA `index.html`, SIGTERM exits 0 with Postgres stopped), then `make desktop-test` (Node unit tests for the shell and release scripts). Locally: `make desktop-test`, or `make desktop-dist-dir desktop-smoke` (`POSTGRES_DIR=` skips the download). The tag-triggered release workflow runs the same headless smoke on every installer target. Rust coverage of desktop mode: `server/tests/integration_desktop.rs`.

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
- `.config/nextest.toml` kills any test still running after 2 minutes, so a hang fails the run instead of stalling it.
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
| **Serial runner** | `cargo test -- --test-threads 1` runs ~1100 tests one by one (~6 min); nextest runs them in parallel (~25 s on 16 cores) |
| **Password hashing** | Argon2 is unusably slow unoptimized and every integration test logs in, so the root `Cargo.toml` builds `argon2`/`blake2` at `opt-level = 3` in the dev profile |
| **Git-heavy tests** (`integration_plugins`, `integration_repo_git`) | Create and clone real repos, ~1 s each |

**Typical wall times** (warm build, 16 cores): `make test` ~25 s of test execution; serial fallback ~6 min.

**Agent / OpenCode runs:** prefer fast iteration during a ticket:

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

Focus: Zod schemas (`lib/schemas/`), pure helpers (`features/board/columns.ts`), `lib/api.ts`.

## E2E smoke

```bash
make e2e-smoke   # compose up + node e2e/smoke/m02-board.mjs
```

Browser script against the compose stack: login → create ticket → drag column → comment. CI may run a subset; full suite grows per milestone in `e2e/`.

M06 keeps the existing context smoke and adds a distinct knowledge smoke:

```bash
make e2e-smoke-m06              # context continuation + pending split behavior
make e2e-smoke-m06-knowledge    # governance → Done ticket → agent compaction → approve → `knowledge_search` in a Full run → audit
```

Both use the default `deploy/docker-compose.yml` stack. The knowledge smoke recreates the server with `MOCK_AGENT_RESPONSE` cleared so agent-keyed fixtures apply (`backend_engineer/compact_knowledge.json` for the compactor; `m06-knowledge-search-worker/work_on_ticket.json` for a preset-less worker agent whose run calls `knowledge_search` through the gateway at `http://127.0.0.1:5000/mcp`, inside the server container). Knowledge is no longer injected into the run context, so usage is logged only by that tool call. It configures the compaction agent, moves a ticket to Done, triggers or waits for compaction, approves the candidate, and restores the previous setting. Compaction integration tests live in `server/tests/integration_knowledge_compaction.rs` and use the root `compact_knowledge_*.json` fixtures for empty, invalid-source, and malformed output.

M10 plugin MCP smoke:

```bash
make e2e-smoke-m10   # plugin dir → setting → Test → enable → assign → mock run → tool-call log
```

It recreates the server with `MOCK_AGENT_RESPONSE=mcp/m10_smoke`, adds `/app/fixtures/plugins/m10-smoke` (baked into the server image) as a plugin dir, and runs a ticket whose mock run calls `ticket_get`, `skill_load` (`m10-smoke:greet`), `m10-smoke__echo` (a `node` stdio MCP server), and `result_submit`; it then checks all four are `ok` in `GET /api/agent-runs/{id}/tool-calls`, with `source = plugin` for the echo call. It is rerunnable on an existing database (timestamped board and agent names).

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

Marketing board capture (`make screenshot`) is separate from these smokes and from CI. It forces desktop mode and writes `static/screenshot.png` plus the site hero `website/public/assets/hero-screenshot.png`. See [development.md — Marketing screenshots](development.md#marketing-screenshots).
