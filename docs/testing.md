# Testing

## CI (`.github/workflows/ci.yml`)

Two jobs on every push/PR to `main`:

### Rust

Rust tests use **embedded PostgreSQL** (`pg-embed`) — no Docker Postgres service in CI:

```bash
export SESSION_SECRET=ci-test-secret
export COPPICE_BOOTSTRAP_PASSWORD=changeme

cargo test --workspace --features embedded-test-db
cargo clippy --workspace -- -D warnings
```

Locally, use `make test` (same flags). First run may download Postgres + pgvector binaries (network once); later runs use cache.

For host `make migrate` / dev server, ensure `config.toml` (or `DATABASE_URL`) matches the Postgres you started: `compose-up` → `:5432`, `compose-local-up` → `:5433`.

### Web

```bash
cd web && yarn install --frozen-lockfile && yarn test
```

Vitest — schemas, API helpers, board column logic. No browser.

## Rust test layers

| Layer | Location | Notes |
|-------|----------|-------|
| Unit | `server/src/**` (`#[cfg(test)]`) | Domain validation, config, provider fixtures |
| Integration | `server/tests/integration_*.rs` | Full HTTP stack against real Postgres; `integration_knowledge.rs` covers M06 lifecycle/retrieval/jobs/plans |
| Health | `server/tests/health.rs` | Smoke without DB |

### Integration test conventions

- Shared helpers: `server/tests/common/mod.rs`
- **No Docker Postgres required** for `cargo test` / `make test`. Tests start in-process PostgreSQL via `pg-embed` (real SQL, same migrations).
- Escape hatch for debugging against compose: `COPPICE_TEST_USE_EXTERNAL_DB=1` + `DATABASE_URL=postgres://coppice:coppice@127.0.0.1:5433/coppice`.
- One shared embedded pool + one migration pass per integration-test **binary**; each case only `TRUNCATE`s tables.
- `DB_TEST_LOCK` serializes DB tests; `truncate_workspace()` resets tables between cases
- Auth: `login_and_csrf()` performs bootstrap login, returns session cookie + CSRF token
- Artifact dir: `/tmp/coppice-test-artifacts`

Run all server tests:

```bash
make test
```

### Why `cargo test --workspace` is slow

| Cause | Effect |
|-------|--------|
| **12 integration binaries** | Each links the full server; cold compile is minutes |
| **`DB_TEST_LOCK`** | All DB tests run serially — times add up |
| **Workflow pipeline test** (`scope_b_mock_pipeline_reaches_final_review`) | Full multi-agent mock pipeline ~30s alone |
| **Agent-run tests** | Spawn job workers + poll for run completion |
| **Postgres down / wrong port** | Eliminated: embedded PG is always up when `embedded-test-db` feature is enabled |

**Typical wall times** (Postgres up, warm build): unit tests ~1 min; full integration suite ~15–25 min.

**Agent / OpenCode runs:** do not use `make test` during a ticket. Use fast iteration instead:

```bash
make test-unit              # lib tests only (~seconds)
make test-smoke             # lib + 3 integration smoke files (~<60s warm)
cargo test -p coppice-server result_contract   # one module
cargo test -p coppice-server --test integration_tickets  # one integration file
make web-test               # frontend unit tests
```

When finished with a task (after tests pass), run `make clean` to reclaim disk. See [development.md](development.md#disk-usage--cleanup).

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
make e2e-smoke-m06-knowledge    # governance → embed → Full retrieval → audit → extraction
```

Both use the default `deploy/docker-compose.yml` stack. The knowledge smoke uses only deterministic mock agent, embedding, and extraction providers.

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
```

Optional before E2E: `make compose-up` (Docker stack for browser smoke only).

Optional before UI-heavy changes: `make e2e-smoke`.
