import { useMemo, useState } from 'react';
import { Button } from '../../components/ui/button';
import { useConfirm } from '../../components/ui/confirm-dialog';
import { useToast } from '../../components/ToastProvider';
import { apiErrorToastMessage, parseApiErrorMessage } from '../../lib/api';
import type { WorktreeAdminEntry } from '../../lib/schemas/worktree';
import { cn } from '../../lib/utils';
import { formatRelativeTime } from '../tools/connectorFormat';
import { statusLabel } from '../tickets/useTicket';
import type { TicketStatus } from '../../lib/schemas/ticket';
import { useRemoveWorktrees, useWorktrees } from './useWorktrees';

function formatBytes(bytes: number | null): string {
  if (bytes == null) return '…';
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

function ticketPillLabel(row: WorktreeAdminEntry): string {
  if (!row.ticketId) return 'No ticket';
  if (row.archived) return 'Archived';
  if (row.merged) return 'Merged';
  if (row.ticketStatus) {
    const label = statusLabel(row.ticketStatus as TicketStatus);
    return label || row.ticketStatus;
  }
  return 'Unknown';
}

function pillClass(row: WorktreeAdminEntry): string {
  const base =
    'inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 font-body text-xs';
  if (!row.ticketId) return `${base} border-border text-text-muted`;
  if (row.archived) return `${base} border-border text-text-muted`;
  if (row.ticketStatus === 'done' || row.merged) {
    return `${base} border-success-muted bg-success-muted text-success`;
  }
  if (row.ticketStatus === 'in_progress') {
    return `${base} border-info-muted bg-info-muted text-info`;
  }
  if (
    row.ticketStatus === 'in_review' ||
    row.ticketStatus === 'wait_for_final_review'
  ) {
    return `${base} border-warning-muted bg-warning-muted text-warning`;
  }
  return `${base} border-border text-text-secondary`;
}

function sortLargestFirst(rows: WorktreeAdminEntry[]): WorktreeAdminEntry[] {
  return [...rows].sort((a, b) => {
    if (a.sizeBytes == null && b.sizeBytes == null) return a.folderName.localeCompare(b.folderName);
    if (a.sizeBytes == null) return 1;
    if (b.sizeBytes == null) return -1;
    return b.sizeBytes - a.sizeBytes;
  });
}

export function WorktreesTab() {
  const { data, isLoading, isError, refetch } = useWorktrees();
  const remove = useRemoveWorktrees();
  const confirm = useConfirm();
  const toast = useToast();
  const [repoFilter, setRepoFilter] = useState('all');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [bulkErrors, setBulkErrors] = useState<string[]>([]);

  const repos = useMemo(() => {
    const names = new Set<string>();
    for (const row of data ?? []) {
      if (row.repoName) names.add(row.repoName);
    }
    return [...names].sort();
  }, [data]);

  const filtered = useMemo(() => {
    const rows = (data ?? []).filter(
      (row) => repoFilter === 'all' || row.repoName === repoFilter,
    );
    return sortLargestFirst(rows);
  }, [data, repoFilter]);

  const totalBytes = useMemo(() => {
    let sum = 0;
    let known = false;
    for (const row of filtered) {
      if (row.sizeBytes != null) {
        sum += row.sizeBytes;
        known = true;
      }
    }
    return known ? sum : null;
  }, [filtered]);

  function toggle(folder: string, on: boolean) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (on) next.add(folder);
      else next.delete(folder);
      return next;
    });
  }

  function selectFinished() {
    const next = new Set<string>();
    for (const row of filtered) {
      if (row.finished && !row.hasActiveRun) next.add(row.folderName);
    }
    setSelected(next);
  }

  async function confirmAndRemove(folders: string[]) {
    if (folders.length === 0) return;
    const rows = filtered.filter((row) => folders.includes(row.folderName));
    const bytes = rows.reduce((n, row) => n + (row.sizeBytes ?? 0), 0);
    const dirty = rows.some((row) => row.hasUncommittedChanges === true);
    const names = rows
      .map((row) => row.ticketTitle ?? row.folderName)
      .slice(0, 5)
      .join(', ');
    const extra = rows.length > 5 ? ` and ${rows.length - 5} more` : '';
    const space =
      rows.every((row) => row.sizeBytes != null) && bytes > 0
        ? ` About ${formatBytes(bytes)} will be freed.`
        : '';
    const warning = dirty
      ? '\n\nSome selected worktrees have uncommitted changes. Those changes will be lost.'
      : '';
    const ok = await confirm({
      title:
        folders.length === 1
          ? 'Remove this worktree?'
          : `Remove ${folders.length} worktrees?`,
      description: `${names}${extra}.${space} Ticket branches and commits stay in git.${warning}`,
      confirmLabel: folders.length === 1 ? 'Remove worktree' : 'Remove worktrees',
      danger: true,
    });
    if (!ok) return;

    setBulkErrors([]);
    try {
      const results = await remove.mutateAsync(folders);
      const failed = results.filter((r) => r.status !== 'removed');
      const succeeded = results.filter((r) => r.status === 'removed').length;
      if (succeeded > 0) {
        toast.success(
          succeeded === 1
            ? 'Worktree removed'
            : `${succeeded} worktrees removed`,
        );
      }
      if (failed.length > 0) {
        setBulkErrors(
          failed.map((r) => {
            if (r.status === 'active_run') {
              return `${r.folderName}: an agent is working in this worktree`;
            }
            return `${r.folderName}: ${r.message ?? r.status}`;
          }),
        );
      }
      setSelected((prev) => {
        const next = new Set(prev);
        for (const r of results) {
          if (r.status === 'removed') next.delete(r.folderName);
        }
        return next;
      });
    } catch (err) {
      toast.error(
        apiErrorToastMessage(parseApiErrorMessage(err, 'Unable to remove worktrees.')),
      );
    }
  }

  if (isLoading) {
    return <p className="font-body text-sm text-text-muted">Loading worktrees…</p>;
  }

  if (isError) {
    return (
      <div className="rounded-lg border border-danger-muted bg-danger-muted/50 p-4">
        <p className="font-body text-sm text-danger">Unable to load worktrees.</p>
        <button
          type="button"
          onClick={() => void refetch()}
          className="mt-2 font-body text-sm font-medium text-moss-700 underline-offset-2 hover:underline"
        >
          Try again
        </button>
      </div>
    );
  }

  const selectedCount = [...selected].filter((name) =>
    filtered.some((row) => row.folderName === name),
  ).length;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-3">
        <p className="font-body text-sm text-text-secondary">
          {filtered.length === 0 ? (
            'None'
          ) : (
            <>
              <span className="font-medium text-text-primary">
                {filtered.length} worktree{filtered.length === 1 ? '' : 's'}
              </span>
              {totalBytes != null && <> · {formatBytes(totalBytes)} on disk</>}
            </>
          )}
        </p>
        <div className="grow" />
        <label className="sr-only" htmlFor="worktree-repo-filter">
          Repository
        </label>
        <select
          id="worktree-repo-filter"
          value={repoFilter}
          onChange={(e) => setRepoFilter(e.target.value)}
          className="rounded-md border border-border bg-surface-raised px-2 py-1.5 font-body text-sm text-text-primary"
        >
          <option value="all">All repositories</option>
          {repos.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
        <Button type="button" variant="secondary" onClick={selectFinished}>
          Select finished
        </Button>
      </div>

      {bulkErrors.length > 0 && (
        <div
          role="alert"
          className="rounded-lg border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger"
        >
          <p className="font-medium">Some removals failed</p>
          <ul className="mt-1 list-disc pl-5">
            {bulkErrors.map((msg) => (
              <li key={msg}>{msg}</li>
            ))}
          </ul>
        </div>
      )}

      <div className="overflow-hidden rounded-xl border border-border bg-surface-raised shadow-card">
        {filtered.length === 0 ? (
          <p className="px-4 py-8 text-center font-body text-sm text-text-muted">
            No worktrees on disk.
          </p>
        ) : (
          <table className="w-full text-left">
            <thead>
              <tr className="border-b border-border bg-paper-100">
                <th className="w-10 px-3 py-3" />
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Worktree
                </th>
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Repository
                </th>
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Ticket
                </th>
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Last used
                </th>
                <th className="px-4 py-3 text-right font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Size
                </th>
                <th className="px-4 py-3" />
              </tr>
            </thead>
            <tbody>
              {filtered.map((row) => {
                const checked = selected.has(row.folderName);
                return (
                  <tr
                    key={row.folderName}
                    className={cn(
                      'border-b border-border last:border-b-0',
                      checked && 'bg-moss-50/40',
                    )}
                  >
                    <td className="px-3 py-3">
                      <input
                        type="checkbox"
                        checked={checked}
                        disabled={row.hasActiveRun}
                        title={
                          row.hasActiveRun
                            ? 'An agent is working in this worktree'
                            : undefined
                        }
                        aria-label={`Select ${row.folderName}`}
                        onChange={(e) => toggle(row.folderName, e.target.checked)}
                      />
                    </td>
                    <td className="px-4 py-3">
                      <div className="font-body text-sm font-medium text-text-primary">
                        {row.ticketTitle ?? (
                          <span className="text-text-muted">No ticket</span>
                        )}
                      </div>
                      <div className="mt-0.5 font-mono text-xs text-text-muted">
                        {row.folderName}
                      </div>
                    </td>
                    <td className="whitespace-nowrap px-4 py-3 font-body text-sm text-text-secondary">
                      {row.repoName ?? '—'}
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex flex-wrap items-center gap-1.5">
                        <span className={pillClass(row)}>{ticketPillLabel(row)}</span>
                        {row.hasActiveRun && (
                          <span className="inline-flex items-center rounded-full border border-info-muted bg-info-muted px-2 py-0.5 font-body text-xs text-info">
                            Agent running
                          </span>
                        )}
                        {row.hasUncommittedChanges === true && (
                          <span className="inline-flex items-center rounded-full border border-warning-muted bg-warning-muted px-2 py-0.5 font-body text-xs text-warning">
                            Uncommitted changes
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="whitespace-nowrap px-4 py-3 font-body text-xs text-text-muted">
                      {formatRelativeTime(row.lastUsedAt)}
                    </td>
                    <td className="whitespace-nowrap px-4 py-3 text-right font-body text-sm tabular-nums text-text-secondary">
                      {formatBytes(row.sizeBytes)}
                    </td>
                    <td className="px-4 py-3 text-right">
                      <Button
                        type="button"
                        variant="secondary"
                        disabled={row.hasActiveRun || remove.isPending}
                        title={
                          row.hasActiveRun ? 'Stop the agent run first' : undefined
                        }
                        onClick={() => void confirmAndRemove([row.folderName])}
                      >
                        Remove
                      </Button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      <p className="font-body text-xs text-text-muted">
        Largest first. Select finished ticks worktrees of Done, merged, archived, or
        unmatched folders that have no uncommitted changes.
      </p>

      {selectedCount > 0 && (
        <div className="sticky bottom-4 flex flex-wrap items-center gap-3 rounded-xl border border-border bg-surface-raised px-4 py-3 shadow-card">
          <p className="font-body text-sm text-text-primary">
            {selectedCount} selected
          </p>
          <div className="grow" />
          <Button
            type="button"
            variant="secondary"
            onClick={() => setSelected(new Set())}
          >
            Clear
          </Button>
          <Button
            type="button"
            disabled={remove.isPending}
            onClick={() =>
              void confirmAndRemove(
                [...selected].filter((name) =>
                  filtered.some((row) => row.folderName === name && !row.hasActiveRun),
                ),
              )
            }
          >
            Remove selected
          </Button>
        </div>
      )}
    </div>
  );
}
