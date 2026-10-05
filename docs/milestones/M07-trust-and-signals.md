# M07 — Git/PR & Forge Secrets

## Goal

Minimal, human-triggered git/PR actions on ticket worktrees, backed by encrypted per-repo forge secrets.

> **Scope change (2026-09-29).** This milestone was originally "Trust & signals". The unimplemented parts were split out into their own milestones:
>
> - Capabilities, sandbox profiles, secret scoping/injection, guided unblock, audit log → [M13 — Security & sandbox](./M13-security-and-sandbox.md)
> - Workspace signals, Workspace Inbox, Run Observation → [M14 — Role-owner agents](./M14-role-owner-agents.md)
>
> Both now follow [M10 — Plugins](./M10-plugins.md). M07 is closed with the scope below.

## Product scope

### Forge secrets

- `secrets` table: encrypted at rest (ciphertext + nonce), unique name (migration `019_forge_secrets_and_pr.sql`)
- Per-repo forge token (`repos.forge_token_secret_id`) configured on Settings → Repositories
- Secret values are write-only from the UI/API after creation

### Git / PR (minimal)

- Ticket git info / diff summary from the worktree (under `WORKTREES_PATH`, from registered repo `local_path`)
- Push branch — explicit human-triggered action
- Create PR via forge API using repo `remote_url` + per-repo secret; PR URL stored on `tickets.pr_url`; no auto-merge
- Merge / rebase ticket branch, remove worktree (human-triggered)

## Out of scope (moved)

- Sandbox enforcement, capabilities, secret injection into runs, blocker unblock flow, audit log → M13
- Proactive signals, Workspace Inbox, observation runs → M14
- GitLab integration, autonomous merge/deploy

## Implementation

```text
server/src/
  services/
    secret_service.rs
    ticket_git_service.rs   # git_info, push_branch, create_pr, merge, rebase, remove_worktree
    pr_create_url.rs
  api/tickets.rs            # git/PR endpoints
```

## Acceptance criteria

- [x] Forge secrets encrypted at rest; value not returned after save
- [x] Per-repo forge token configurable in Settings → Repositories
- [x] View diff / push branch / create PR available with human trigger
- [x] Merge / rebase / remove worktree available with human trigger

## References

- Product design §14 (secrets), §24
- [M13 — Security & sandbox](./M13-security-and-sandbox.md)
- [M14 — Role-owner agents](./M14-role-owner-agents.md)
