# M11 — Security & Sandbox

## Goal

Production-ready trust boundaries for agent runs and plugins: capabilities, sandbox profiles, scoped secret injection, capability blockers with guided unblock, and an audit log. After M11, every process an agent starts and every tool it calls (M10) is checked against policy.

Carried over from the original M07 "Trust & signals" scope, extended to cover M10 plugins and tools.

## Product scope

### Capabilities and sandbox profiles

- `Capability` and `SandboxProfile` models (product design §14)
- Capability resolver: agent capabilities + sandbox profile → allowed commands, paths, network hosts, secrets, **plugins and tools**
- Default sandbox profiles per agent preset (product design §18.1)
- Replace `sandbox/permissive.rs` with profile-driven enforcement on all runs

### Process sandbox v1

- Command allowlist wrapper, path restrictions (worktree / chat cwd / managed `$HOME`), env injection, timeout, output limits
- Network host allowlist where the platform allows it; otherwise documented as best-effort
- External plugin MCP server processes (M10) run under the same sandbox profile as the agent that uses them

### Tool policy

- Coppice MCP endpoint (M10) checks every tool call against the resolved capabilities before dispatch
- Denied tool call returns a structured error the agent can report as a `missing_capability` blocker
- Chat write-denial (M09) becomes one policy among others, not a special case

### Secrets

- Existing encrypted `secrets` store (M07) extended with scopes: allowed agents, plugins, repos
- Injected into run / plugin env only when allowed — never in prompts, context files, comments, tool results, or API responses
- Output redaction: known secret values scrubbed from logs, artifacts, and comments
- Admin screen: Secrets (names/scopes only after creation)

### Blocker flow with guided unblock

```json
{
  "status": "blocked",
  "blockerType": "missing_capability",
  "requiredCapabilities": ["psql"],
  "requiredSecrets": ["DB_READONLY_URL"]
}
```

Ticket UI shows: Allow command | Add secret | Grant capability | Enable tool | Reject | Ask agent why. A grant resumes the run through the existing clarification/resume path.

### Audit log

- Sensitive actions: secret create/grant/delete, capability grant, sandbox profile change, plugin install/enable, push branch, create PR
- Admin screen: Audit log (filterable, read-only)

## Out of scope

- Container sandbox v2, Kubernetes runner
- Workspace signals / observation runs (M12)
- Autonomous merge/deploy

## Dependencies

- M07: encrypted secrets store, git/PR actions (now audited)
- M09: chat write-denial folded into policy
- M10: MCP endpoint and plugin processes as the enforcement point

## Architecture notes

```text
server/src/
  sandbox/
    command_wrapper.rs
    policy.rs
    permissive.rs         # removed or test-only
  services/
    capability_service.rs
    secret_service.rs     # extended with scopes + injection
    audit_service.rs
  mcp/policy.rs           # tool-call authorization
  api/
    capabilities.rs
    sandbox_profiles.rs
    secrets.rs
    audit_log.rs
```

Tables: `capabilities`, `sandbox_profiles`, `agent_capabilities`, `secret_scopes`, `audit_log`.

```text
GET/POST/PATCH  /api/capabilities
GET/POST/PATCH  /api/sandbox-profiles
GET/POST        /api/secrets
DELETE          /api/secrets/:id
GET             /api/audit-log
POST            /api/blockers/:id/grant-capability
POST            /api/blockers/:id/add-secret
```

Compose delta: `SECRETS_MASTER_KEY`, `SANDBOX_ENFORCE=true`; optional `postgres-readonly` service behind a compose profile for capability integration tests.

## Testing strategy

### Unit

- Sandbox policy: allowed command passes; `rm`, `curl` denied
- Tool policy: tool outside capabilities denied with structured error
- Secret scope: agent/plugin not in scope → injection blocked
- Capability resolver merges agent + profile
- Redaction scrubs secret values from output

### Integration

- Run without `psql` in profile → blocked → grant capability + secret via API → resume → success
- Plugin tool denied by policy → blocker → enable tool → resume
- Secret value never appears in comments, tool results, artifacts, or API JSON
- Audit log entries for secret grant, capability grant, plugin enable, push

### Smoke

`make e2e-smoke-m11`: agent with restrictive sandbox (no `pnpm`) → blocked badge → grant via guided unblock UI → retry → succeeded.

## Acceptance criteria

- [ ] Sandbox enforces command/path/network/secret policy on all runs
- [ ] Tool calls through the Coppice MCP endpoint are policy-checked
- [ ] External plugin MCP servers run under the agent's sandbox profile
- [ ] Secrets scoped, injected only when allowed, redacted from output, never leaked in API/comments/tool results
- [ ] Capability blockers show guided unblock; grant resumes the run
- [ ] Admin screens: Capabilities, Sandbox Profiles, Secrets, Audit log
- [ ] CI smoke E2E passes

## References

- Product design §14 (capabilities, sandbox, secrets), §18 (permissions)
- Framework selection §4 (sandbox v1), §2 (secrets encryption)
- [M10 — Plugins](./M10-plugins.md)
