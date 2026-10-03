import { useId, useState } from 'react';
import { useAgents } from '../agents/useAgents';
import { KnowledgeUsed } from '../knowledge/KnowledgeUsed';
import { RunToolsAndSkills } from '../runs/RunToolsAndSkills';
import type { AgentRun, RunStatus } from '../../lib/schemas/agentRun';
import { useOpenTicket } from './useOpenTicket';
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
  const [errorExpanded, setErrorExpanded] = useState(false);
  const [detailsExpanded, setDetailsExpanded] = useState(false);
  const openTicket = useOpenTicket();

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
          {run.status !== 'failed' && (
            <button
              type="button"
              onClick={() => setErrorExpanded((value) => !value)}
              className="font-body text-xs font-medium text-danger underline-offset-2 hover:underline"
            >
              {errorExpanded ? 'Hide error' : 'Show error'}
            </button>
          )}
          {(errorExpanded || run.status === 'failed') && (
            <pre className="mt-2 overflow-x-auto rounded-md border border-danger-muted bg-danger-muted/30 p-2 font-mono text-xs text-danger whitespace-pre-wrap">
              {run.errorMessage}
            </pre>
          )}
        </div>
      )}

      <button
        type="button"
        onClick={() => setDetailsExpanded((value) => !value)}
        aria-expanded={detailsExpanded}
        className="mt-3 font-body text-xs font-medium text-moss-700 underline-offset-2 hover:underline"
      >
        {detailsExpanded ? 'Hide run details' : 'Show run details'}
      </button>
      {detailsExpanded && (
        <RunDetailsTabs runId={run.id} onOpenTicket={openTicket} />
      )}
    </article>
  );
}

type RunDetailsTab = 'tools' | 'knowledge';

const RUN_DETAILS_TABS: { key: RunDetailsTab; label: string }[] = [
  { key: 'tools', label: 'Tools & Skills' },
  { key: 'knowledge', label: 'Knowledge Used' },
];

function RunDetailsTabs({
  runId,
  onOpenTicket,
}: {
  runId: string;
  onOpenTicket: (ticketId: string) => void | Promise<void>;
}) {
  const [tab, setTab] = useState<RunDetailsTab>('tools');
  const idPrefix = useId();
  const tabId = (key: RunDetailsTab) => `${idPrefix}-tab-${key}`;
  const panelId = (key: RunDetailsTab) => `${idPrefix}-panel-${key}`;

  return (
    <div className="mt-3 border-t border-border pt-2">
      <div
        role="tablist"
        aria-label="Run details"
        className="flex gap-1 border-b border-border"
      >
        {RUN_DETAILS_TABS.map(({ key, label }) => (
          <button
            key={key}
            id={tabId(key)}
            type="button"
            role="tab"
            aria-selected={tab === key}
            aria-controls={panelId(key)}
            onClick={() => setTab(key)}
            className={[
              'border-b-2 px-3 py-2 font-body text-xs transition-colors duration-fast',
              tab === key
                ? 'border-accent text-accent'
                : 'border-transparent text-text-secondary hover:text-text-primary',
            ].join(' ')}
          >
            {label}
          </button>
        ))}
      </div>
      <div
        id={panelId('tools')}
        role="tabpanel"
        aria-labelledby={tabId('tools')}
        hidden={tab !== 'tools'}
        className="pt-3"
      >
        <RunToolsAndSkills runId={runId} enabled={tab === 'tools'} />
      </div>
      <div
        id={panelId('knowledge')}
        role="tabpanel"
        aria-labelledby={tabId('knowledge')}
        hidden={tab !== 'knowledge'}
      >
        <KnowledgeUsed
          runId={runId}
          enabled={tab === 'knowledge'}
          onOpenTicket={onOpenTicket}
        />
      </div>
    </div>
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
