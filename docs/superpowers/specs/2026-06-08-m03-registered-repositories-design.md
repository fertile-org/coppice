# M03 Retcon — Registered Repositories Design Spec

**Date:** 2026-06-08  
**Status:** Approved  
**Implementation plan:** [docs/superpowers/plans/2026-06-08-m03-registered-repositories.md](../plans/2026-06-08-m03-registered-repositories.md)  
**Product:** Coppice — grow an agent team from shared roots.

**Supersedes (partial):** [2026-06-08-m03-agent-execution-design.md](./2026-06-08-m03-agent-execution-design.md) — repo bootstrap, clone paths, `GIT_REPOS_PATH`, and project-scoped repos.

**Preserves from original M03:** job queue, worker, worktrees, context package, result contract, Runs tab, Run Agent / Stop, MockProvider, permissive sandbox placeholder.

**Milestone doc:** `docs/milestones/M03-agent-execution.md`

## Purpose

The first M03 implementation used **lazy `git clone`** from `repos.remote_url` into Coppice-managed `/data/repos/{repo-id}/`. That model is hard to reason about (credentials, visibility, control) and does not match self-hosted reality: operators already clone repos on the machine.

This retcon replaces lazy clone with **registered local repositories**:

1. Admin clones (or already has) a git checkout on the **Coppice server** filesystem.
2. Admin registers it in **Settings → Repositories** with an absolute `local_path`.
3. Agent runs create **worktrees** from that path; Coppice never runs `git clone`.
4. Optional `remote_url` is metadata for display, agent context, and future PR APIs (M07).
5. CI and integration tests use the **same pattern**: create a temp git repo on disk, register `local_path`, then run agent.

One model only — no dual lazy-clone path.

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Milestone packaging | **Retcon M03** — registered repos + agent execution in one milestone revision |
| Clone model | **Local path only** — owner clones; Coppice never `git clone`s |
| Repo fields | **`local_path` required**; **`remote_url` optional** (display, context, future PR) |
| Path security | **Trust admin** — validate path exists and is a valid git repo; no allowlist roots |
| Repo scope | **Instance-wide** — any project's tickets can link any registered repo |
| Repo UI | **Global Settings → Repositories** — admin only for create/edit/delete |
| Worktrees | Central **`WORKTREES_PATH`** (e.g. `/data/worktrees`) — not inside operator clone |
| CI / tests | Temp `git init` checkout → register `local_path` → run agent (no `file://` bare remote on worker) |
| Implementation | **Unified retcon** — schema, API, worker, UI, tests, compose in one coherent pass |

### Unchanged from original M03

| Topic | Decision |
|-------|----------|
| Result side effects | `nextStatus` + agent comment; `mentionAgents` on comment `mentions` only (M05 for mention jobs) |
| Run preconditions | Ticket must have `assignee_agent_id` and linked repo with **ready** `local_path` |
| Parallelism | Different agents may run in parallel on the same ticket |
| Worktree isolation | One worktree per `(ticket, agent)` |
| Same-agent overlap | Reject new run if active run exists for `(ticket_id, agent_id)` |
| Blocked results | Hybrid `nextStatus` + `blockerType` → substatus |
| Runs UI | Header Run Agent / Stop; Runs tab read-only history |
| Worker | In-process Tokio; `agent_jobs` with `FOR UPDATE SKIP LOCKED` |

---

## Architecture overview

### Repository model

```text
Operator machine (or host)
  ~/code/my-app/          ← git checkout (operator manages clone/auth)

Coppice server container
  local_path = /data/host-repos/my-app   ← bind-mounted from host
  WORKTREES_PATH/TICKET-{id}-{agent}-{repo}/   ← Coppice-created worktrees
```

**Important:** `local_path` is absolute on the **server process** (inside the container when using Docker). Operators bind-mount host directories into the container and register the in-container path.

### Agent run pipeline (revised)

```text
POST /api/tickets/:id/run-agent
  → validate assignee + repo_id
  → validate repo.local_path exists and is valid git repo
  → create agent_run (queued) + agent_job
  → worker claims job
  → mark run running
  → git_dir = canonicalize(repo.local_path)
  → ensure worktree under WORKTREES_PATH (reuse if exists)
  → write .agent/context.md (includes repo name, remote_url if set)
  → AgentProvider::run
  → result_contract::apply_agent_result
  → finish run; update ticket; agent comment
```

**Removed stages:** `ensure_repo_clone`, `GIT_REPOS_PATH/{repo-id}/`, `repo_data` volume.

### Monorepo delta (retcon)

```text
server/migrations/004_registered_repos.sql
server/src/
  services/repo_service.rs          # new or extracted from project_service
  services/worktree_service.rs      # remove clone; worktree from local_path
  workers/job_worker.rs             # use repo.local_path
  services/run_service.rs             # precondition: local_path ready
  api/repos.rs                        # global routes; admin mutations
web/src/features/repos/
  RepositoriesPage.tsx
  useRepos.ts
web/src/App.tsx                     # /settings/repositories
deploy/docker-compose*.yml          # remove repo_data; keep worktree_data
e2e/smoke/m03-agent-run.mjs         # register path, not public clone URL
server/tests/common/mod.rs          # temp git dir + register repo helper
```

---

## Data model

### `repos` table (revised)

| Column | Type | Notes |
|--------|------|-------|
| `id` | UUID PK | unchanged |
| `name` | TEXT NOT NULL | display name |
| `local_path` | TEXT NOT NULL | absolute path on server; **unique** |
| `remote_url` | TEXT NULL | optional; never used for clone |
| `default_branch` | TEXT NOT NULL | default `main` |
| `verification_status` | TEXT NOT NULL | `ready`, `path_missing`, `not_git_repo`, `error` |
| `verification_error` | TEXT NULL | last verify failure message |
| `last_verified_at` | TIMESTAMPTZ NULL | |
| `created_at` | TIMESTAMPTZ NOT NULL | |
| `updated_at` | TIMESTAMPTZ NOT NULL | |

**Removed:** `project_id` — repos are instance-wide.

**Migration `004_registered_repos.sql`:**

1. Add new columns with defaults for existing rows (dev/CI may truncate).
2. Backfill: tests drop and recreate repos or set `local_path` in fixtures.
3. Drop `project_id` FK and column.
4. Add unique index on `local_path`.
5. Drop project-scoped repo indexes if any.

### Domain (`server/src/domain/repo.rs`)

```rust
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
```

`tickets.repo_id` remains optional FK to `repos.id` — any global repo.

---

## Repository verification

### When verification runs

- On **create** and **update** when `local_path` changes (sync verify before save).
- On **POST /api/repos/:id/verify** (explicit re-check).
- Optionally on **run-agent** if status is stale (fast path: trust `ready` if `last_verified_at` recent; or always quick-check `Path::exists` + `.git`).

### Verification rules

1. `local_path` is non-empty.
2. Path exists on filesystem (from server's perspective).
3. Path is a git repository: `.git` file or directory present, or `git rev-parse --git-dir` succeeds.
4. Optional warning (non-blocking): if `remote_url` set, compare `git remote get-url origin` when origin exists — log mismatch in `verification_error` or UI warning only.

### No path allowlist

Admin-entered paths are accepted if verification passes. Document in operator guide: use bind mounts in Docker; path must be visible inside server container.

### Status values

| Status | Meaning |
|--------|---------|
| `ready` | Path exists and is valid git repo |
| `path_missing` | Path does not exist |
| `not_git_repo` | Path exists but not a git repo |
| `error` | Git command or IO failure |

**Run Agent** requires linked repo with `verification_status = ready`.

---

## API

### Global repositories (replace project-scoped routes)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/repos` | Member | List all registered repos |
| POST | `/api/repos` | Admin | Create repo |
| GET | `/api/repos/:id` | Member | Get repo |
| PATCH | `/api/repos/:id` | Admin | Update repo |
| DELETE | `/api/repos/:id` | Admin | Delete if no tickets reference (or soft-delete policy: block delete if in use) |
| POST | `/api/repos/:id/verify` | Admin | Re-run verification |

**Removed:**

- `GET/POST /api/projects/:project_id/repos`

### Request/response (camelCase JSON)

**CreateRepoBody:**

```json
{
  "name": "my-app",
  "localPath": "/data/host-repos/my-app",
  "remoteUrl": "https://github.com/org/my-app.git",
  "defaultBranch": "main"
}
```

**RepoResponse** includes `localPath`, `remoteUrl`, `verificationStatus`, `verificationError`, `lastVerifiedAt`.

### Run-agent precondition errors

| Condition | HTTP |
|-----------|------|
| No assignee | 400 |
| No repo on ticket | 400 |
| Repo `local_path` not ready | 400 with message e.g. "repository path is not ready" |
| Active run exists | 409 |

---

## Worktree service (revised)

### `compute_paths` signature change

```rust
pub fn compute_paths(
    worktrees_root: &Path,
    repo_name: &str,
    ticket_id: Uuid,
    agent_name: &str,
) -> WorktreePaths {
    // worktree_dir under worktrees_root
    // branch_name: agent/TICKET-{short}-{agent-slug}
    // repo_dir removed — caller passes repo.local_path separately
}
```

### `WorktreeService` methods

- **Remove:** `ensure_repo_clone`
- **Keep:** `ensure_worktree(git_dir: &Path, worktree_dir: &Path, branch: &str)`
- **Constructor:** only `worktrees_root` (drop `repos_root`)

### Worktree layout (unchanged naming)

```text
{WORKTREES_PATH}/TICKET-{ticket-short}-{agent-slug}-{repo-slug}/
branch: agent/TICKET-{ticket-short}-{agent-slug}
```

Git directory for worktree add: `repo.local_path` (the registered checkout).

---

## Context package

Add to `.agent/context.md` **Repository** section:

- Registered name
- `local_path` (worktree path used for execution — prefer run's worktree path over bare repo path in context)
- `remote_url` if set
- `default_branch`

Agents and operators share the same repo registry — tickets link by `repo_id`.

---

## Web UI

### Settings → Repositories (`/settings/repositories`)

- Nav link next to **Users** in AppShell (admin-visible or visible to all with read-only for members).
- **Admin:** create, edit, delete, verify.
- **Member:** read-only list (for ticket repo picker context).

### List columns

Name, local path (truncated + tooltip), remote URL, default branch, status pill, last verified.

### Create/Edit form

- Name (required)
- Local path (required) — helper text: "Absolute path on the Coppice server. In Docker, use the path inside the container (bind-mount host clones)."
- Remote URL (optional)
- Default branch (default `main`)
- **Verify** button

### Future placeholder (M07)

Read-only section: **Secrets** — "GitHub token for pull requests — available in M07." No backend in this retcon.

### Ticket flows

- Repo picker on ticket create/edit/metadata: all repos from `GET /api/repos`.
- Run Agent disabled when `!assigneeAgentId || !repoId`.
- Drawer hint when repo not ready: link to Repositories settings (admin) or "ask admin to verify repository."

---

## Docker Compose (revised)

### Remove

- Volume `repo_data` / `coppice_local_repos`
- Env `GIT_REPOS_PATH`

### Keep

- `worktree_data` → `/data/worktrees`
- `artifact_data` → `/data/artifacts`
- `git` in server image (for `worktree` commands)
- Agent worker env: `WORKTREES_PATH`, `AGENT_WORKER_COUNT`, `AGENT_DEFAULT_PROVIDER`

### Operator documentation

Example bind mount for local dev:

```yaml
server:
  volumes:
    - ~/code:/data/host-repos:ro   # or rw if agents push later
```

Register `local_path: /data/host-repos/my-app` in UI.

---

## Config (`AppConfig`)

**Remove from `AgentConfig`:**

- `git_repos_path`

**Keep:**

- `worktrees_path`
- `default_provider`
- `worker_count`

**Remove from `deploy/config/default.yaml`:** `agent.git_repos_path`

---

## Milestone document updates

| Document | Change |
|----------|--------|
| `M03-agent-execution.md` | Add registered repositories + admin UI; remove lazy clone acceptance criteria |
| `M02-workspace-and-board.md` | Note: repo registry completed in M03 retcon; project-scoped repo API removed |
| `coppice-milestone-strategy-design.md` | Compose: `/data/worktrees` only for Coppice-managed paths; operator repos via bind mount |
| `M07-trust-and-signals.md` | PR: use repo `remote_url` + per-repo secret on Repositories page |
| `architecture.md`, `AGENTS.md` | Registered-repo model |

Original M03 implementation plan (`2026-06-08-m03-agent-execution.md`) is **obsolete** for repo sections; new implementation plan to be written after this spec.

---

## Testing strategy

### Unit

- Path verification: missing path, non-git dir, valid `git init` dir
- `compute_paths` without `repo_dir`
- Run precondition rejects non-ready repo

### Integration

Replace bare-remote clone helper with:

```rust
pub fn create_temp_git_checkout() -> (TempDir, PathBuf) {
    // git init in temp dir, initial commit
    // return (guard, absolute_path)
}

pub async fn register_test_repo(app, path, cookie, csrf) -> Uuid {
    // POST /api/repos { name, localPath: path, ... }
}
```

Existing agent run tests updated to register path before `run-agent`.

### E2E smoke (`m03-agent-run.mjs`)

1. Shell or API setup: clone `Hello-World` to temp dir **outside** Coppice (or use compose-mounted test dir).
2. `POST /api/repos` with that `localPath`.
3. Create ticket, assign, link repo, run-agent, poll, assert comment.

No dependency on Coppice performing network clone during the run.

### CI

No `GIT_REPOS_PATH` in test env. Integration tests set `WORKTREES_PATH` to temp dir (unchanged).

---

## M03 code revision checklist

| Area | Action |
|------|--------|
| `worktree_service.rs` | Remove clone; simplify paths |
| `job_worker.rs` | Use `repo.local_path` |
| `run_service.rs` | Validate `local_path` ready, not `remote_url` |
| `project_service.rs` | Remove repo CRUD or delegate to `repo_service` |
| `api/repos.rs` | Global routes + verify |
| `api/mod.rs`, `api/tickets.rs` | Wire changes |
| `config/mod.rs`, `default.yaml` | Remove `git_repos_path` |
| `deploy/docker-compose*.yml` | Remove `repo_data` |
| `web` | Repositories page + ticket repo picker |
| `e2e/smoke/m03-agent-run.mjs` | Register path flow |
| `server/tests/common/mod.rs` | New helpers |
| Docs | This spec + milestone updates |

---

## M07 forward compatibility

Repositories page reserves **Secrets** section. M07 adds:

- `repos.github_token_secret_id` or join to secrets table
- Create PR uses `remote_url` to parse `owner/repo` + injected token

No secret storage in this retcon.

---

## Acceptance criteria (M03 retcon)

- [ ] Admin can register a repo with `local_path` and optional `remote_url`
- [ ] Verify detects missing/invalid paths and marks repo ready when valid
- [ ] Any project's ticket can link any registered repo
- [ ] Agent run creates worktree from `local_path`; no `git clone` in worker
- [ ] Run Agent fails clearly when repo path not ready
- [ ] Lazy clone code and `GIT_REPOS_PATH` / `repo_data` removed
- [ ] Integration + E2E smoke use register-path pattern
- [ ] Original M03 acceptance (runs, result contract, stop/retry, Runs UI) still passes

---

## References

- Superseded clone sections: [2026-06-08-m03-agent-execution-design.md](./2026-06-08-m03-agent-execution-design.md)
- Product design §11 (worktrees), §22 (`/repos` API)
- M07 git/PR: `docs/milestones/M07-trust-and-signals.md`
