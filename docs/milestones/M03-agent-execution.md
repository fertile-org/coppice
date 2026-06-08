# M03 — Agent Execution & Registered Repositories

## Goal

Agents execute work on tickets through the job queue and mock provider, using **admin-registered local git repositories** and Coppice-managed worktrees. Includes global repository registry (Settings UI), context packages, and machine-readable result contracts driving ticket updates and comments.

**Design spec:** [docs/superpowers/specs/2026-06-08-m03-registered-repositories-design.md](../superpowers/specs/2026-06-08-m03-registered-repositories-design.md)

## Product scope

### Registered repositories (retcon)

- Instance-wide `repos` registry — not project-scoped
- Admin **Settings → Repositories**: register name, `local_path`, optional `remote_url`, default branch
- Path verification: exists + valid git repo; status pill (`ready`, `path_missing`, `not_git_repo`, `error`)
- Any project's tickets can link any registered repo
- **No lazy clone** — Coppice never runs `git clone`; operator clones on the host and bind-mounts into the server container when using Docker

### Agent execution (unchanged scope)

- Postgres-backed `agent_jobs` queue and background worker
- Job types: `work_on_ticket` (others in later milestones)
- Git worktree lifecycle from registered `local_path`; branch `agent/TICKET-{id}-{agent-slug}`
- Agent run lifecycle: queued → running → succeeded | failed | blocked | cancelled
- Context package generation (`.agent/context.md`) with ticket, role, repo, rules, sandbox note
- Result contract parser (product design §17)
- Agent-authored comments from run output
- Ticket detail: **Runs** tab; header **Run Agent** / **Stop**
- `MockProvider` as default compose provider
- Permissive default sandbox profile (until M07)

## Out of scope

- Per-repo secrets / GitHub PR (M07 — UI placeholder only)
- Live terminal streaming (M04)
- Workflow rule engine and mention jobs (M05)
- Knowledge injection into context (M06)
- Strict capability/sandbox enforcement (M07)
- Real CLI adapters
- Path allowlist roots (trust admin + git validation)

## Dependencies

- M01: MockProvider, auth, Postgres
- M02: tickets, agents, comments, projects

## Architecture notes

### Server modules

```text
server/src/
  workers/job_worker.rs
  services/
    repo_service.rs           # global repo CRUD + verify
    job_service.rs
    run_service.rs
    worktree_service.rs       # worktree only (no clone)
    context_builder.rs
    result_contract.rs
  sandbox/permissive.rs
  api/repos.rs                # global /api/repos
  api/agent_runs.rs, jobs.rs
```

### Database tables

```text
repos                         # revised: local_path, no project_id
agent_jobs
agent_runs
```

### API endpoints

```text
GET/POST       /api/repos
GET/PATCH/DELETE /api/repos/:id
POST           /api/repos/:id/verify

POST  /api/tickets/:id/run-agent
GET   /api/tickets/:id/runs
GET   /api/agent-runs/:id
POST  /api/agent-runs/:id/stop
POST  /api/agent-runs/:id/retry
GET   /api/agent-jobs         (admin/debug)
```

**Removed:** `GET/POST /api/projects/:id/repos`

### Filesystem layout

```text
{repo.local_path}/                    ← operator-managed checkout (bind-mounted in Docker)
{WORKTREES_PATH}/TICKET-{id}-{agent}-{repo-slug}/   ← Coppice worktrees
```

## Docker Compose delta

```yaml
  server:
    volumes:
      - artifact_data:/data/artifacts
      - worktree_data:/data/worktrees
      # operator bind-mounts host clones; e.g. ~/code:/data/host-repos
    environment:
      AGENT_DEFAULT_PROVIDER: mock
      WORKTREES_PATH: /data/worktrees
      AGENT_WORKER_COUNT: 2

volumes:
  worktree_data:
```

Server image needs `git` CLI (worktree commands). **No `repo_data` volume.**

## Testing strategy

### Unit

- Repo path verification
- Worktree path naming (no `repo_dir` under Coppice clone root)
- Result contract, context package, job state machine

### Integration

- `git init` temp checkout → register via `POST /api/repos` → run-agent → succeeded
- Blocked fixture, duplicate run 409, stop, retry
- Worktree created from registered path

### E2E smoke

`e2e/smoke/m03-agent-run.mjs`: clone to temp dir (or use mounted test dir) → register `localPath` → run-agent → assert comment.

## Acceptance criteria

- [ ] Admin can register and verify repositories with `local_path`
- [ ] Tickets in any project can use any registered repo
- [ ] Agent run uses worktree from registered path (no server-side clone)
- [ ] Mock agent run completes end-to-end
- [ ] Result contract drives comment + status/substatus update
- [ ] Stop and retry work via API and UI
- [ ] All automated tests use MockProvider only
- [ ] CI smoke E2E passes

## References

- [Registered repositories design spec](../superpowers/specs/2026-06-08-m03-registered-repositories-design.md)
- Product design §10 (runs), §11 (worktrees), §16 (context), §17 (result contract)
- Framework selection §2 (job queue, git CLI)
