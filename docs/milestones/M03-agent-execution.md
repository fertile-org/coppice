# M03 — Agent Execution

## Goal

Agents execute work on tickets through the job queue and mock provider, with git worktrees, context packages, and machine-readable result contracts driving ticket updates and comments.

## Product scope

- Postgres-backed `agent_jobs` queue and background worker
- Job types: `work_on_ticket` (others in later milestones)
- Git worktree lifecycle: branch `agent/TICKET-{id}`, isolated worktree path
- Agent run lifecycle: queued → running → succeeded | failed | blocked | cancelled
- Context package generation (`.agent/context.md`) with ticket, role, rules, sandbox note
- Result contract parser (product design §17): done, blocked, mentionAgents, nextStatus, changedFiles, etc.
- Agent-authored comments from run output (intent: progress_update, implementation_done, blocked, …)
- Ticket detail: **Runs** tab; actions Run Agent, Stop Run
- `MockProvider` as default compose provider — returns scripted JSON + optional stdout
- Permissive default sandbox profile (wide command/path allowlist until M07)

## Out of scope

- Live terminal streaming (M04)
- Workflow rule engine and mention jobs (M05)
- Knowledge injection into context (M06)
- Strict capability/sandbox enforcement (M07)
- Real CLI adapters

## Dependencies

- M01: MockProvider, auth, Postgres
- M02: tickets, agents, comments, repos

## Architecture notes

### New server modules

```text
server/src/
  workers/
    job_worker.rs
  services/
    job_service.rs
    run_service.rs
    worktree_service.rs
    context_builder.rs      # basic ticket+agent sections only
    result_contract.rs
  providers/
    mock.rs                 # extended: emit stdout for M04 prep
  sandbox/
    permissive.rs           # placeholder until M07
  api/
    agent_runs.rs
    jobs.rs
```

### New database tables

```text
agent_jobs
agent_runs
```

### API endpoints

```text
POST  /api/tickets/:id/run-agent
GET   /api/tickets/:id/runs
GET   /api/agent-runs/:id
POST  /api/agent-runs/:id/stop
POST  /api/agent-runs/:id/retry
GET   /api/agent-jobs         (admin/debug)
```

### AgentRun fields

Per product design §10.2: ticketId, agentId, jobType, status, sandboxProfileId, worktreePath, timestamps, usedCapabilityIds (empty until M07).

### Worktree layout

```text
/data/repos/{repo-id}/
/data/worktrees/TICKET-{id}-{repo-slug}/
```

## Docker Compose delta

**Added in M03:**

```yaml
  server:
    volumes:
      - artifact_data:/data/artifacts
      - worktree_data:/data/worktrees
      - repo_data:/data/repos
    environment:
      AGENT_DEFAULT_PROVIDER: mock
      GIT_REPOS_PATH: /data/repos
      WORKTREES_PATH: /data/worktrees

volumes:
  worktree_data:
  repo_data:
```

Server image needs `git` CLI installed.

## Testing strategy

### Unit tests

- Context package renders expected markdown sections
- Result contract parser: all JSON variants in product design §17
- Job state machine transitions
- Worktree path naming and branch slug sanitization

### Integration tests

- Assign agent → POST run-agent → job enqueued → worker picks up → MockProvider returns `done` → ticket comment created → run status succeeded
- MockProvider returns `blocked` → ticket substatus updated → comment with blocker text
- Worktree created on disk; branch exists in repo clone
- Stop run cancels in-flight job
- Retry creates new run linked to same ticket

### E2E smoke (CI)

`e2e/smoke/m03-agent-run.spec`:

1. Login → open ticket → assign mock-configured agent
2. Click Run Agent → wait for run succeeded badge
3. Verify agent comment appears in Comments tab
4. Runs tab shows completed run

### E2E full (local)

- Stop mid-run
- Retry after failure
- Metadata tab shows worktree path and branch name

## Acceptance criteria

- [ ] Mock agent run completes end-to-end on a ticket
- [ ] Result contract drives comment + status/substatus update
- [ ] Worktree and branch created per ticket
- [ ] Stop and retry work via API and UI
- [ ] All automated tests use MockProvider only
- [ ] CI smoke E2E passes

## References

- Product design §7 (provider abstraction), §10 (runs), §11 (worktrees), §16 (context package), §17 (result contract)
- Framework selection §2 (job queue, git CLI, process execution)
