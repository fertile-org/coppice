# M04 — Live Console

## Goal

Observable agent runs through a live terminal stream in the browser, persisted log artifacts, and realtime board updates via WebSocket.

## Product scope

- tmux session per agent run (one session name per run ID)
- MockProvider emits scripted terminal output during run (simulates CLI typing)
- WebSocket `GET /ws/agent-runs/:id/live` — stream terminal frames to browser
- WebSocket `GET /ws/events` — broadcast ticket.updated, agent_run.started, agent_run.finished, comment.created
- Ticket detail: **Live Console** tab with xterm.js
- Stop / Retry controls wired to run lifecycle
- Terminal log saved as artifact on filesystem after run completes (not in Postgres body)
- Board card badge: live run indicator
- SPA subscribes to `/ws/events` for board refresh

## Out of scope

- Workflow mentions (M05)
- Strict sandbox command filtering (M07)
- PTY driver without tmux (future option)

## Dependencies

- M01: auth (WebSocket must validate session)
- M03: agent runs, MockProvider with stdout script

## Architecture notes

### New server modules

```text
server/src/
  sessions/
    tmux_driver.rs
    terminal_stream.rs
  api/
    ws/
      live.rs
      events.rs
  services/
    artifact_service.rs   # terminal_log type
```

### Artifact storage

```text
/data/artifacts/runs/{run-id}/terminal.log
/data/artifacts/runs/{run-id}/meta.json
```

Database stores attachment metadata only (product design §10.3).

### WebSocket auth

Session cookie on WebSocket upgrade; reject unauthenticated connections.

### Frontend

```text
web/src/features/
  runs/
    LiveConsole.tsx    # xterm.js + WS connect
  ws/
    useEventSocket.ts  # board refresh
```

## Docker Compose delta

**Added in M04:**

```yaml
  server:
    environment:
      TMUX_SESSION_PREFIX: coppice-run
    # server image includes tmux
```

No new services. Artifact volume from M02/M03 used for terminal logs.

## Testing strategy

### Unit tests

- Terminal frame encoding/decoding
- Artifact path builder
- Event payload serialization (ticket.updated, agent_run.*)

### Integration tests

- Start run → attach WS client → receive ≥1 terminal frame → run completes → log file exists on disk
- Stop run → tmux session terminated → WS closes cleanly
- Event bus: run started → subscriber receives agent_run.started
- WS rejected without session cookie

### E2E smoke (CI)

`e2e/smoke/m04-live-console.spec`:

1. Login → ticket → Run Agent
2. Open Live Console tab
3. Assert terminal pane contains mock output text
4. Wait for run complete → Runs tab shows succeeded

### E2E full (local)

- Disconnect WS mid-run → reconnect → stream resumes or shows buffered tail
- Board card shows live badge during run, clears after
- Open terminal.log artifact from Artifacts tab (if exposed) or via API

## Acceptance criteria

- [ ] Live Console displays streaming mock output during run
- [ ] Terminal log persisted as filesystem artifact
- [ ] Board updates without full page reload (WS events)
- [ ] Stop terminates tmux session and run
- [ ] WebSocket requires authentication
- [ ] CI smoke E2E passes

## References

- Product design §10 (live console), §12 (terminal_log artifact), §22 (WebSocket endpoints)
- Framework selection §2 (tmux), §4 (live session v1), §7 (realtime)
