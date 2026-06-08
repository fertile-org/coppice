# M07 — Trust & Signals

## Goal

Production-ready trust boundaries: capabilities, sandbox profiles, encrypted secrets, capability blockers with guided unblock, proactive workspace signals, and minimal git/PR actions. After this milestone, Coppice matches the full product design and is ready for daily use.

## Product scope

### Capabilities, sandbox, secrets

- `Capability`, `SandboxProfile`, `Secret` models (product design §14)
- Capability resolver: agent.capabilityIds + sandbox profile → allowed commands, paths, network hosts, secrets
- Process-level sandbox v1: command allowlist wrapper, env injection, timeout, output limits, audit log
- Secrets encrypted at rest (age or ring); injected into run env only when allowed — never in prompts/comments
- Missing-capability blocker flow with guided unblock UI (product design §14.6)
- Admin screens: Capabilities, Sandbox Profiles, Secrets (names/scopes only after creation)
- Default sandbox profiles per agent preset (product design §18.1)
- Replace permissive sandbox from M03 with profile-driven enforcement

### Proactive signals

- `WorkspaceSignal` model (product design §15.2)
- Workspace Inbox / Signals screen
- Job types: `observe_domain`, `inspect_health`, `create_signal`
- Manual **Run Observation** button per agent (scheduled cron out of scope)
- Actions: Create Ticket, Acknowledge, Dismiss, Snooze, Grant Capability, Add Secret
- Anti-spam: max signals per agent per day, dedup window, evidence + recommendation required (product design §15.6)
- Convert signal to ticket with `sourceSignalId` link

### Git / PR (minimal)

- View diff summary artifact from worktree (under `WORKTREES_PATH`, from registered repo `local_path`)
- Push branch (requires explicit human-triggered action + config flag)
- Create PR via GitHub API using repo `remote_url` + **per-repo secret** on Settings → Repositories (M03 retcon UI placeholder); no auto-merge
- Sensitive actions audit log

## Out of scope

- Container sandbox v2 (future)
- Scheduled observation cron
- GitLab integration (GitHub first)
- Kubernetes runner
- Autonomous merge/deploy

## Dependencies

- M01–M06: all prior functionality
- M03 runs must migrate from permissive to profile-based sandbox

## Architecture notes

### New server modules

```text
server/src/
  sandbox/
    command_wrapper.rs
    policy.rs
    permissive.rs         # removed or test-only
  services/
    capability_service.rs
    secret_service.rs
    signal_service.rs
    git_service.rs
  api/
    capabilities.rs
    sandbox_profiles.rs
    secrets.rs
    signals.rs
  crypto/
    secret_store.rs
```

### New database tables

```text
capabilities
sandbox_profiles
secrets
workspace_signals
signal_dedup_keys
audit_log
```

### Blocker flow

```json
{
  "status": "blocked",
  "blockerType": "missing_capability",
  "requiredSecret": "DB_READONLY_URL",
  "mentionAgents": ["owner"]
}
```

UI shows: Allow command | Add secret | Grant capability | Reject | Ask agent why

### API endpoints

```text
GET/POST/PATCH  /api/capabilities
GET/POST/PATCH  /api/sandbox-profiles
GET/POST        /api/secrets
DELETE          /api/secrets/:id

GET             /api/signals
GET             /api/signals/:id
POST            /api/signals/:id/acknowledge
POST            /api/signals/:id/dismiss
POST            /api/signals/:id/snooze
POST            /api/signals/:id/convert-to-ticket
POST            /api/agents/:id/run-observation

POST            /api/tickets/:id/view-diff
POST            /api/tickets/:id/push-branch
POST            /api/tickets/:id/create-pr

POST            /api/blockers/:id/grant-capability
POST            /api/blockers/:id/add-secret
```

## Docker Compose delta

```yaml
  server:
    environment:
      SECRETS_MASTER_KEY: ${SECRETS_MASTER_KEY:-dev-master-key-change-me}
      GITHUB_TOKEN: ${GITHUB_TOKEN:-}          # optional for PR tests
      SANDBOX_ENFORCE: "true"
    volumes:
      - ./deploy/capabilities.yaml:/etc/coppice/capabilities.yaml:ro
      - ./deploy/sandbox-profiles.yaml:/etc/coppice/sandbox-profiles.yaml:ro
```

Optional `postgres-readonly` service for DBA agent integration tests:

```yaml
  postgres-readonly:
    image: postgres:16
    profiles: ["dba-test"]
    # readonly user for capability integration tests
```

Use compose profiles so default `docker compose up` stays minimal.

## Testing strategy

### Unit tests

- Sandbox policy: allowed command passes; `rm`, `curl` denied
- Secret scope: agent not in allowedAgentIds → injection blocked
- Capability resolver merges agent + profile
- Signal dedup: duplicate title+agent within window updates existing
- Anti-spam: 4th signal same day rejected

### Integration tests

- Agent run without `psql` in profile → blocked result → grant capability + secret via API → resume → success
- DBA mock observation creates signal with evidence → convert to ticket → ticket has sourceSignalId
- Secret value never appears in comment body or API JSON responses
- Push branch blocked when config disallows; enabled with flag + human action
- Audit log entries for secret grant and push

### E2E smoke (CI)

`e2e/smoke/m07-trust.spec`:

1. Configure FE agent with restrictive sandbox (no `pnpm`)
2. Run agent → blocked badge with missing command message
3. Grant capability via guided unblock UI
4. Retry run → succeeded

`e2e/smoke/m07-signals.spec`:

1. Run Observation on DBA mock agent
2. Signal appears in Workspace Inbox
3. Create Ticket from signal → ticket on board

### E2E full (local)

- Secrets screen: create secret, verify value hidden after save
- Sandbox profile editor save and apply to agent
- Snooze/dismiss signal
- View diff on ticket with worktree changes
- Create PR against test repo (requires GITHUB_TOKEN locally)

## Acceptance criteria

- [ ] Sandbox enforces command/path/network/secret policy on all runs
- [ ] Capability blockers show guided unblock; grant unblocks resume
- [ ] Secrets encrypted; never leaked in API or comments
- [ ] Workspace Inbox shows proactive signals with evidence
- [ ] Convert signal to ticket works
- [ ] Manual Run Observation works for role-owner agents
- [ ] View diff / push branch / create PR available with human trigger
- [ ] Full product design §1–27 covered for v1 scope
- [ ] `docker compose up` yields complete Coppice ready for daily use
- [ ] CI smoke E2E passes

## References

- Product design §14 (capabilities, sandbox, secrets), §15 (proactive signals), §18 (permissions), §26 (end-to-end scenarios)
- Product design §24 Phase 6–9
- Framework selection §4 (sandbox v1), §2 (secrets encryption)

## v1 complete

When M07 acceptance criteria pass, Coppice implements the full philosophy product design for self-hosted v1. Real CLI provider adapters and scheduled observations can follow as post-v1 configuration work without new milestones.
