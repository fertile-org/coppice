# Testing

## CI (`.github/workflows/ci.yml`)

Two jobs on every push/PR to `main`:

### Rust

Runs against a service Postgres (`pgvector/pgvector:pg16`):

```bash
export DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice
export SESSION_SECRET=ci-test-secret
export COPPICE_BOOTSTRAP_PASSWORD=changeme

cargo run -p coppice-cli -- migrate
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Locally, `cargo test` / `clippy` do not need Postgres. For host `make migrate`, ensure `config.toml` (or `DATABASE_URL`) matches the Postgres you started: `compose-up` → `:5432`, `compose-local-up` → `:5433`.

### Web

```bash
cd web && yarn install --frozen-lockfile && yarn test
```

Vitest — schemas, API helpers, board column logic. No browser.

## Rust test layers

| Layer | Location | Notes |
|-------|----------|-------|
| Unit | `server/src/**` (`#[cfg(test)]`) | Domain validation, config, provider fixtures |
| Integration | `server/tests/integration_*.rs` | Full HTTP stack against real Postgres |
| Health | `server/tests/health.rs` | Smoke without DB |

### Integration test conventions

- Shared helpers: `server/tests/common/mod.rs`
- `db_available()` skips tests when `DATABASE_URL` is unreachable
- `DB_TEST_LOCK` serializes DB tests; `truncate_workspace()` resets tables between cases
- Auth: `login_and_csrf()` performs bootstrap login, returns session cookie + CSRF token
- Artifact dir: `/tmp/coppice-test-artifacts`

Run all server tests:

```bash
make test
```

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
make compose-up
make migrate
make test
make clippy
make web-test
```

Optional before UI-heavy changes: `make e2e-smoke`.
