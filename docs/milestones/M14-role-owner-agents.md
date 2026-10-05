# M14 — Role-owner Agents (signals & Workspace Inbox)

## Goal

Let agents that own a role or domain (DBA, QC, Tech Lead, …) **proactively** inspect their area and raise evidence-backed signals into a Workspace Inbox, where humans turn them into tickets or dismiss them. After M14, Coppice matches the full product design for self-hosted v1.

Carried over from the original M07 "Trust & signals" scope; observation runs now use M10 plugin tools and run under M13 capabilities.

This is not the public-beta hero. [M12 — Beta Release](./M12-beta-release.md) ships the gated desktop board first.

## Product scope

### Role ownership

- Agents can declare owned domains (e.g. database, frontend quality, release health) and an observation brief
- Only agents with an owned domain can run observations

### Observation runs

- New run kind `observe_domain` with its own context profile (read-oriented, no ticket)
- Manual **Run Observation** button per role-owner agent (scheduled cron out of scope)
- Uses plugin tools to inspect state (repo, board, knowledge, domain plugins such as a read-only DB tool)
- Capability-gated by M13: observation needs the grants its tools/commands require; missing ones produce a blocker signal

### Workspace signals

- `WorkspaceSignal` model (product design §15.2): agent, domain, title, severity, evidence, recommendation, status
- Raised via a new Coppice core tool `signal_create` on the M10 gateway — evidence and recommendation are required by the tool schema
- Anti-spam (product design §15.6): max signals per agent per day, dedup window (same agent + dedup key updates the existing signal)

### Workspace Inbox

- Inbox / Signals screen with filters (agent, domain, severity, status)
- Actions: Create Ticket, Acknowledge, Dismiss, Snooze, Grant Capability, Add Secret
- Convert signal to ticket with `sourceSignalId` link

## Out of scope

- Scheduled observation cron
- Agent-initiated chat DMs
- Autonomous fixes from signals (a human creates the ticket)

## Dependencies

- M06: knowledge available to observations via `knowledge_search`
- M10: `signal_create` tool, domain plugins
- M13: capabilities/secrets gate what observations can touch; Grant Capability / Add Secret actions

## Architecture notes

```text
server/src/
  domain/workspace_signal.rs
  services/signal_service.rs
  mcp/core_tools/signal.rs
  api/signals.rs
```

Tables: `workspace_signals`, `signal_dedup_keys`, `agent_domains`.

```text
GET   /api/signals
GET   /api/signals/:id
POST  /api/signals/:id/acknowledge
POST  /api/signals/:id/dismiss
POST  /api/signals/:id/snooze
POST  /api/signals/:id/convert-to-ticket
POST  /api/agents/:id/run-observation
```

## Testing strategy

### Unit

- Signal dedup: duplicate dedup key + agent within window updates existing
- Anti-spam: signal over daily limit rejected with clear tool error
- `signal_create` rejects missing evidence/recommendation

### Integration

- DBA mock observation calls `signal_create` → signal with evidence → convert to ticket → ticket has `sourceSignalId`
- Observation lacking a capability → blocker signal → grant → re-run succeeds
- Non-role-owner agent cannot start an observation

### Smoke

`make e2e-smoke-m14`: Run Observation on DBA mock agent → signal appears in Workspace Inbox → Create Ticket → ticket on board.

## Acceptance criteria

- [ ] Role-owner agents with owned domains; Run Observation available only for them
- [ ] Observation runs use plugin tools and respect M13 capabilities
- [ ] Workspace Inbox shows proactive signals with evidence and recommendation
- [ ] Anti-spam limits and dedup enforced
- [ ] Acknowledge / dismiss / snooze / convert-to-ticket work
- [ ] Full product design §1–27 covered for v1 scope
- [ ] `docker compose up` yields complete Coppice ready for daily use
- [ ] CI smoke E2E passes

## References

- Product design §15 (proactive signals), §26 (end-to-end scenarios), §24 Phase 6–9
- [M10 — Plugins](./M10-plugins.md)
- [M12 — Beta Release](./M12-beta-release.md)
- [M13 — Security & sandbox](./M13-security-and-sandbox.md)

## v1 complete

When M14 acceptance criteria pass, Coppice implements the full philosophy product design for self-hosted v1.
