# Parallel Rust Test Databases — Design

**Status:** Approved
**Date:** 2026-08-03
**Goal:** Make database-backed Rust library tests deterministic under the default parallel test runner without paying the cost of serializing the whole unit suite.

---

## Problem

`shared_test_pool()` currently returns a fresh SQLx pool connected to one shared `coppice_test` database. Each database-backed library test calls `truncate_test_workspace()` before inserting its fixture. Under Rust's default parallel test runner, one test can truncate tables after another test has inserted a parent row and before it inserts a child row. The observed result is intermittent foreign-key failures: a serialized run passes, while a parallel run loses multiple tests.

The shared migrated database has a second isolation failure. The embedded PostgreSQL process and `coppice_test` database persist across worktrees. If another worktree applies a migration not present in the current checkout, SQLx rejects this checkout with `migration N was previously applied but is missing` before tests run.

Serializing the suite avoids both races but makes unrelated pure unit tests wait for database tests. A global mutex is therefore not an acceptable fix.

## Decision

Keep one embedded PostgreSQL server, but give every call to `embedded_test_pool()` a fresh database cloned from an immutable migrated template:

```text
embedded PostgreSQL session
        |
        +-- coppice_template_<migration fingerprint>
        |       migrations applied once
        |
        +-- coppice_case_<process>_<counter>  <- pool for test A
        +-- coppice_case_<process>_<counter>  <- pool for test B
        +-- coppice_test                      <- legacy database, unused by tests
```

The template name includes a fingerprint of every migration version and checksum. Different branches or worktrees therefore never reuse a template with incompatible migration history. Creating templates and cloning case databases is protected by a PostgreSQL advisory lock because the embedded server is shared across test processes.

Each returned case pool keeps at least one connection open. Before cloning another case, the allocator removes only `coppice_case_` databases with no active connections. This bounds generated database buildup without risking a database still owned by a running test. One final inactive case may remain until the next allocation.

The manual `COPPICE_TEST_USE_EXTERNAL_DB=1` escape hatch keeps its existing shared-database behavior. It is a debugging path and still requires serialized database tests; the default embedded path is the supported parallel path.

## Components

| Path | Responsibility |
|------|----------------|
| `server/src/db/pool.rs` | Own one static migrator and expose its migration fingerprint. |
| `server/src/db/test_embed.rs` | Create/reuse the fingerprinted template, clone isolated case databases, and reclaim inactive cases. |
| `Makefile` | Let `make test-unit` use Rust's parallel runner; keep full-suite serialization unchanged. |
| `docs/testing.md` | Document isolated embedded databases and the external escape-hatch constraint. |

Production connection and migration code does not change. Integration-test locks also remain in place because some integration cases coordinate process environment and filesystem state beyond PostgreSQL.

## Data and error flow

1. Resolve or start the shared embedded PostgreSQL session.
2. Connect to its built-in `postgres` database and acquire the test-database advisory lock.
3. Derive `coppice_template_<fingerprint>` from the compiled migration set.
4. Create the template if absent, connect to it, and run/validate migrations.
5. Close every template connection before releasing the advisory lock.
6. For each requested pool, reacquire the lock, drop inactive case databases, clone a unique case database from the template, and connect its pool before releasing the lock.
7. Return errors with database/template context; dropping the administrative connection releases the advisory lock on every error path.

Database identifiers are generated internally from fixed ASCII prefixes, a hexadecimal fingerprint, the process ID, and an atomic counter. No user input is interpolated into `CREATE DATABASE` or `DROP DATABASE` statements.

## Testing

- Add a deterministic regression that creates two pools, inserts a parent in the first, truncates the second, and then inserts a child in the first. It fails against the old shared database with the same foreign-key mechanism as the reported race.
- Assert that the second database cannot see the first database's project.
- Run the complete server library suite repeatedly with the default parallel runner.
- Run `make test-unit` to verify the fast target no longer forces `--test-threads 1`.
- Run formatting and Clippy for the touched Rust code.

## Acceptance criteria

- [ ] Database-backed library tests cannot delete or observe another test's fixture data.
- [ ] A persisted database migrated by another worktree cannot poison this worktree's test migrations.
- [ ] `make test-unit` runs with Rust's default parallelism.
- [ ] Repeated parallel server library runs pass without foreign-key collisions.
- [ ] Production database behavior and the external debugging escape hatch remain unchanged.

## Rejected alternatives

- **Global async mutex / `--test-threads 1`:** deterministic but retains the performance problem.
- **Unique schema per test:** PostgreSQL extensions, search paths, and cleanup are more error-prone than native database cloning.
- **Remove truncation and rely on UUIDs:** several tests intentionally query global job/mention state, so foreign fixtures would still change assertions and service behavior.
