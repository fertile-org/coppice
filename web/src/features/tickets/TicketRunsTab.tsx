import { useState } from 'react';
import { useAgents } from '../agents/useAgents';
import type { AgentRun, RunStatus } from '../../lib/schemas/agentRun';
import { useAgentRuns } from './useAgentRuns';

interface TicketRunsTabProps {
  ticketId: string;
}

const RUN_STATUS_LABELS: Record<RunStatus, string> = {
  queued: 'Queued',
  running: 'Running',
  succeeded: 'Succeeded',
  failed: 'Failed',
  blocked: 'Blocked',
  cancelled: 'Cancelled',
};

function runStatusPillClass(status: RunStatus): string {
  const base =
    'inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 font-body text-xs capitalize';
  switch (status) {
    case 'queued':
      return `${base} border-info-muted bg-info-muted text-info`;
    case 'running':
      return `${base} border-accent-muted bg-accent-muted text-accent`;
    case 'succeeded':
      return `${base} border-success-muted bg-success-muted text-success`;
    case 'failed':
      return `${base} border-danger-muted bg-danger-muted/40 text-danger`;
    case 'blocked':
      return `${base} border-border bg-paper-200 text-text-secondary`;
    case 'cancelled':
      return `${base} border-border bg-surface text-text-muted`;
  }
}

function formatRelativeTime(iso: string | null): string {
  if (!iso) return '—';
  try {
    const date = new Date(iso);
    const diffMs = date.getTime() - Date.now();
    const absSec = Math.round(Math.abs(diffMs) / 1000);
    const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

    if (absSec < 60) return rtf.format(Math.round(diffMs / 1000), 'second');
    if (absSec < 3600) return rtf.format(Math.round(diffMs / 60000), 'minute');
    if (absSec < 86400) return rtf.format(Math.round(diffMs / 3600000), 'hour');
    return rtf.format(Math.round(diffMs / 86400000), 'day');
  } catch {
    return iso;
  }
}

function truncatePath(path: string, max = 48): string {
  if (path.length <= max) return path;
  return `…${path.slice(-(max - 1))}`;
}

function agentName(
  agentId: string,
  agents: { id: string; name: string }[] | undefined,
): string {
  return agents?.find((agent) => agent.id === agentId)?.name ?? 'Unknown agent';
}

function RunRow({
  run,
  agents,
}: {
  run: AgentRun;
  agents: { id: string; name: string }[] | undefined;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <article className="rounded-md border border-border bg-surface px-4 py-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className={runStatusPillClass(run.status)}>
              {RUN_STATUS_LABELS[run.status]}
            </span>
            <span className="font-body text-sm font-medium text-text-primary">
              {agentName(run.agentId, agents)}
            </span>
          </div>
          <p className="font-body text-xs text-text-muted">
            Started {formatRelativeTime(run.startedAt)}
            {run.endedAt && <> · Ended {formatRelativeTime(run.endedAt)}</>}
          </p>
        </div>
        <time
          dateTime={run.createdAt}
          className="shrink-0 font-body text-xs text-text-muted"
          title={run.createdAt}
        >
          {formatRelativeTime(run.createdAt)}
        </time>
      </div>

      {(run.branchName || run.worktreePath) && (
        <dl className="mt-3 space-y-1 font-mono text-xs text-text-secondary">
          {run.branchName && (
            <div className="flex gap-2">
              <dt className="shrink-0 text-text-muted">Branch</dt>
              <dd className="truncate" title={run.branchName}>
                {run.branchName}
              </dd>
            </div>
          )}
          {run.worktreePath && (
            <div className="flex gap-2">
              <dt className="shrink-0 text-text-muted">Worktree</dt>
              <dd className="truncate" title={run.worktreePath}>
                {truncatePath(run.worktreePath)}
              </dd>
            </div>
          )}
        </dl>
      )}

      {run.errorMessage && (
        <div className="mt-3">
          <button
            type="button"
            onClick={() => setExpanded((value) => !value)}
            className="font-body text-xs font-medium text-danger underline-offset-2 hover:underline"
          >
            {expanded ? 'Hide error' : 'Show error'}
          </button>
          {expanded && (
            <pre className="mt-2 overflow-x-auto rounded-md border border-danger-muted bg-danger-muted/30 p-2 font-mono text-xs text-danger whitespace-pre-wrap">
              {run.errorMessage}
            </pre>
          )}
        </div>
      )}
    </article>
  );
}

export function TicketRunsTab({ ticketId }: TicketRunsTabProps) {
  const { data: runs, isLoading, isError } = useAgentRuns(ticketId);
  const { data: agents } = useAgents();

  if (isLoading) {
    return (
      <p className="font-body text-sm text-text-muted">Loading runs…</p>
    );
  }

  if (isError) {
    return (
      <p className="font-body text-sm text-danger">Unable to load runs.</p>
    );
  }

  if ((runs?.length ?? 0) === 0) {
    return (
      <p className="font-body text-sm text-text-muted">
        No agent runs yet. Use Run Agent in the header to start one.
      </p>
    );
  }

  return (
    <div className="space-y-3">
      {runs?.map((run) => (
        <RunRow key={run.id} run={run} agents={agents} />
      ))}
    </div>
  );
}
