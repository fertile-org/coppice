# M03 Retcon — Registered Repositories Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace lazy `git clone` with admin-registered `local_path` repositories (global registry + Settings UI), and refactor the agent worker to create worktrees from registered paths only.

**Architecture:** Migration makes `repos` instance-wide with `local_path` + verification status. `RepoService` owns CRUD/verify; worker uses `repo.local_path` as git dir and `WORKTREES_PATH` for worktrees. Remove `ensure_repo_clone`, `GIT_REPOS_PATH`, and `repo_data` volume. Tests register temp `git init` checkouts via `POST /api/repos`.

**Tech Stack:** Rust/Axum/SQLx/tokio::process, React/Vite/TanStack Query, git CLI, Docker Compose, Vitest, Node smoke E2E

**Spec:** [docs/superpowers/specs/2026-06-08-m03-registered-repositories-design.md](../specs/2026-06-08-m03-registered-repositories-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/migrations/004_registered_repos.sql` | `local_path`, verification columns; drop `project_id` |
| `server/src/domain/repo.rs` | `VerificationStatus`, updated `Repo` struct |
| `server/src/services/repo_verifier.rs` | Path exists + `git rev-parse` validation |
| `server/src/services/repo_service.rs` | Global repo CRUD, verify, delete guards |
| `server/src/services/project_service.rs` | Remove repo CRUD methods |
| `server/src/services/worktree_service.rs` | Remove clone; simplify `compute_paths` |
| `server/src/services/run_service.rs` | Require `verification_status = ready` |
| `server/src/services/context_builder.rs` | Repository section in context md |
| `server/src/workers/job_worker.rs` | Use `local_path` not clone |
| `server/src/api/repos.rs` | Global routes + `POST verify` + admin guards |
| `server/src/config/mod.rs` | Remove `git_repos_path` from `AgentConfig` |
| `deploy/config/default.yaml` | Remove `git_repos_path` |
| `deploy/docker-compose.yml` | Remove `repo_data`, `GIT_REPOS_PATH` |
| `deploy/docker-compose.local.yml` | Same |
| `server/tests/common/mod.rs` | `create_temp_git_checkout`, `register_test_repo` |
| `server/tests/integration_agent_runs.rs` | Register-path flow |
| `server/tests/integration_workspace.rs` | Update repo helper usage |
| `web/src/lib/schemas/repo.ts` | Zod schema |
| `web/src/features/repos/useRepos.ts` | Query + mutations |
| `web/src/features/repos/RepositoriesPage.tsx` | Admin CRUD + member read-only |
| `web/src/App.tsx` | `/settings/repositories` route |
| `web/src/components/AppShell.tsx` | Nav link |
| `web/src/features/tickets/TicketMetadataTab.tsx` | Global repo picker |
| `e2e/smoke/m03-agent-run.mjs` | Register local path before run |
| `AGENTS.md` | Mark retcon complete when done |

---

### Task 1: Migration + domain types

**Files:**
- Create: `server/migrations/004_registered_repos.sql`
- Modify: `server/src/domain/repo.rs`

- [ ] **Step 1: Write migration**

```sql
-- server/migrations/004_registered_repos.sql
ALTER TABLE repos
    ADD COLUMN local_path TEXT,
    ADD COLUMN verification_status TEXT NOT NULL DEFAULT 'path_missing',
    ADD COLUMN verification_error TEXT,
    ADD COLUMN last_verified_at TIMESTAMPTZ,
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- Dev/CI: truncate or backfill before NOT NULL enforcement in same migration:
UPDATE repos SET local_path = '/tmp/coppice-migration-placeholder' WHERE local_path IS NULL;
ALTER TABLE repos ALTER COLUMN local_path SET NOT NULL;

ALTER TABLE repos DROP CONSTRAINT IF EXISTS repos_project_id_fkey;
DROP INDEX IF EXISTS repos_project_id_idx;
ALTER TABLE repos DROP COLUMN project_id;

CREATE UNIQUE INDEX repos_local_path_idx ON repos (local_path);
```

Note: integration tests call `truncate_workspace` which clears repos; no production data in M03 era.

- [ ] **Step 2: Update domain types**

`server/src/domain/repo.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Ready,
    PathMissing,
    NotGitRepo,
    Error,
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub id: Uuid,
    pub name: String,
    pub local_path: String,
    pub remote_url: Option<String>,
    pub default_branch: String,
    pub verification_status: VerificationStatus,
    pub verification_error: Option<String>,
    pub last_verified_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

pub fn verification_status_to_str(s: VerificationStatus) -> &'static str { /* ready, path_missing, not_git_repo, error */ }
pub fn verification_status_from_str(s: &str) -> Option<VerificationStatus> { /* … */ }
```

- [ ] **Step 3: Run migration**

Run: `make migrate-local` (or `make migrate` if default stack up)

Expected: `migrations applied`

- [ ] **Step 4: Commit**

```bash
git add server/migrations/004_registered_repos.sql server/src/domain/repo.rs
git commit -m "feat(server): migration for registered repository local_path"
```

---

### Task 2: Repo verifier

**Files:**
- Create: `server/src/services/repo_verifier.rs`
- Modify: `server/src/services/mod.rs`

- [ ] **Step 1: Write failing tests**

```rust
// server/src/services/repo_verifier.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyResult {
    pub status: VerificationStatus,
    pub error: Option<String>,
}

pub fn verify_local_path(path: &Path) -> VerifyResult { /* … */ }

#[cfg(test)]
mod tests {
    #[test]
    fn missing_path_returns_path_missing() { /* /tmp/does-not-exist-{uuid} */ }

    #[test]
    fn non_git_dir_returns_not_git_repo() {
        // tempfile::tempdir(), no git init
    }

    #[test]
    fn git_init_dir_returns_ready() {
        // git init + commit in tempdir
    }
}
```

Use `std::process::Command` for `git -C {path} rev-parse --git-dir` (sync is fine for verify).

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test -p coppice-server repo_verifier -- --nocapture`

- [ ] **Step 3: Implement `verify_local_path`**

1. Empty path → `PathMissing`
2. `!path.exists()` → `PathMissing`
3. `git -C path rev-parse --git-dir` success → `Ready`
4. Command fails → `NotGitRepo` or `Error` with stderr snippet

- [ ] **Step 4: Run tests — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add server/src/services/repo_verifier.rs server/src/services/mod.rs
git commit -m "feat(server): add repository path verifier"
```

---

### Task 3: RepoService (global CRUD)

**Files:**
- Create: `server/src/services/repo_service.rs`
- Modify: `server/src/services/project_service.rs` (remove repo methods)
- Modify: `server/src/services/mod.rs`

- [ ] **Step 1: Implement RepoService**

```rust
pub struct RepoService<'a> { pool: &'a PgPool }

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("repo not found")]
    NotFound,
    #[error("repo in use by tickets")]
    InUse,
    #[error("duplicate local_path")]
    DuplicatePath,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl RepoService<'_> {
    pub async fn list_all(&self) -> Result<Vec<Repo>, RepoError>;
    pub async fn get(&self, id: Uuid) -> Result<Repo, RepoError>;
    pub async fn create(&self, name: &str, local_path: &str, remote_url: Option<&str>, default_branch: &str) -> Result<Repo, RepoError>;
    pub async fn update(&self, id: Uuid, name: Option<&str>, local_path: Option<&str>, remote_url: Option<Option<&str>>, default_branch: Option<&str>) -> Result<Repo, RepoError>;
    pub async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    pub async fn verify(&self, id: Uuid) -> Result<Repo, RepoError>;
}
```

- `create` / `update` (when path changes): call `verify_local_path`, persist status + `last_verified_at`
- `delete`: `SELECT EXISTS(SELECT 1 FROM tickets WHERE repo_id = $1)` → `InUse` if true
- `create`: unique violation on `local_path` → `DuplicatePath`
- Row mapper: include all new columns; snake_case DB ↔ domain

- [ ] **Step 2: Remove repo CRUD from `project_service.rs`**

Delete: `list_repos`, `create_repo`, `get_repo`, `update_repo`, `delete_repo`, `row_to_repo` (move to repo_service).

- [ ] **Step 3: Unit test roundtrips**

Add `verification_status_from_str` roundtrip test in `domain/repo.rs` or repo_service tests.

- [ ] **Step 4: Commit**

```bash
git add server/src/services/repo_service.rs server/src/services/project_service.rs server/src/services/mod.rs
git commit -m "feat(server): add global RepoService"
```

---

### Task 4: Global repos API

**Files:**
- Modify: `server/src/api/repos.rs`
- Modify: `server/src/api/mod.rs` (if needed)

- [ ] **Step 1: Replace routes**

```rust
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/repos", get(list_repos).post(create_repo))
        .route(
            "/api/repos/{repo_id}",
            get(get_repo).patch(update_repo).delete(delete_repo),
        )
        .route("/api/repos/{repo_id}/verify", post(verify_repo))
}
```

- `GET` — `AuthUser` (any member)
- `POST`, `PATCH`, `DELETE`, `verify` — `AdminUser` from `middleware/admin.rs`

**RepoResponse** (camelCase):

```rust
struct RepoResponse {
    id: Uuid,
    name: String,
    local_path: String,
    remote_url: Option<String>,
    default_branch: String,
    verification_status: String,
    verification_error: Option<String>,
    last_verified_at: Option<String>,
    created_at: String,
    updated_at: String,
}
```

Map `RepoError::InUse` → 409, `DuplicatePath` → 409, `Validation` → 400.

- [ ] **Step 2: Remove project-scoped routes**

Delete handlers `list_repos`/`create_repo` that take `project_id` Path.

- [ ] **Step 3: Run existing tests — fix compile errors**

Run: `cargo build -p coppice-server`

- [ ] **Step 4: Commit**

```bash
git add server/src/api/repos.rs
git commit -m "feat(server): global repos API with verify endpoint"
```

---

### Task 5: Worktree service refactor

**Files:**
- Modify: `server/src/services/worktree_service.rs`

- [ ] **Step 1: Update `WorktreePaths` and `compute_paths`**

Remove `repo_dir` field. Signature:

```rust
pub fn compute_paths(
    worktrees_root: &Path,
    repo_name: &str,
    ticket_id: Uuid,
    agent_name: &str,
) -> WorktreePaths {
    // worktree_dir + branch_name only
}
```

Update unit test `compute_paths_builds_expected_strings` — drop `repo_dir` assertion.

- [ ] **Step 2: Remove clone machinery**

Delete: `ensure_repo_clone`, `run_git` (if only used by clone), `EmptyRemoteUrl` variant, clone tests.

- [ ] **Step 3: Simplify `WorktreeService`**

```rust
pub struct WorktreeService {
    worktrees_root: PathBuf,
}

impl WorktreeService {
    pub fn new(worktrees_root: PathBuf) -> Self { /* … */ }
    pub async fn ensure_worktree(&self, git_dir: &Path, worktree_dir: &Path, branch: &str) -> Result<(), WorktreeError>;
}
```

Keep `ensure_worktree` unchanged internally.

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server worktree -- --nocapture`

- [ ] **Step 5: Commit**

```bash
git add server/src/services/worktree_service.rs
git commit -m "refactor(server): worktree service uses registered local_path only"
```

---

### Task 6: Run service + job worker

**Files:**
- Modify: `server/src/services/run_service.rs`
- Modify: `server/src/workers/job_worker.rs`

- [ ] **Step 1: Update `start_run` preconditions**

Replace `remote_url` check with:

```rust
let repo_row = sqlx::query(
    "SELECT local_path, verification_status FROM repos WHERE id = $1",
)
.bind(repo_id)
.fetch_optional(self.pool)
.await?
.ok_or_else(|| RunError::Validation("repo not found".into()))?;

let status: String = repo_row.get("verification_status");
if status != "ready" {
    return Err(RunError::Validation("repository path is not ready".into()));
}
```

- [ ] **Step 2: Update `job_worker.rs`**

```rust
let local_path: String = repo_row.get("local_path");
let repo_name: String = repo_row.get("name");

let worktree_service = WorktreeService::new(
    state.config.agent.worktrees_path.clone().into(),
);
let paths = compute_paths(
    worktree_service.worktrees_root(),
    &repo_name,
    run.ticket_id,
    &agent.name,
);
let git_dir = PathBuf::from(&local_path);

worktree_service
    .ensure_worktree(&git_dir, &paths.worktree_dir, &paths.branch_name)
    .await?;
```

Remove `ensure_repo_clone` call and `git_repos_path` usage.

- [ ] **Step 3: Commit**

```bash
git add server/src/services/run_service.rs server/src/workers/job_worker.rs
git commit -m "feat(server): agent runs use registered repo local_path"
```

---

### Task 7: Context builder + config + compose

**Files:**
- Modify: `server/src/services/context_builder.rs`
- Modify: `server/src/config/mod.rs`
- Modify: `deploy/config/default.yaml`
- Modify: `deploy/docker-compose.yml`
- Modify: `deploy/docker-compose.local.yml`

- [ ] **Step 1: Extend `ContextInput`**

```rust
pub struct ContextInput<'a> {
    // existing fields…
    pub repo_name: Option<&'a str>,
    pub repo_remote_url: Option<&'a str>,
    pub repo_default_branch: Option<&'a str>,
    pub worktree_path: Option<&'a str>,
}
```

Add markdown section `# Repository` in `build_context_md`.

- [ ] **Step 2: Wire in job_worker** when building context.

- [ ] **Step 3: Remove `git_repos_path`**

From `AgentConfig`, `default_values()`, env merge `GIT_REPOS_PATH`, and `deploy/config/default.yaml`.

- [ ] **Step 4: Update compose files**

Remove from both compose files:
- `GIT_REPOS_PATH` env
- `repo_data` / `coppice_local_repos` volumes and mounts

Add comment in `docker-compose.local.yml` example bind mount:

```yaml
# Example: - ~/code:/data/host-repos
```

- [ ] **Step 5: Run `cargo clippy -p coppice-server -- -D warnings`**

- [ ] **Step 6: Commit**

```bash
git add server/src/services/context_builder.rs server/src/workers/job_worker.rs server/src/config/mod.rs deploy/
git commit -m "chore(server): remove git_repos_path; add repo context section"
```

---

### Task 8: Integration test helpers

**Files:**
- Modify: `server/tests/common/mod.rs`
- Modify: `server/tests/integration_agent_runs.rs`
- Modify: `server/tests/integration_workspace.rs`

- [ ] **Step 1: Add `create_temp_git_checkout`**

```rust
pub fn create_temp_git_checkout() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    // git init -b main, write README, commit (same env vars as existing helper)
    (dir, path)
}
```

Returns guard + absolute path string usable as `local_path`.

- [ ] **Step 2: Add `register_test_repo`**

```rust
pub async fn register_test_repo(
    app: &Router,
    local_path: &str,
    cookie: &str,
    csrf: &str,
) -> String {
    let body = serde_json::json!({
        "name": "test-repo",
        "localPath": local_path,
        "defaultBranch": "main",
    });
    // POST /api/repos → 201, return id
}
```

- [ ] **Step 3: Update `test_state_with_db_and_workers`**

Remove `GIT_REPOS_PATH` and `git_repos` field from `AgentTestEnv`; keep only `worktrees` tempdir + `WORKTREES_PATH`.

- [ ] **Step 4: Rewrite `integration_agent_runs.rs` setup**

```rust
let (git_dir, local_path) = common::create_temp_git_checkout();
let repo_id = common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
// drop remote_url bare repo helper
```

Keep `_git_dir` guard alive for test duration.

- [ ] **Step 5: Update `integration_workspace.rs`**

Replace `create_test_repo(&app, &project_id, …)` with checkout + `register_test_repo`.

- [ ] **Step 6: Remove obsolete helpers**

Delete or deprecate `create_test_repo_with_remote`, `create_temp_git_remote` if unused.

- [ ] **Step 7: Run integration tests**

Run: `DATABASE_URL=postgres://coppice:coppice@localhost:5433/coppice cargo test -p coppice-server --test integration_agent_runs -- --nocapture`

Expected: PASS (requires Postgres + git)

- [ ] **Step 8: Commit**

```bash
git add server/tests/
git commit -m "test(server): integration tests use registered local_path repos"
```

---

### Task 9: Web — Repositories page

**Files:**
- Create: `web/src/lib/schemas/repo.ts`
- Create: `web/src/features/repos/useRepos.ts`
- Create: `web/src/features/repos/RepositoriesPage.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/components/AppShell.tsx`

- [ ] **Step 1: Zod schema**

```typescript
// web/src/lib/schemas/repo.ts
export const verificationStatusSchema = z.enum([
  'ready', 'path_missing', 'not_git_repo', 'error',
]);

export const repoSchema = z.object({
  id: z.string().uuid(),
  name: z.string(),
  localPath: z.string(),
  remoteUrl: z.string().nullable(),
  defaultBranch: z.string(),
  verificationStatus: verificationStatusSchema,
  verificationError: z.string().nullable(),
  lastVerifiedAt: z.string().nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
});
```

- [ ] **Step 2: Hooks**

`useRepos()` — `GET /api/repos`

Mutations (admin): `useCreateRepo`, `useUpdateRepo`, `useDeleteRepo`, `useVerifyRepo`

Follow `useUsers.ts` patterns; invalidate `['repos']` on success.

- [ ] **Step 3: `RepositoriesPage`**

- List table with status pills (color by status)
- Admin: create/edit form (name, localPath, remoteUrl, defaultBranch)
- Verify button per row
- M07 placeholder card: "Secrets for pull requests — coming in M07"
- Member (non-admin): read-only list; hide create form (use `useSession().user?.role`)

- [ ] **Step 4: Routing**

`App.tsx`: `<Route path="/settings/repositories" element={<RepositoriesPage />} />`

`AppShell.tsx`: nav link visible to all authenticated users (or admin-only for link — spec says members read-only list, so show link to all)

- [ ] **Step 5: Run web tests**

Run: `make web-test`

- [ ] **Step 6: Commit**

```bash
git add web/src/
git commit -m "feat(web): add Settings Repositories page"
```

---

### Task 10: Ticket repo picker + drawer hints

**Files:**
- Modify: `web/src/features/tickets/TicketMetadataTab.tsx`
- Modify: `web/src/features/tickets/TicketDrawer.tsx` (optional hint)

- [ ] **Step 1: Add repo picker to Metadata tab**

Import `useRepos()` from `../repos/useRepos`.

Add select for `repoId` on ticket (PATCH ticket with `repoId` — verify `UpdateTicketBody` supports `repoId`; if not, add to API).

If `repoId` missing from update API, add `repo_id` to `ticket_service.update_fields` and `UpdateTicketBody` in same task.

- [ ] **Step 2: Run Agent hint**

When ticket has `repoId` but selected repo `verificationStatus !== 'ready'`, show: "Repository path is not ready. Ask an admin to verify in Settings → Repositories."

- [ ] **Step 3: Run `make web-test`**

- [ ] **Step 4: Commit**

```bash
git add web/src/features/tickets/ server/src/api/tickets.rs server/src/services/ticket_service.rs
git commit -m "feat(web): global repo picker on ticket metadata"
```

---

### Task 11: E2E smoke revision

**Files:**
- Modify: `e2e/smoke/m03-agent-run.mjs`
- Optionally: `e2e/smoke/setup-test-repo.mjs` helper

- [ ] **Step 1: Setup local git checkout in smoke**

Before registering repo, use Node `child_process.execSync`:

```javascript
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execSync } from 'node:child_process';

function createSmokeGitCheckout() {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-smoke-repo-'));
  execSync('git init -b main', { cwd: dir });
  writeFileSync(join(dir, 'README.md'), '# smoke\n');
  execSync('git add README.md && git commit -m "init"', {
    cwd: dir,
    env: { ...process.env, GIT_AUTHOR_NAME: 'smoke', GIT_AUTHOR_EMAIL: 'smoke@test' },
  });
  return dir;
}
```

**Docker note:** smoke runs against compose server; `localPath` must be a path **inside the server container**. For CI, mount a host temp dir or use a fixed path like `/data/smoke-repo` with a compose init step.

**Pragmatic CI approach:** add to `docker-compose.yml` server service:

```yaml
volumes:
  - smoke_repo:/data/smoke-repo
```

Smoke script writes to `/data/smoke-repo` only works if script runs **inside** container — instead, use **API-only** path that matches a pre-created volume:

1. Add `deploy/scripts/init-smoke-repo.sh` run in Dockerfile or compose command wrapper, OR
2. Smoke uses `docker compose exec server` to create repo at `/tmp/smoke-repo` then registers `/tmp/smoke-repo` via API.

**Simplest for M03 retcon:** smoke script documents that for local `make e2e-smoke-m03`, run:

```bash
docker compose exec server sh -c 'mkdir -p /tmp/smoke-repo && cd /tmp/smoke-repo && git init -b main && echo hi > README.md && git add . && git commit -m init'
```

Then API `POST /api/repos` with `localPath: "/tmp/smoke-repo"`.

Implement `ensureSmokeRepo(api, auth)` in m03 script that execs via documented env `COPPICE_SMOKE_REPO_PATH` default `/tmp/smoke-repo`, with setup function called from compose CI job before node script OR inline `docker compose exec` in Makefile target:

```makefile
e2e-smoke-m03: compose-up
	docker compose -f deploy/docker-compose.yml exec -T server sh -c '...init git...'
	node e2e/smoke/m03-agent-run.mjs
```

- [ ] **Step 2: Replace `REMOTE_URL` repo create with `POST /api/repos`**

- [ ] **Step 3: Run smoke locally**

Run: `make e2e-smoke-m03`

- [ ] **Step 4: Commit**

```bash
git add e2e/smoke/m03-agent-run.mjs Makefile deploy/
git commit -m "test(e2e): smoke uses registered local_path repository"
```

---

### Task 12: Docs + final verification

**Files:**
- Modify: `AGENTS.md`
- Modify: `docs/superpowers/specs/2026-06-08-m03-registered-repositories-design.md` (status → Implemented)
- Modify: `docs/superpowers/plans/2026-06-08-m03-agent-execution.md` (header note: obsolete for repos)

- [ ] **Step 1: Update AGENTS.md**

Status: M03 retcon complete. Remove `GIT_REPOS_PATH` from env list.

- [ ] **Step 2: Full checklist**

```bash
make migrate-local
cargo clippy --workspace -- -D warnings
cargo test --workspace
make web-test
make e2e-smoke-m03
```

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md docs/
git commit -m "docs: mark M03 registered repositories retcon complete"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| `local_path` required, `remote_url` optional | 1, 3 |
| Drop `project_id` — global repos | 1, 3 |
| Verification status + verify endpoint | 2, 3, 4 |
| Admin-only mutations | 4, 9 |
| Remove lazy clone | 5, 6 |
| Worker uses `local_path` | 6 |
| Run requires `ready` status | 6 |
| Central worktrees | 5, 6 |
| Context Repository section | 7 |
| Remove `GIT_REPOS_PATH` / `repo_data` | 7 |
| Integration tests register path | 8 |
| Repositories UI | 9 |
| Ticket repo picker | 10 |
| E2E smoke register path | 11 |
| Docs | 12 |

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-08-m03-registered-repositories.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration

2. **Inline Execution** — implement tasks in this session with checkpoints for review

**Which approach?**
