# Embedded Postgres for Rust Tests — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpower:subagent-driven-development (recommended) or superpower:executing-plans to implement this plan task-by-task. Do not use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run all Rust tests against in-process PostgreSQL via `pg-embed` by default — no Docker, no `DATABASE_URL`, no 30s pool timeouts when Postgres is down.

**Architecture:** Singleton embedded PG per `cargo test` process; shared pool + migrate once; `TRUNCATE` between cases. Production server still uses `connect_and_migrate(config.database.url)`. Escape hatch: `COPPICE_TEST_USE_EXTERNAL_DB=1`.

**Tech Stack:** Rust, sqlx 0.8, pg-embed 1.x, tokio, existing `server/migrations/`

**Design spec:** [docs/superpower/specs/2026-06-10-embedded-postgres-tests-design.md](../specs/2026-06-10-embedded-postgres-tests-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/src/db/test_embed.rs` | Start/stop pg-embed; `embedded_test_pool()` |
| `server/src/db/pool.rs` | Route test pool to embedded; keep prod `connect_and_migrate` |
| `server/src/db/mod.rs` | Export test_embed API |
| `server/tests/common/mod.rs` | Use `embedded_test_pool`; simplify `db_availability` |
| `server/tests/integration_auth.rs` | Already uses common helpers — verify only |
| `server/src/services/*` lib tests | Replace `DATABASE_URL` with `embedded_test_pool()` |
| `server/Cargo.toml` | Add `pg-embed` dev-dependency |
| `.github/workflows/ci.yml` | Remove postgres service + migrate step |
| `docs/testing.md`, `AGENTS.md`, `docs/development.md` | Document new test model |
| `Makefile` | Optional `test-embedded` alias |

---

## Phase 1 — Embedded PG core

### Task 1: `test_embed` module + unit test

**Files:**
- Create: `server/src/db/test_embed.rs`
- Modify: `server/src/db/mod.rs`
- Modify: `server/Cargo.toml`
- Test: `server/src/db/test_embed.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Add dev-dependency**

```toml
# server/Cargo.toml [dev-dependencies]
pg-embed = { version = "1.0", default-features = false, features = ["rt_tokio_migrate"] }
```

- [ ] **Step 2: Implement `test_embed.rs`**

Core API:

```rust
// server/src/db/test_embed.rs
#[cfg(test)]
pub async fn embedded_test_pool() -> anyhow::Result<sqlx::PgPool> { ... }

#[cfg(test)]
pub fn use_external_test_db() -> bool {
    std::env::var("COPPICE_TEST_USE_EXTERNAL_DB").as_deref() == Ok("1")
}
```

Implementation sketch:
- `static EMBED: OnceCell<EmbeddedPg>` holding `PgEmbed` + pool
- `pick_free_port()` via `TcpListener::bind("127.0.0.1:0")`
- `PgSettings { persistent: false, port, user, password, migration_dir: None, ... }`
- `PgFetchSettings { version: PG_V16 or PG_V17, .. }` (match CI postgres 16)
- `setup()` → `start_db()` → `create_database("coppice_test")` → connect pool → `migrate_pool`
- `EmbeddedPg` implements `Drop` to call `stop_db()`
- If `use_external_test_db()`, delegate to existing `connect_and_migrate_for_tests(DATABASE_URL)`

- [ ] **Step 3: Write failing test**

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn embedded_test_pool_connects_and_migrates() {
        let pool = super::embedded_test_pool().await.expect("embedded pool");
        let row: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, 1);
        // migrations applied — users table exists
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0);
    }
}
```

- [ ] **Step 4: Run test**

Run: `cargo test -p coppice-server db::test_embed::tests::embedded_test_pool_connects_and_migrates -- --nocapture`

Expected: PASS (may download PG binary first run — allow network)

- [ ] **Step 5: Commit**

```bash
git add server/src/db/test_embed.rs server/src/db/mod.rs server/Cargo.toml
git commit -m "feat(test): add pg-embed embedded postgres for Rust tests"
```

---

### Task 2: Wire `shared_test_pool` to embedded default

**Files:**
- Modify: `server/src/db/pool.rs`
- Modify: `server/tests/common/mod.rs`

- [ ] **Step 1: Update `pool.rs`**

```rust
pub async fn shared_test_pool() -> anyhow::Result<PgPool> {
    #[cfg(test)]
    {
        return crate::db::test_embed::embedded_test_pool().await;
    }
    #[cfg(not(test))]
    {
        anyhow::bail!("shared_test_pool only for tests");
    }
}
```

Remove URL parameter from `shared_test_pool` — callers pass nothing.

- [ ] **Step 2: Update `common/mod.rs`**

```rust
pub async fn db_availability() -> bool {
    db::shared_test_pool().await.is_ok()
}

async fn prepare_test_pool() -> sqlx::PgPool {
    let pool = db::shared_test_pool().await
        .expect("embedded test database");
    truncate_workspace(&pool).await;
    pool
}
```

Remove `test_database_url()` unless escape hatch needs it.

- [ ] **Step 3: Fix all `shared_test_pool(&url)` call sites**

Grep: `shared_test_pool` — update in:
- `server/tests/common/mod.rs`
- `server/src/services/run_orchestrator.rs` (test module)
- `server/src/services/run_service.rs`
- `server/src/services/split_service.rs`
- `server/src/services/agent_service.rs` — switch from `connect_and_migrate_for_tests` to `shared_test_pool()`

- [ ] **Step 4: Run integration smoke**

Run: `cargo test -p coppice-server --test integration_comments -- --nocapture`

Expected: PASS without Docker Postgres

- [ ] **Step 5: Commit**

```bash
git add server/src/db/pool.rs server/tests/common/mod.rs server/src/services/
git commit -m "feat(test): default shared_test_pool to embedded postgres"
```

---

## Phase 2 — CI + docs

### Task 3: CI workflow

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Remove postgres service**

Delete `services: postgres:` block and `cargo run -p coppice-cli -- migrate` step.

- [ ] **Step 2: Optional cache pg-embed binaries**

Add to rust job after checkout:

```yaml
- name: Cache pg-embed
  uses: actions/cache@v4
  with:
    path: ~/.embedded-postgres
    key: pg-embed-${{ runner.os }}-${{ hashFiles('server/Cargo.toml') }}
```

(Adjust path to match pg-embed cache location from crate docs.)

- [ ] **Step 3: Verify locally**

Run: `cargo test --workspace` with Docker Postgres **stopped**

Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: use embedded postgres for Rust tests"
```

---

### Task 4: Documentation

**Files:**
- Modify: `docs/testing.md`
- Modify: `AGENTS.md`
- Modify: `docs/development.md`

- [ ] **Step 1: Update testing.md**

Document:
- Rust tests: **no Docker** required; embedded PG default
- Escape hatch: `COPPICE_TEST_USE_EXTERNAL_DB=1` + `DATABASE_URL`
- Remove "Postgres must be running" for integration tests
- Keep E2E compose requirements

- [ ] **Step 2: Update AGENTS.md rule 6/7**

Replace external Postgres guidance with embedded default + `make test-fast` for iteration.

- [ ] **Step 3: Commit**

```bash
git add docs/testing.md AGENTS.md docs/development.md
git commit -m "docs: embedded postgres as default for cargo test"
```

---

## Phase 3 — Speed validation (follow-up in same PR if time)

### Task 5: Acceptance timing + Makefile

**Files:**
- Modify: `Makefile` (optional)
- Modify: `docs/testing.md`

- [ ] **Step 1: Time full suite**

Run with Docker stopped:

```bash
time cargo test --workspace
time cargo test -p coppice-server --test integration_workflow
```

Record wall times in PR description. Targets: integration_workflow &lt; 3 min warm; full workspace &lt; 8 min warm (compile excluded on rerun).

- [ ] **Step 2: Add Makefile target (optional)**

```makefile
test-embedded:
	cargo test --workspace
```

- [ ] **Step 3: Commit if Makefile changed**

---

## Spec coverage checklist

| Spec requirement | Task |
|----------------|------|
| pg-embed singleton per process | 1 |
| Random port | 1 |
| Migrate once | 1, 2 |
| TRUNCATE between tests | 2 (unchanged) |
| Escape hatch external DB | 1, 2 |
| CI no postgres service | 3 |
| Lib tests use embedded | 2 |
| Docs | 4 |
| Acceptance: no Docker | 3, 5 |

---

## Execution handoff

Plan complete and saved to `docs/superpower/plans/2026-06-10-embedded-postgres-tests.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks
2. **Inline Execution** — implement in this session with checkpoints

Which approach do you want?
