# Embedded Postgres for Rust Tests — Design

**Status:** Approved  
**Date:** 2026-06-10  
**Goal:** Make `cargo test` fast and reliable locally and in CI by using in-process PostgreSQL (`pg-embed`) instead of Docker/external Postgres for all Rust tests.

---

## Problem

Rust integration tests today require external Postgres (`DATABASE_URL`, Docker compose on 5432/5433). When Postgres is down or on the wrong port, tests hang on sqlx pool timeouts (~30s per case) or skip unpredictably. Even with a shared pool fix, developers and agents must manage Docker + port alignment.

Integration tests are **not E2E** — they exercise HTTP + services + real SQL. They should not depend on network infrastructure for a database that can run in-process.

---

## Decision

| Choice | Value |
|--------|-------|
| Engine | **Embedded PostgreSQL** via `pg-embed` v1.x (sqlx 0.8, tokio) |
| Default for Rust tests | **Always embedded** (option A) — `DATABASE_URL` ignored for `cargo test` |
| Escape hatch | `COPPICE_TEST_USE_EXTERNAL_DB=1` + `DATABASE_URL` for manual debugging only |
| Mock DB in integration tests | **No** — keep real SQL, migrations, constraints |
| E2E smoke | Unchanged — Docker compose + browser scripts |

SQLite was rejected: migrations use `UUID`, `JSONB`, `ARRAY`, `gen_random_uuid()`; services use `PgRow` throughout.

---

## Test tiers

| Tier | Location | Database |
|------|----------|----------|
| Unit | `server/src/**` `#[cfg(test)]` | Embedded PG only when test needs DB; otherwise none |
| Integration | `server/tests/integration_*.rs` | Embedded PG |
| E2E | `e2e/smoke/*.mjs` | Docker Postgres (compose stack) |

---

## Architecture

```
cargo test (any server test binary)
        │
        ▼
embedded_test_pool()  ── OnceCell singleton per process
        │
        ├─ pg-embed: setup() [download binary once, cached]
        ├─ start_db() on random free port
        ├─ create_database("coppice_test")
        ├─ sqlx::migrate!("./migrations") once
        │
        ▼
shared PgPool (max_connections ~5)
        │
        ├─ per test: TRUNCATE workspace tables (existing pattern)
        ├─ DB_TEST_LOCK: serialize DB tests (unchanged for now)
        │
        ▼
process exit → stop_db()
```

### Components

| Path | Responsibility |
|------|----------------|
| `server/src/db/test_embed.rs` | Start/stop embedded PG; expose `embedded_test_pool()` |
| `server/src/db/pool.rs` | Route test helpers to embedded pool |
| `server/tests/common/mod.rs` | `db_available()` always succeeds after init; remove network URL defaults |
| `server/Cargo.toml` | `pg-embed` as dev-dependency |
| `.github/workflows/ci.yml` | Remove `services: postgres` and separate migrate step |
| `docs/testing.md` | Document: no Docker required for `cargo test` |

---

## Configuration

### Default (all Rust tests)

- No `DATABASE_URL` required.
- No Docker required.
- First run on a machine may download PG binaries (network once); subsequent runs use cache.

### Escape hatch (debugging only)

```bash
export COPPICE_TEST_USE_EXTERNAL_DB=1
export DATABASE_URL=postgres://coppice:coppice@127.0.0.1:5433/coppice
cargo test -p coppice-server --test integration_tickets
```

Document in `docs/testing.md`. Not used in CI or agent defaults.

### CI cache

Cache pg-embed binary directory in GitHub Actions (`Swatinem/rust-cache` or explicit path from pg-embed docs) to avoid re-download on every run.

---

## Lifecycle details

1. **Port:** Bind embedded PG to a random free port (never 5432/5433) to avoid clashing with dev compose.
2. **Database name:** `coppice_test` (single DB per process).
3. **Migrations:** Same files as production (`server/migrations/`). No parallel SQLite migration set.
4. **Reset between tests:** Existing `truncate_workspace()` / auth truncate helpers — unchanged semantics.
5. **Lib tests with DB:** `run_orchestrator`, `split_service`, etc. use `embedded_test_pool()` instead of `DATABASE_URL` + network connect.
6. **Shutdown:** Stop embedded server when test process exits (Drop on singleton guard or explicit hook).

---

## Error handling

| Failure | Behavior |
|---------|----------|
| Binary download fails (no network, first run) | Fail with message: network required once; cache path documented |
| Port bind fails | Retry with another random port (up to N attempts) |
| Migration fails | Fail fast with migration error (same as today) |
| `pg-embed` start timeout | Fail in &lt; 30s with actionable log (not silent 30s pool wait) |

---

## Speed targets (after implementation)

| Scenario | Target |
|----------|--------|
| `db_available()` | &lt; 50ms |
| Simple integration case | &lt; 500ms |
| Full integration suite (warm build, warm PG cache) | &lt; 5 min |
| `make test-fast` (`--lib`) | &lt; 30s |

Embedded PG removes the **30s wrong-port / no-Postgres** failure mode entirely.

### Follow-up (same milestone, separate tasks)

These are not fixed by embedded PG alone:

- `DB_TEST_LOCK` serializes all DB tests — consider parallel DBs or test sharding later
- `scope_b_mock_pipeline_reaches_final_review` — ~30s worker polling; shorten poll interval in tests or extract faster unit coverage
- 12 integration binaries — cold compile still slow; `make test-fast` remains the agent iteration path

---

## CI changes

**Before:**

```yaml
services:
  postgres: ...
steps:
  - cargo run -p coppice-cli -- migrate
  - cargo test --workspace
```

**After:**

```yaml
steps:
  - cargo test --workspace
```

Optional: cache pg-embed artifacts.

---

## Agent / developer guidance

- **`cargo test --workspace`:** No Docker, no `DATABASE_URL`, no compose-up for Rust tests.
- **`make test-fast`:** Unit tests only; preferred during agent ticket runs.
- **E2E:** Still requires `make compose-up` + smoke scripts.
- **Human dev server:** Still uses `config.toml` + compose-local-up; embedded PG is test-only.

Update `AGENTS.md` and `docs/testing.md` accordingly.

---

## Out of scope

- SQLite or dual migration paths
- Mocking `TicketService` / repository layer in integration tests
- Changing E2E to embedded PG
- Removing `DB_TEST_LOCK` in v1 (may follow if suite is still slow)

---

## Acceptance criteria

- [ ] `cargo test --workspace` passes with **no** Docker Postgres running and **no** `DATABASE_URL` set
- [ ] CI passes without postgres service container
- [ ] Integration test that previously took ~30s when Postgres was down completes in &lt; 2s (skip path eliminated — DB always up)
- [ ] `docs/testing.md` documents embedded default + external escape hatch
- [ ] First-run download failure produces a clear error message

---

## References

- Prior profiling: 30s per test = sqlx acquire timeout + duplicate `connect_and_migrate` per case
- Partial fix in tree: `shared_test_pool`, 2s acquire timeout (superseded by embedded PG default)
- `pg-embed`: https://crates.io/crates/pg-embed
