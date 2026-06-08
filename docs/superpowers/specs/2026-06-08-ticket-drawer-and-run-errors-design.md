# Ticket Drawer Layout & Agent Run Error Handling

**Date:** 2026-06-08  
**Status:** Implemented  
**Milestone:** Pre-M04 polish (does not replace M04)  
**Deferred to M04:** Run completion toasts / app-level notifications (see `docs/milestones/M04-live-console.md`)

## Purpose

Two UX gaps block confident agent-run testing after M03:

1. **Opaque run failures** — worker failures often persist only a context label (e.g. `ensure worktree`) without the underlying git/OS cause, so the Runs tab is unhelpful.
2. **Cramped ticket detail** — full-screen drawer with four tabs hides description, metadata, and comments behind navigation.

Run-completion **notifications** (toasts, app-level feedback when the drawer is closed) are explicitly **out of scope here** and captured in M04 WebSocket work.

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Notifications / toasts | **Defer to M04** — use `agent_run.finished` WebSocket events |
| Toast position (when built) | Top-right of viewport |
| Toast behavior (when built) | Success: subtle, 3s auto-dismiss; failure: persistent; click → Agent Runs tab + scroll/highlight |
| Toasts when drawer closed | Yes (app-level) — noted in M04, not built now |
| Error storage | Fix server to persist full error chain; no schema change |
| Repo re-verify | Re-verify `local_path` at `start_run` (sync), not only stale DB `ready` |
| Drawer width | ~90% viewport, anchored right, full height |
| Drawer animation | Fade-in only (opacity ~200ms); backdrop overlay behind |
| Detail layout | Trello-style two columns on Detail tab |
| Tabs | **Detail** and **Agent Runs** only |
| Assignee | Move from Description to metadata (right) column |

---

## 1. Agent run error handling (server)

### Problem

`job_worker.rs` calls `fail_job(..., &err.to_string())`. For worktree failures wrapped with `.context("ensure worktree")`, `to_string()` can collapse to the context label only — the `WorktreeError` / IO / git stderr is lost. Example observed in production: `error_message = 'ensure worktree'` (15 chars).

### Solution

#### 1.1 `format_job_error(err: &anyhow::Error) -> String`

New small helper (e.g. `server/src/util/error_format.rs`):

- Use alternate display: `format!("{err:#}")` to include the full error chain.
- Trim whitespace; cap length at **4000** characters before DB write (truncate with `…` suffix).

Replace all `err.to_string()` passed to `fail_job` / `finish_failed` in the worker path with `format_job_error(&err)`.

#### 1.2 Richer git command errors

In `worktree_service.rs` `run_git_in`:

- On non-success exit, combine **stderr + stdout** (git sometimes prints `fatal:` to stdout).
- Format: `git command failed: git {args}: {output}`

On `Command::output()` IO failure (bad `current_dir`, missing git binary):

- Return explicit message: `repository not accessible at {path}: {io_error}`

#### 1.3 Re-verify repository at run start

In `RunService::start_run`, after loading repo row:

- Call `verify_local_path` synchronously (same logic as Settings → Verify).
- If not `Ready`, return `RunError::Validation` with verifier message (e.g. `repository path is not ready: path does not exist`).
- Update repo row verification status when re-verify fails (keep DB in sync).

Worker may still fail for other reasons (worktree branch collision, disk full); those use `format_job_error`.

### API / schema

No migration. `agent_runs.error_message` unchanged.

### Runs tab display (web)

- Failed runs: **auto-expand** error block (no extra click for default case).
- Keep monospace `pre` with `whitespace-pre-wrap` for long chains.
- Optional: copy-to-clipboard button (YAGNI — skip unless trivial).

---

## 2. Ticket drawer layout (web)

### Shell (`TicketDrawer.tsx`)

```text
┌──────────────────────────────────────────────────────────────┐
│ viewport                                                      │
│  ┌────────┐ ┌─────────────────────────────────────────────┐ │
│  │ board  │ │ drawer (90% width, right, full height)       │ │
│  │ ~10%   │ │ fade-in, bg-surface-raised                   │ │
│  │ visible│ │                                              │ │
│  └────────┘ └─────────────────────────────────────────────┘ │
│  ░░░░░░░░░░ overlay (click to close) ░░░░░░░░░░░░░░░░░░░░░░ │
└──────────────────────────────────────────────────────────────┘
```

- Outer wrapper: `fixed inset-0 z-50 flex justify-end`.
- **Overlay:** `absolute inset-0 bg-bark-950/40` — click closes; `Escape` unchanged.
- **Panel:** `relative h-full w-[90%] max-w-[90vw] flex flex-col shadow-2xl animate-fade-in`.
- Add Tailwind keyframe `fade-in` (opacity 0→1, ~200ms) if not present.

### Header (unchanged responsibilities)

- Ticket title, status line, repo-not-ready warning.
- **Run Agent** / **Stop** / **Close** (right).
- Remove dependency on four-tab navigation.

### Tabs (two)

| Tab | Label | Content |
|-----|-------|---------|
| `detail` | Detail | Two-column layout (below) |
| `runs` | Agent Runs | Existing `TicketRunsTab` |

Default tab: `detail`.

### Detail tab — two columns

```text
┌─────────────────────────────────────────────────────────────┐
│ Header: title, status, Run Agent / Stop / Close              │
├─────────────────────────────────────────────────────────────┤
│ Detail │ Agent Runs                                          │
├──────────────────────────────┬──────────────────────────────┤
│ LEFT (~60%)                  │ RIGHT (~40%)                  │
│                              │ border-l                      │
│ Title + edit controls        │ Assignee (moved here)         │
│ Description (markdown)       │ Repository                    │
│ ─────────────────────        │ Status, Priority, Substatus   │
│ Comments section             │ Substatus metadata fields     │
│ (full thread + compose)      │ Branch / worktree (read-only) │
│                              │ Save metadata button          │
│ (single scroll)              │ (own scroll if needed)        │
└──────────────────────────────┴──────────────────────────────┘
```

**Responsive (`< md`):** stack right column below left (metadata after content).

### Component refactor

| New / updated | Responsibility |
|---------------|----------------|
| `TicketDetailPanel.tsx` | Left column: title edit, description, comments (compose from `TicketDescriptionTab` + `TicketCommentsTab`) |
| `TicketMetadataPanel.tsx` | Right column: assignee + all fields from `TicketMetadataTab` |
| `TicketDrawer.tsx` | Shell, header, two tabs, column grid |
| `TicketRunsTab.tsx` | Runs list; auto-expand errors on `failed` |

Remove standalone tab components from drawer routing (files may remain exported for tests or be deleted if fully inlined).

**Assignee:** Move select + assign mutation from `TicketDescriptionTab` to `TicketMetadataPanel`. Description panel is title + body only.

### Board integration

`BoardPage.tsx` — no API change; drawer open/close state unchanged.

---

## 3. Deferred: run notifications (M04)

Documented in `docs/milestones/M04-live-console.md`:

- App-level toast stack (top-right).
- Subscribe to `agent_run.finished` on `/ws/events` (and/or poll fallback).
- Success toast: 3s auto-dismiss; failure toast: persistent; click → open ticket drawer, Agent Runs tab, scroll + highlight failed row.
- Fires even when ticket drawer is closed.

No implementation in this spec.

---

## Testing

### Server unit tests

- `format_job_error` preserves nested context (mock anyhow chain).
- `run_git_in` includes stdout when stderr empty.
- `verify_local_path` integration in `start_run` rejects missing path.

### Server integration test

- Register repo with path that exists at create time; delete path; `POST run-agent` → 400 with meaningful message.
- Worker failure path: temp git repo + force worktree failure → `error_message` length > context label alone.

### Web unit tests (Vitest)

- `TicketDrawer` renders two tabs.
- Detail tab shows two columns at `md+`.
- `RunRow` auto-expands error when `status === 'failed'`.

### E2E

- Optional: extend M03 smoke assert `errorMessage` on forced failure contains `git` or `repository` substring (not required for layout).

---

## Acceptance criteria

- [ ] Failed agent runs store a multi-line `error_message` with root cause (not context label alone).
- [ ] `start_run` re-verifies `local_path`; stale `ready` repos rejected with clear 400.
- [ ] Runs tab auto-shows full error for failed runs.
- [ ] Ticket drawer is ~90% width, right-aligned, fade-in, with backdrop overlay.
- [ ] Detail tab: left = description + comments; right = metadata including assignee.
- [ ] Only **Detail** and **Agent Runs** tabs remain.
- [ ] M04 milestone doc lists deferred toast notification requirements.

## References

- `server/src/workers/job_worker.rs` — `fail_job`, `.context("ensure worktree")`
- `server/src/services/worktree_service.rs` — `run_git_in`
- `web/src/features/tickets/TicketDrawer.tsx` — current full-screen four-tab layout
- `docs/milestones/M04-live-console.md` — WebSocket events (notification home)
