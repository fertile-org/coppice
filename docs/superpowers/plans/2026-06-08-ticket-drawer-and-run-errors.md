# Ticket Drawer Layout & Run Error Handling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist and display full agent-run failure details, re-verify repos before runs, and restructure the ticket drawer into a right-side 90% panel with a two-column Detail tab.

**Architecture:** Server adds `format_job_error` (anyhow `{:#}` chain) and richer git CLI output; `start_run` calls `RepoService::verify` before enqueue. Web splits drawer into `TicketDetailPanel` + `TicketMetadataPanel`, two tabs only, fade-in shell. Run notifications deferred to M04.

**Tech Stack:** Rust/Axum/SQLx/anyhow, React 19/Vite/Tailwind/TanStack Query/Vitest, git CLI

**Spec:** [docs/superpowers/specs/2026-06-08-ticket-drawer-and-run-errors-design.md](../specs/2026-06-08-ticket-drawer-and-run-errors-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/src/util/mod.rs` | New `util` module root |
| `server/src/util/error_format.rs` | `format_job_error` helper |
| `server/src/lib.rs` | `pub mod util` |
| `server/src/services/worktree_service.rs` | Richer git stdout/stderr + IO context |
| `server/src/workers/job_worker.rs` | Use `format_job_error` in `fail_job` |
| `server/src/services/run_service.rs` | Re-verify repo in `start_run` |
| `server/tests/integration_agent_runs.rs` | Stale path + detailed failure tests |
| `web/tailwind.config.js` | `fade-in` keyframe + animation |
| `web/src/features/tickets/TicketDetailPanel.tsx` | Left column: title, description, comments |
| `web/src/features/tickets/TicketMetadataPanel.tsx` | Right column: assignee + metadata |
| `web/src/features/tickets/TicketDrawer.tsx` | Right drawer shell, two tabs |
| `web/src/features/tickets/TicketRunsTab.tsx` | Auto-expand failed run errors |
| `web/src/features/tickets/TicketDrawer.test.tsx` | Drawer layout tests |
| `web/src/features/tickets/TicketRunsTab.test.tsx` | Auto-expand error test |

**Remove after refactor:** `web/src/features/tickets/TicketDescriptionTab.tsx`, `web/src/features/tickets/TicketMetadataTab.tsx` (logic moved to new panels).

---

### Task 1: `format_job_error` helper

**Files:**
- Create: `server/src/util/mod.rs`
- Create: `server/src/util/error_format.rs`
- Modify: `server/src/lib.rs`

- [ ] **Step 1: Write the failing unit test**

Create `server/src/util/error_format.rs`:

```rust
const MAX_JOB_ERROR_LEN: usize = 4000;

pub fn format_job_error(err: &anyhow::Error) -> String {
    let message = format!("{err:#}").trim().to_string();
    if message.len() <= MAX_JOB_ERROR_LEN {
        return message;
    }
    let mut truncated = message;
    truncated.truncate(MAX_JOB_ERROR_LEN);
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[derive(Debug, thiserror::Error)]
    #[error("git command failed: git worktree add: fatal: not a git repository")]
    struct InnerGitError;

    #[test]
    fn format_job_error_includes_full_chain() {
        let err = anyhow::Error::new(InnerGitError)
            .context("ensure worktree");
        let formatted = format_job_error(&err);
        assert!(formatted.contains("ensure worktree"));
        assert!(formatted.contains("git command failed"));
        assert!(formatted.contains("not a git repository"));
    }

    #[test]
    fn format_job_error_truncates_long_messages() {
        let err = anyhow::anyhow!("x".repeat(5000));
        let formatted = format_job_error(&err);
        assert!(formatted.len() <= MAX_JOB_ERROR_LEN + 4);
        assert!(formatted.ends_with('…'));
    }
}
```

Create `server/src/util/mod.rs`:

```rust
pub mod error_format;
```

Add to `server/src/lib.rs`:

```rust
pub mod util;
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p coppice-server format_job_error -- --nocapture`

Expected: PASS (implementation included above)

- [ ] **Step 3: Commit**

```bash
git add server/src/util/mod.rs server/src/util/error_format.rs server/src/lib.rs
git commit -m "feat(server): add format_job_error for full worker error chains"
```

---

### Task 2: Richer worktree git errors

**Files:**
- Modify: `server/src/services/worktree_service.rs`

- [ ] **Step 1: Update `run_git_in`**

Replace `run_git_in` with:

```rust
async fn run_git_in(git_dir: &Path, args: &[&str]) -> Result<(), WorktreeError> {
    let output = tokio::process::Command::new("git")
        .current_dir(git_dir)
        .args(args)
        .output()
        .await
        .map_err(|err| {
            WorktreeError::Io(std::io::Error::new(
                err.kind(),
                format!(
                    "repository not accessible at {}: {err}",
                    git_dir.display()
                ),
            ))
        })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let combined = match (stderr.is_empty(), stdout.is_empty()) {
            (false, false) => format!("{stderr}\n{stdout}"),
            (false, true) => stderr,
            (true, false) => stdout,
            (true, true) => format!("exit code {}", output.status),
        };
        Err(WorktreeError::GitCommandFailed {
            command: format!("git {}", args.join(" ")),
            stderr: combined,
        })
    }
}
```

- [ ] **Step 2: Add unit test for stdout-only git failure**

Add to `worktree_service.rs` `#[cfg(test)]` module — test `GitCommandFailed` display includes combined output (construct error manually):

```rust
#[test]
fn git_command_failed_display_includes_stderr() {
    let err = WorktreeError::GitCommandFailed {
        command: "git worktree add".into(),
        stderr: "fatal: not a git repository".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("fatal: not a git repository"));
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p coppice-server worktree -- --nocapture`

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add server/src/services/worktree_service.rs
git commit -m "fix(server): include git stdout and path context in worktree errors"
```

---

### Task 3: Wire `format_job_error` in job worker

**Files:**
- Modify: `server/src/workers/job_worker.rs`

- [ ] **Step 1: Use helper in `fail_job` call site**

At top of `job_worker.rs` add:

```rust
use crate::util::error_format::format_job_error;
```

In `process_one`, change the failure branch:

```rust
Err(err) => {
    if run_svc.is_cancelled(run.id).await.unwrap_or(false) {
        job_svc.mark_cancelled(job.id).await?;
    } else {
        fail_job(pool, run.id, job.id, &format_job_error(&err)).await?;
    }
    return Err(err);
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo build -p coppice-server`

Expected: success

- [ ] **Step 3: Commit**

```bash
git add server/src/workers/job_worker.rs
git commit -m "fix(server): persist full error chain on agent run failure"
```

---

### Task 4: Re-verify repository in `start_run`

**Files:**
- Modify: `server/src/services/run_service.rs`

- [ ] **Step 1: Replace stale status check with live verify**

Add imports:

```rust
use crate::services::repo_service::RepoService;
use crate::domain::repo::VerificationStatus;
```

In `start_run`, replace the block from `let repo_row = sqlx::query(...)` through `if status != "ready"` with:

```rust
let repo_id = ticket.ticket.repo_id
    .ok_or_else(|| RunError::Validation("ticket has no repo".into()))?;

let repo = RepoService::new(self.pool)
    .verify(repo_id)
    .await
    .map_err(|e| match e {
        crate::services::repo_service::RepoError::NotFound => {
            RunError::Validation("repo not found".into())
        }
        other => RunError::Database(other.into()),
    })?;

if repo.verification_status != VerificationStatus::Ready {
    let detail = repo
        .verification_error
        .unwrap_or_else(|| "repository path is not ready".to_string());
    return Err(RunError::Validation(format!(
        "repository path is not ready: {detail}"
    )));
}
```

Remove the old `repo_row` query and `status != "ready"` check (repo_id is now resolved above — remove duplicate `repo_id` extraction if present).

- [ ] **Step 2: Add integration test `reject_run_when_repo_path_missing`**

Add to `server/tests/integration_agent_runs.rs`:

```rust
#[tokio::test]
async fn reject_run_when_repo_path_missing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let (ticket_id, _agent_id, _repo_id) =
        setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    drop(git_dir);

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = body
        .as_ref()
        .and_then(|b| b["error"].as_str())
        .unwrap_or("");
    assert!(message.contains("not ready"), "expected not ready message, got: {message}");
}
```

Check API error shape in `server/src/api/agent_runs.rs` — adjust assertion key if handler returns a different JSON field (e.g. `message`).

- [ ] **Step 3: Run integration test**

Run: `cargo test -p coppice-server reject_run_when_repo_path_missing -- --nocapture`

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add server/src/services/run_service.rs server/tests/integration_agent_runs.rs
git commit -m "feat(server): re-verify repository path before starting agent run"
```

---

### Task 5: Integration test for detailed worker failure message

**Files:**
- Modify: `server/tests/integration_agent_runs.rs`

- [ ] **Step 1: Assert `errorMessage` on fixture failure includes root cause**

Extend `retry_after_failed_creates_new_run` after first `poll_run_until(..., "failed", ...)`:

```rust
let error_message = failed_run["errorMessage"]
    .as_str()
    .unwrap_or("");
assert!(
    error_message.len() > "ensure worktree".len(),
    "expected detailed error, got: {error_message:?}"
);
assert!(
    error_message.contains("fixture") || error_message.contains("provider"),
    "expected provider/fixture detail in error, got: {error_message:?}"
);
```

(Adjust substring to match actual `nonexistent-fixture` MockProvider error text after running once locally.)

- [ ] **Step 2: Run test**

Run: `cargo test -p coppice-server retry_after_failed_creates_new_run -- --nocapture`

Expected: PASS; tune assertion strings if needed.

- [ ] **Step 3: Commit**

```bash
git add server/tests/integration_agent_runs.rs
git commit -m "test(server): assert failed runs store detailed error messages"
```

---

### Task 6: Auto-expand errors on Agent Runs tab

**Files:**
- Modify: `web/src/features/tickets/TicketRunsTab.tsx`
- Create: `web/src/features/tickets/TicketRunsTab.test.tsx`

- [ ] **Step 1: Write failing test**

Create `web/src/features/tickets/TicketRunsTab.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { TicketRunsTab } from './TicketRunsTab';
import type { AgentRun } from '../../lib/schemas/agentRun';

const failedRun: AgentRun = {
  id: 'run-1',
  ticketId: 'ticket-1',
  agentId: 'agent-1',
  jobType: 'work_on_ticket',
  status: 'failed',
  sandboxProfileId: 'permissive',
  worktreePath: null,
  branchName: null,
  errorMessage: 'ensure worktree: git command failed: fatal: path missing',
  startedAt: '2026-06-08T00:00:00.000Z',
  endedAt: '2026-06-08T00:00:01.000Z',
  createdAt: '2026-06-08T00:00:00.000Z',
};

vi.mock('./useAgentRuns', () => ({
  useAgentRuns: () => ({ data: [failedRun], isLoading: false, isError: false }),
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({ data: [{ id: 'agent-1', name: 'Worker' }] }),
}));

function renderRuns() {
  const client = new QueryClient();
  return render(
    <QueryClientProvider client={client}>
      <TicketRunsTab ticketId="ticket-1" />
    </QueryClientProvider>,
  );
}

describe('TicketRunsTab', () => {
  it('shows failed run error without extra click', () => {
    renderRuns();
    expect(screen.getByText(/path missing/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /show error/i })).toBeNull();
  });
});
```

Add `import { vi } from 'vitest';` at top.

- [ ] **Step 2: Run test — expect FAIL**

Run: `cd web && yarn test src/features/tickets/TicketRunsTab.test.tsx`

Expected: FAIL (Show error button still present)

- [ ] **Step 3: Update `RunRow` in `TicketRunsTab.tsx`**

Change `RunRow` to default expanded for failed runs:

```tsx
function RunRow({ run, agents }: { run: AgentRun; agents: ... }) {
  const [expanded, setExpanded] = useState(run.status === 'failed');

  useEffect(() => {
    setExpanded(run.status === 'failed');
  }, [run.status, run.errorMessage]);

  // For failed runs with error: show pre directly (optional collapse for long errors)
  {run.errorMessage && (
    <div className="mt-3">
      {run.status !== 'failed' && (
        <button type="button" onClick={() => setExpanded((v) => !v)} ...>
          {expanded ? 'Hide error' : 'Show error'}
        </button>
      )}
      {(expanded || run.status === 'failed') && (
        <pre className="mt-2 ...">{run.errorMessage}</pre>
      )}
    </div>
  )}
}
```

Add `useEffect` import from React.

- [ ] **Step 4: Run test — expect PASS**

Run: `cd web && yarn test src/features/tickets/TicketRunsTab.test.tsx`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web/src/features/tickets/TicketRunsTab.tsx web/src/features/tickets/TicketRunsTab.test.tsx
git commit -m "fix(web): auto-expand agent run errors on failure"
```

---

### Task 7: `TicketDetailPanel` and `TicketMetadataPanel`

**Files:**
- Create: `web/src/features/tickets/TicketDetailPanel.tsx`
- Create: `web/src/features/tickets/TicketMetadataPanel.tsx`
- Delete: `web/src/features/tickets/TicketDescriptionTab.tsx`
- Delete: `web/src/features/tickets/TicketMetadataTab.tsx`

- [ ] **Step 1: Create `TicketDetailPanel.tsx`**

Left column component — copy title/description edit logic from `TicketDescriptionTab.tsx` **without** assignee select. Below description, render a divider and embed `TicketCommentsTab`:

```tsx
import type { Ticket } from '../board/useTickets';
import { TicketCommentsTab } from './TicketCommentsTab';
// ... title/description state from TicketDescriptionTab (no assignee)

export function TicketDetailPanel({ ticket }: { ticket: Ticket }) {
  return (
    <div className="flex min-h-0 flex-col gap-6">
      {/* title + description blocks from TicketDescriptionTab */}
      <hr className="border-border" />
      <section>
        <h3 className="mb-3 font-body text-sm font-medium text-text-secondary">Comments</h3>
        <TicketCommentsTab ticketId={ticket.id} />
      </section>
    </div>
  );
}
```

- [ ] **Step 2: Create `TicketMetadataPanel.tsx`**

Copy body of `TicketMetadataTab.tsx` and add assignee block at top (from `TicketDescriptionTab`):

```tsx
import { useAssignAgent, useAgents, useUpdateTicket, useUpdateTicketStatus } from './useTicket';
// assignee select + assignAgent.mutateAsync on change
// then existing metadata fields (repo, status, priority, substatus, save button)
```

Export as `TicketMetadataPanel`.

- [ ] **Step 3: Delete old tab files**

```bash
rm web/src/features/tickets/TicketDescriptionTab.tsx web/src/features/tickets/TicketMetadataTab.tsx
```

Grep for imports of deleted files — update any references.

- [ ] **Step 4: Verify web build**

Run: `cd web && yarn build`

Expected: success

- [ ] **Step 5: Commit**

```bash
git add web/src/features/tickets/
git commit -m "refactor(web): split ticket detail into content and metadata panels"
```

---

### Task 8: Right-side drawer shell + two tabs

**Files:**
- Modify: `web/src/features/tickets/TicketDrawer.tsx`
- Modify: `web/tailwind.config.js`
- Create: `web/src/features/tickets/TicketDrawer.test.tsx`

- [ ] **Step 1: Add fade-in animation to Tailwind**

In `web/tailwind.config.js` inside `theme.extend`:

```js
      keyframes: {
        'fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
      },
      animation: {
        'fade-in': 'fade-in 200ms var(--ease-out) forwards',
      },
```

- [ ] **Step 2: Write failing drawer layout test**

Create `web/src/features/tickets/TicketDrawer.test.tsx` with mocks for `useTicket`, `useAgentRuns`, `useRunAgent`, `useStopRun`, `useRepos`:

```tsx
it('renders Detail and Agent Runs tabs only', () => {
  renderDrawer();
  expect(screen.getByRole('tab', { name: 'Detail' })).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: 'Agent Runs' })).toBeInTheDocument();
  expect(screen.queryByRole('tab', { name: 'Comments' })).toBeNull();
});

it('drawer panel uses 90% width class', () => {
  renderDrawer();
  const dialog = screen.getByRole('dialog', { name: 'Ticket detail' });
  expect(dialog.className).toMatch(/w-\[90%\]/);
});
```

- [ ] **Step 3: Refactor `TicketDrawer.tsx`**

Key structural changes:

```tsx
type DrawerTab = 'detail' | 'runs';

// Replace outer return:
return (
  <div className="fixed inset-0 z-50 flex justify-end" role="presentation">
    <div
      className="absolute inset-0 bg-bark-950/40 backdrop-blur-[1px]"
      onClick={onClose}
      aria-hidden="true"
    />
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Ticket detail"
      className="relative flex h-full w-[90%] max-w-[90vw] animate-fade-in flex-col bg-surface-raised shadow-2xl"
      onClick={(e) => e.stopPropagation()}
    >
      {/* header unchanged */}
      <nav role="tablist" ...>
        <button role="tab" ...>Detail</button>
        <button role="tab" ...>Agent Runs</button>
      </nav>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {ticket && tab === 'detail' && (
          <div className="grid h-full gap-0 md:grid-cols-[3fr_2fr]">
            <div className="min-h-0 overflow-y-auto px-6 py-5">
              <TicketDetailPanel ticket={ticket} />
            </div>
            <div className="min-h-0 overflow-y-auto border-border px-6 py-5 md:border-l">
              <TicketMetadataPanel ticket={ticket} />
            </div>
          </div>
        )}
        {ticket && tab === 'runs' && (
          <div className="px-6 py-5">
            <TicketRunsTab ticketId={ticket.id} />
          </div>
        )}
      </div>
    </div>
  </div>
);
```

Remove imports of deleted tab components; import new panels.

- [ ] **Step 4: Run tests**

Run: `cd web && yarn test src/features/tickets/TicketDrawer.test.tsx`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web/src/features/tickets/TicketDrawer.tsx web/src/features/tickets/TicketDrawer.test.tsx web/tailwind.config.js
git commit -m "feat(web): right-side ticket drawer with two-column detail layout"
```

---

### Task 9: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Server tests**

Run: `make test`

Expected: all pass

- [ ] **Step 2: Clippy**

Run: `make clippy`

Expected: no warnings

- [ ] **Step 3: Web tests + build**

Run: `make web-test && make web-build`

Expected: pass

- [ ] **Step 4: Manual smoke (local stack)**

```bash
make compose-local-down && make compose-local-up
```

1. Open board → ticket drawer: ~90% width, right-aligned, fade-in, overlay visible.
2. Detail tab: description + comments left; assignee + metadata right.
3. Register `/data/host-repos/coppice` repo, link ticket, Run Agent.
4. On failure, Agent Runs tab shows full git error (not just `ensure worktree`).

- [ ] **Step 5: Update spec status**

In `docs/superpowers/specs/2026-06-08-ticket-drawer-and-run-errors-design.md`, set `**Status:** Implemented`.

- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/specs/2026-06-08-ticket-drawer-and-run-errors-design.md
git commit -m "docs: mark ticket drawer and run errors spec implemented"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| `format_job_error` with `{:#}` + 4k cap | Task 1, 3 |
| Git stderr + stdout | Task 2 |
| Re-verify at `start_run` | Task 4 |
| Auto-expand failed run errors | Task 6 |
| 90% right drawer, fade-in, overlay | Task 8 |
| Two-column Detail layout | Task 7, 8 |
| Assignee in metadata column | Task 7 |
| Two tabs only | Task 8 |
| M04 notifications deferred | Already in M04 doc (no task) |

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-08-ticket-drawer-and-run-errors.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration  
2. **Inline Execution** — run tasks in this session with checkpoints for review

Which approach?
