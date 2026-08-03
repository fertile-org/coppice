# Parallel Rust Test Databases Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate parallel library-test fixture races by cloning one migrated PostgreSQL template into a fresh database for every test pool.

**Architecture:** The existing persistent embedded PostgreSQL process remains shared. A fingerprinted template database is migrated once, while advisory-lock-protected `CREATE DATABASE ... TEMPLATE ...` calls create isolated case databases; pools retain one connection so cleanup can distinguish live cases from stale ones.

**Tech Stack:** Rust, Tokio, SQLx 0.8, pg-embed, PostgreSQL 16

**Design spec:** [docs/superpowers/specs/2026-08-03-parallel-rust-test-databases-design.md](../specs/2026-08-03-parallel-rust-test-databases-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/src/db/pool.rs` | Reuse one compiled migrator and derive a stable fingerprint from all migration versions/checksums. |
| `server/src/db/test_embed.rs` | Allocate, migrate, clone, and clean embedded test databases. |
| `Makefile` | Run the unit target with default Rust test parallelism. |
| `docs/testing.md` | Explain the isolated default and serialized external escape hatch. |

### Task 1: Capture the shared-database race

**Files:**
- Modify: `server/src/db/test_embed.rs`
- Test: `server/src/db/test_embed.rs`

- [ ] **Step 1: Add a deterministic failing regression**

Create two pools, insert a project through the first, truncate through the second, and then insert a ticket through the first:

```rust
#[tokio::test]
async fn embedded_test_pools_isolate_fixture_data() {
    if super::use_external_test_db() {
        return;
    }

    let first = super::embedded_test_pool().await.expect("first pool");
    let second = super::embedded_test_pool().await.expect("second pool");
    crate::db::truncate_test_workspace(&first).await.expect("truncate first");

    let project_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, name, slug) VALUES ($1, $2, $3)")
        .bind(project_id)
        .bind("isolated project")
        .bind(format!("isolated-{project_id}"))
        .execute(&first)
        .await
        .expect("insert project");

    crate::db::truncate_test_workspace(&second)
        .await
        .expect("truncate second");

    sqlx::query(
        "INSERT INTO tickets (id, project_id, title, status, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(project_id)
    .bind("isolated ticket")
    .bind("backlog")
    .bind("test")
    .execute(&first)
    .await
    .expect("first fixture survives second reset");

    let second_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(&second)
        .await
        .expect("count second projects");
    assert_eq!(second_count, 0);
}
```

- [ ] **Step 2: Run the regression against the old allocator**

Run:

```bash
cargo test -p coppice-server --features embedded-test-db \
  db::test_embed::tests::embedded_test_pools_isolate_fixture_data -- --exact --nocapture
```

Expected: FAIL. With a clean legacy database, the second truncate removes the first pool's project and the child insert reports a foreign-key violation. A persisted incompatible database may instead fail earlier during migration, demonstrating the second shared-state defect.

### Task 2: Allocate a fresh database per embedded test pool

**Files:**
- Modify: `server/src/db/pool.rs`
- Modify: `server/src/db/test_embed.rs`
- Test: `server/src/db/test_embed.rs`

- [ ] **Step 1: Make the compiled migrator reusable and fingerprintable**

Add a module-level migrator and deterministically hash every migration version and checksum:

```rust
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub(crate) fn test_migration_fingerprint() -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    MIGRATOR.iter().fold(FNV_OFFSET_BASIS, |hash, migration| {
        migration
            .version
            .to_be_bytes()
            .iter()
            .chain(migration.checksum.as_ref())
            .fold(hash, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
            })
    })
}

pub(crate) async fn migrate_pool(pool: &PgPool) -> anyhow::Result<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}
```

- [ ] **Step 2: Replace the migrated shared database with a fingerprinted template**

In `test_embed.rs`, replace `SESSION_MIGRATED` with:

```rust
static TEMPLATE_DATABASE: OnceCell<String> = OnceCell::const_new();
static CASE_COUNTER: AtomicU64 = AtomicU64::new(0);

const TEMPLATE_DATABASE_PREFIX: &str = "coppice_template_";
const CASE_DATABASE_PREFIX: &str = "coppice_case_";
const TEST_DATABASE_LOCK: i64 = 0x434f_5050_4943_4554;
```

`template_database(session_url)` must:

1. Connect to `postgres` with `PgConnectOptions::from_str(session_url)?.database("postgres")`.
2. Acquire `SELECT pg_advisory_lock($1)` using `TEST_DATABASE_LOCK`.
3. Create `coppice_template_<fingerprint>` if `pg_database` does not contain it.
4. Connect to that database, call `migrate_pool`, and close the pool before the admin connection drops.

The template name is:

```rust
format!(
    "{TEMPLATE_DATABASE_PREFIX}{:016x}",
    crate::db::pool::test_migration_fingerprint()
)
```

- [ ] **Step 3: Clone and connect a case database under the same lock**

Generate the case name with the process ID and atomic counter:

```rust
let case_name = format!(
    "{CASE_DATABASE_PREFIX}{}_{}",
    std::process::id(),
    CASE_COUNTER.fetch_add(1, Ordering::Relaxed)
);
```

While the admin advisory lock is held:

1. Select inactive databases whose names start with `CASE_DATABASE_PREFIX` and have no `pg_stat_activity` rows.
2. Drop each inactive database with a safely quoted identifier.
3. Run `CREATE DATABASE <case> TEMPLATE <template>`.
4. Connect a pool configured with `min_connections(1)`, `max_connections(10)`, disabled idle/lifetime reaping, and a ten-second acquire timeout before dropping the admin connection.

`embedded_test_pool()` uses this path unless `COPPICE_TEST_USE_EXTERNAL_DB=1`; the external path continues through `connect_and_migrate_for_tests`.

- [ ] **Step 4: Run the regression**

Run:

```bash
cargo test -p coppice-server --features embedded-test-db \
  db::test_embed::tests::embedded_test_pools_isolate_fixture_data -- --exact --nocapture
```

Expected: PASS. The first ticket insert succeeds and the second database contains zero projects.

- [ ] **Step 5: Run all database-module tests**

Run:

```bash
cargo test -p coppice-server --features embedded-test-db db::test_embed::tests -- --nocapture
```

Expected: both connection/migration and isolation tests pass even when the legacy `coppice_test` database has an incompatible migration history.

### Task 3: Enable and document parallel unit tests

**Files:**
- Modify: `Makefile`
- Modify: `docs/testing.md`

- [ ] **Step 1: Remove forced serialization from `test-unit`**

Change only the unit target:

```make
# Unit tests only — isolated databases allow default parallelism (~5–15s warm).
test-unit:
	$(CARGO_TEST) --workspace --lib -q
```

Keep `make test` serialized because this ticket does not audit integration tests' process-environment and filesystem locks.

- [ ] **Step 2: Update the testing guide**

Document that the embedded process is shared but each pool clones a migrated, migration-fingerprinted template. State that `COPPICE_TEST_USE_EXTERNAL_DB=1` uses one caller-supplied database and therefore requires serialized database tests.

- [ ] **Step 3: Run the fast unit target**

Run:

```bash
make test-unit
```

Expected: PASS without `--test-threads 1` and without Docker or `DATABASE_URL`.

### Task 4: Stress and quality verification

**Files:**
- Verify: `server/src/db/pool.rs`
- Verify: `server/src/db/test_embed.rs`
- Verify: `Makefile`
- Verify: `docs/testing.md`

- [ ] **Step 1: Format and inspect the diff**

Run:

```bash
rustfmt --edition 2021 --check server/src/db/pool.rs server/src/db/test_embed.rs
git diff --check
```

Expected: both commands exit zero.

- [ ] **Step 2: Repeat the parallel library suite**

Run the following command at least three times:

```bash
cargo test -p coppice-server --features embedded-test-db --lib
```

Expected: every run passes with no foreign-key or migration-history failures.

- [ ] **Step 3: Run Clippy**

Run:

```bash
cargo clippy -p coppice-server --lib --features embedded-test-db -- -D warnings
cargo clippy --workspace -- -D warnings
```

Expected: exit zero with no warnings.

- [ ] **Step 4: Run the integration smoke tier**

Run:

```bash
make test-smoke
```

Expected: the library suite and health, comments, and tickets integration binaries pass against isolated embedded databases.

- [ ] **Step 5: Commit implementation**

```bash
git add Makefile docs/testing.md server/src/db/pool.rs server/src/db/test_embed.rs \
  docs/superpowers/plans/2026-08-03-parallel-rust-test-databases.md
git commit -m "test(server): isolate parallel database fixtures"
```
