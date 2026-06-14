# In-App Code Review — Design Spec

**Date:** 2026-06-08  
**Status:** Approved (plan ready)  
**Product:** Coppice — agent workspace on tickets

**Depends on:** M03 registered repositories & worktrees, M05 comments (`review_feedback` intent), ticket git actions  
**Replaces (partially):** M07 planned `POST /api/tickets/:id/view-diff` — superseded by richer repo-scoped diff APIs + dedicated Code page

## Purpose

Let humans review agent (or branch) changes **inside Coppice** — browse changed files, read syntax-highlighted diffs, leave inline line comments, and submit a single combined review comment on a ticket — without embedding a diff viewer in the ticket drawer.

The Code page opens in a **new browser tab** as a full-width dedicated route. Entry points: ticket metadata sidebar and per-repo action on Settings → Repositories.

## Problem (today)

| Path | UX | Gap |
|------|-----|-----|
| **Ticket drawer** | Detail, Live Console, Agent Runs | No way to read diffs or leave structured review feedback on code |
| **Create PR button** | Opens external git host | Review happens outside Coppice; feedback must be copy-pasted into comments |
| **M07 view-diff (planned)** | Ticket-scoped diff artifact | Not implemented; no inline comments or submit-to-ticket flow |

Engineers reviewing `wait_for_final_review` tickets need PR-style review (file list + diff + line notes → one ticket comment) tied to the ticket worktree.

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Diff baseline | Worktree branch vs **selectable base branch** (default: repo `defaultBranch`); three-dot range (`base...HEAD`) |
| Diff UI library | **`react-diff-view`** + `gitdiff-parser` + existing `highlight.js` (Monaco deferred) |
| Submit format | **One combined comment** — summary + inline notes grouped by file (GitHub-style review body) |
| Entry points | Ticket metadata sidebar **Review code** + Repositories page **View code** per repo row |
| Worktree picker | **All worktrees on disk** for that repo (path + branch); optional ticket title when linked |
| No ticket context | Submit allowed → **create new unassigned ticket** (Backlog); user enters title + project in submit dialog |
| Existing ticket submit | **Optional workflow action** in dialog: comment only (default), move to In Progress, or reassign engineer |
| Page placement | Dedicated `/code` route in new tab — **never embedded** in ticket drawer tabs |

---

## Architecture overview

```text
Entry: Ticket sidebar or Repositories → View code
  → /code?repoId=&worktree=&ticketId=&baseBranch=
  → GET /api/repos/:id/worktrees
  → GET /api/repos/:id/branches
  → GET /api/repos/:id/diff?worktreePath=&baseBranch=
  → GET /api/repos/:id/diff/file?worktreePath=&baseBranch=&path=
  → User adds inline comments (client state) + summary
  → POST /api/code-reviews/submit
       → create ticket if needed
       → format markdown review body
       → CommentService.create (intent: review_feedback)
       → optional status/assign workflow action
       → publish CommentCreated event
```

**Approach:** Server-side git diff (read-only) + client-side review UI. No new database tables in v1 — reviews are ticket comments with structured markdown bodies.

**Library choice (recommended):** `react-diff-view` — purpose-built for PR-style review with inline widgets; ~50KB vs Monaco ~2MB; pairs with `highlight.js` already in the project. Monaco remains a future upgrade if IDE density is needed.

---

## Navigation & routing

**Route:** `/code` (protected; opens in new browser tab)

**Query params:**

```text
/code?repoId=<uuid>&worktree=<path>&ticketId=<uuid>&baseBranch=main
```

| Param | Required | Notes |
|-------|----------|-------|
| `repoId` | Yes (or inferred from ticket) | Registered repo |
| `worktree` | No | Auto-set from ticket or first worktree for repo |
| `ticketId` | No | Pre-fills submit target; shows ticket link in toolbar |
| `baseBranch` | No | Defaults to repo `defaultBranch` |

**Entry points:**

1. **Ticket metadata sidebar** — "Review code" button when ticket has `repoId` and repo is `ready`. Opens `/code?ticketId=…&repoId=…&worktree=…` with worktree and base branch pre-selected.

2. **Repositories page** (`/settings/repositories`) — "View code" action on each ready repo row. Opens `/code?repoId=…`; user selects worktree from dropdown.

**Page chrome:** Minimal header (logo + link back to Repositories or ticket board). Full viewport width — no `max-w-6xl` constraint on this route.

Code review **does not** appear as a tab in the ticket drawer (Detail / Live Console / Agent Runs).

---

## Code page layout & diff viewing

**Layout (three columns, full viewport height):**

```text
┌─────────────────────────────────────────────────────────────────┐
│ Toolbar: Repo ▾  Worktree ▾  Base branch ▾  [Split|Unified]    │
│          Ticket link (if set)                    [Submit review]│
├──────────┬──────────────────────────────────────────────────────┤
│ Changed  │  Diff viewer (react-diff-view)                       │
│ files    │  - syntax highlighted hunks                          │
│ (scroll) │  - click line → add inline comment                   │
│          │  - pending comments shown as widgets in diff         │
└──────────┴──────────────────────────────────────────────────────┘
```

### Toolbar

- **Repo** — read-only label when opened with `repoId`
- **Worktree** — dropdown from `GET /api/repos/:id/worktrees` (path + branch); changing worktree reloads diff
- **Base branch** — dropdown from `GET /api/repos/:id/branches`; defaults to `defaultBranch`
- **Split / Unified** — toggle diff view mode
- **Ticket link** — if `ticketId` set, show title linking to board; else "No ticket — will create on submit"
- **Submit review** — opens submit dialog

### Changed-files panel (left)

- Files from `git diff --name-status <base>...HEAD` in selected worktree
- Status badge (A/M/D/R) and +/- line counts per file
- Click file → load and display that file's patch in main panel
- "All files" at top (optional v1: default to first changed file to avoid huge combined diffs)

### Diff viewer (main)

- Server returns unified diff patch per file; frontend parses with `gitdiff-parser` / `react-diff-view` `parseDiff`
- Syntax highlighting via `highlight.js` (language from file extension)
- **Inline comments:** click line number → textarea via `renderWidget`; stored in React state until submit
- Pending comments show gutter marker; editable/removable before submit
- **Lazy load:** fetch file list first; fetch per-file patch on selection

### Diff computation (server)

- Inside worktree: `git diff <baseBranch>...HEAD` (three-dot, PR-style)
- Validate worktree belongs to registered repo (path under `WORKTREES_PATH`, git dir matches repo `local_path`)
- Return `{ files, baseSha, headSha }` for summary; `{ patch }` per file

### Empty / error states

- No worktrees → empty state + link to Repositories
- Worktree removed → message + pick another
- No diff (branch matches base) → "No changes" with branch/SHA info
- Repo not ready → block with verify message
- File too large (>512KB patch) → placeholder "File too large to diff inline"

---

## Submit review flow

### Submit dialog fields

| Field | Existing ticket | No ticket |
|-------|-----------------|-----------|
| Review summary | Required | Required |
| Inline comments preview | Read-only, grouped by file | Same |
| Project | Hidden | Required dropdown |
| Ticket title | Hidden | Required |
| Ticket description | Hidden | Optional |
| Workflow action | Optional (see below) | Hidden (new ticket → Backlog) |

### Combined comment body

Posted as one human comment with `intent: review_feedback`:

```markdown
## Code review

**Repo:** coppice · **Worktree:** TICKET-abc-… · **Compare:** main → agent/TICKET-abc (abc1234)

### Summary
<user summary>

### Inline comments

#### `src/foo.ts`
- **L42** (new): Missing null check here.
- **L88** (old): Consider extracting this helper.

#### `server/bar.rs`
- **L15** (new): Rename for clarity.
```

Line references use diff side: `old`, `new`, or `delete`. Omit "Inline comments" section when empty.

Formatting is done **server-side** on submit (client preview mirrors logic for UX only).

### Optional workflow actions (existing ticket only)

- **Comment only** (default)
- **Move to In Progress** — for tickets in `wait_for_final_review` with requested changes
- **Reassign engineer** — set assignee to last engineer agent on ticket, or show picker if none; does **not** auto-start a run

### After submit

- Clear local draft comments
- Toast: "Review posted" with link to ticket on board
- Return `{ ticketId, commentId, ticketCreated }`

### Draft persistence (nice-to-have, not v1)

- `sessionStorage` keyed by `repoId+worktree+baseBranch` for pending comments + summary

---

## Backend API

**New module:** `server/src/services/code_review_service.rs`  
Reuses git helpers from `ticket_git_service.rs` and path logic from `worktree_service.rs`.

### `GET /api/repos/:repoId/worktrees`

Scan `WORKTREES_PATH` for directories matching `TICKET-*-{repo_slug}` (via `slugify(repo.name)`).

For each existing path: `git -C <path> rev-parse --abbrev-ref HEAD` and `git rev-parse HEAD`.

Cross-reference tickets where `repo_id` matches (attach `ticketId` + `ticketTitle` when known — informational).

```json
{
  "worktrees": [
    {
      "path": "/data/worktrees/TICKET-abc-coppice",
      "branch": "agent/TICKET-abc",
      "headSha": "abc1234",
      "ticketId": "uuid | null",
      "ticketTitle": "string | null"
    }
  ]
}
```

### `GET /api/repos/:repoId/branches`

List local branches from repo `local_path` (reuse `list_local_branches`).

```json
{ "defaultBranch": "main", "branches": ["main", "develop"] }
```

### `GET /api/repos/:repoId/diff`

Query: `worktreePath`, `baseBranch`

- Validate repo `ready`
- Validate `worktreePath` canonicalizes under `WORKTREES_PATH` and git dir matches repo
- `git diff --name-status` and `--numstat` with `<baseBranch>...HEAD`
- Resolve `baseSha`, `headSha`

```json
{
  "baseBranch": "main",
  "baseSha": "…",
  "headSha": "…",
  "files": [
    { "path": "src/foo.ts", "status": "modified", "additions": 12, "deletions": 3 }
  ]
}
```

### `GET /api/repos/:repoId/diff/file`

Query: `worktreePath`, `baseBranch`, `path`

- Same validation as diff summary
- `git diff <baseBranch>...HEAD -- <path>` → `{ "path", "patch" }`
- Reject paths with `..` or absolute paths; cap patch size at 512KB

### `POST /api/code-reviews/submit`

Auth required (any member).

```json
{
  "repoId": "uuid",
  "worktreePath": "/data/worktrees/TICKET-…",
  "baseBranch": "main",
  "headSha": "abc1234",
  "ticketId": "uuid | null",
  "newTicket": {
    "projectId": "uuid",
    "title": "…",
    "description": "…"
  },
  "summary": "Overall looks good, a few nits.",
  "inlineComments": [
    { "path": "src/foo.ts", "line": 42, "side": "new", "body": "Missing null check." }
  ],
  "workflowAction": "none | move_to_in_progress | reassign_engineer"
}
```

Server steps:

1. Validate repo, worktree, branches, paths
2. If `ticketId` — load ticket, verify `repoId` matches
3. If no ticket — create via `TicketService::create` (Backlog, unassigned, repo linked, branch from worktree HEAD)
4. Format markdown body
5. Create comment via `CommentService` (`author: human`, `intent: review_feedback`)
6. Apply optional `workflowAction` via existing status/assign services
7. Publish `CommentCreated` event
8. Return `{ ticketId, commentId, ticketCreated }`

---

## Security

- Git commands use fixed argument arrays — no shell interpolation
- `worktreePath` must canonicalize under configured `WORKTREES_PATH`
- `baseBranch` validated with existing `validate_branch_name`
- File paths in diff requests: no `..`, no absolute paths
- Patch size cap per file (512KB)
- All endpoints require authenticated session (same as other ticket/repo APIs)

---

## Frontend modules

```text
web/src/features/code/
  CodeReviewPage.tsx
  ChangedFilesPanel.tsx
  DiffViewer.tsx
  InlineCommentWidget.tsx
  SubmitReviewDialog.tsx
  formatReviewPreview.ts
  useCodeReview.ts          # worktrees, branches, diff, file patch, submit
```

**Route:** add `/code` to `App.tsx` (protected).

**Entry-point changes:**

- `TicketMetadataPanel.tsx` — "Review code" button
- `RepositoriesPage.tsx` — "View code" per repo row (when `verificationStatus === 'ready'`)

**Dependencies:**

- `react-diff-view`
- `gitdiff-parser`

---

## Testing

### Unit tests (Rust)

- `validate_worktree_path` — accept/reject paths
- `validate_diff_path` — reject traversal
- `format_review_comment` — markdown output
- `list_worktrees_for_repo` — slug filter

### Integration tests

- Repo + ticket + worktree with commits ahead of base
- Worktrees list, diff summary, file patch endpoints
- Submit with existing ticket → `review_feedback` comment
- Submit without ticket → new Backlog ticket + comment
- Submit with `move_to_in_progress` → status updated
- Invalid worktree → 400; repo not ready → 400/403

### Frontend (vitest)

- Review preview formatter
- Submit dialog field visibility (ticket vs no-ticket)

### Manual smoke

1. Ticket in `wait_for_final_review` → Review code → diff visible
2. Inline comments + summary → submit with Move to In Progress
3. Repositories → View code → new ticket on submit
4. Comment readable in ticket thread for agent follow-up

---

## Out of scope (v1)

- Editing code in the diff viewer
- Uncommitted working-tree diff (only branch vs base)
- Multi-reviewer approval states
- GitHub PR sync or posting review externally
- Binary / image file diffs (placeholder only)
- `sessionStorage` draft persistence
- Monaco diff editor upgrade
- Separate top-level "Code" nav item (Repositories page is the global entry)

---

## Acceptance criteria

- [x] `/code` page opens in new tab from ticket sidebar and Repositories page
- [x] Worktree and base branch selectable; diff shows changed files with syntax highlighting
- [x] Inline line comments can be added and removed before submit
- [x] Submit posts one combined `review_feedback` comment on existing ticket
- [x] Submit without ticket creates unassigned Backlog ticket (title + project from dialog)
- [x] Optional workflow action on existing ticket (comment only / move to In Progress / reassign)
- [x] Invalid paths and oversized files rejected safely
- [x] Integration tests pass for diff APIs and submit flow
