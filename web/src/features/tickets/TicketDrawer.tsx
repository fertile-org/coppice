import { useEffect, useState } from 'react';
import { TicketDetailPanel } from './TicketDetailPanel';
import { TicketMetadataPanel } from './TicketMetadataPanel';
import { TicketRunsTab } from './TicketRunsTab';
import { useRepos } from '../repos/useRepos';
import {
  isActiveRunStatus,
  useAgentRuns,
  useRunAgent,
  useStopRun,
} from './useAgentRuns';
import { TicketStatusBadge } from './TicketStatusBadge';
import { useTicket } from './useTicket';

type DrawerTab = 'detail' | 'runs';

interface TicketDrawerProps {
  ticketId: string;
  onClose: () => void;
}

const TAB_LABELS: Record<DrawerTab, string> = {
  detail: 'Detail',
  runs: 'Agent Runs',
};

const TAB_ORDER: DrawerTab[] = ['detail', 'runs'];

export function TicketDrawer({ ticketId, onClose }: TicketDrawerProps) {
  const { data: ticket, isLoading, isError, refetch } = useTicket(ticketId);
  const { data: repos } = useRepos();
  const { data: runs } = useAgentRuns(ticketId);
  const runAgent = useRunAgent(ticketId);
  const stopRun = useStopRun(ticketId);
  const [tab, setTab] = useState<DrawerTab>('detail');
  const [actionError, setActionError] = useState<string | null>(null);

  const selectedRepo = repos?.find((repo) => repo.id === ticket?.repoId);
  const repoNotReady = Boolean(
    ticket?.repoId && selectedRepo && selectedRepo.verificationStatus !== 'ready',
  );
  const canRunAgent = Boolean(
    ticket?.assigneeAgentId && ticket?.repoId && !repoNotReady,
  );
  const activeRun = runs?.find(
    (run) =>
      run.agentId === ticket?.assigneeAgentId &&
      isActiveRunStatus(run.status),
  );
  const runAgentDisabledReason = !ticket?.assigneeAgentId
    ? 'Assign an agent before running.'
    : !ticket?.repoId
      ? 'Link a repository before running.'
      : repoNotReady
        ? 'Repository path is not ready. Ask an admin to verify in Settings → Repositories.'
        : null;

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  useEffect(() => {
    setTab('detail');
    setActionError(null);
  }, [ticketId]);

  async function handleRunAgent() {
    if (!canRunAgent) return;
    setActionError(null);
    try {
      await runAgent.mutateAsync();
    } catch {
      setActionError('Unable to start agent run.');
    }
  }

  async function handleStopRun() {
    if (!activeRun) return;
    setActionError(null);
    try {
      await stopRun.mutateAsync(activeRun.id);
    } catch {
      setActionError('Unable to stop agent run.');
    }
  }

  const headerBusy = runAgent.isPending || stopRun.isPending;

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
        <header className="flex shrink-0 items-center justify-between gap-4 border-b border-border px-6 py-4">
          <div className="min-w-0">
            <p className="font-body text-xs uppercase tracking-wide text-text-muted">
              Ticket
            </p>
            <h2 className="truncate font-display text-xl font-semibold text-text-primary">
              {isLoading ? 'Loading…' : (ticket?.title ?? 'Ticket detail')}
            </h2>
            {ticket && (
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <TicketStatusBadge status={ticket.status} />
                {ticket.substatusDisplay && (
                  <span
                    className="inline-flex items-center rounded-full border border-border bg-surface px-2.5 py-0.5 font-body text-xs text-text-secondary"
                    title={ticket.substatusDisplay.detail}
                  >
                    {ticket.substatusDisplay.label}
                  </span>
                )}
              </div>
            )}
            {repoNotReady && (
              <p className="mt-2 font-body text-sm text-warning">
                Repository path is not ready. Ask an admin to verify in Settings
                → Repositories.
              </p>
            )}
            {actionError && (
              <p className="mt-2 font-body text-sm text-danger">{actionError}</p>
            )}
          </div>

          <div className="flex shrink-0 items-center gap-2">
            {ticket && (
              <>
                <button
                  type="button"
                  onClick={() => void handleRunAgent()}
                  disabled={!canRunAgent || headerBusy}
                  title={runAgentDisabledReason ?? undefined}
                  className="rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-white transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {runAgent.isPending ? 'Starting…' : 'Run Agent'}
                </button>

                {activeRun && (
                  <button
                    type="button"
                    onClick={() => void handleStopRun()}
                    disabled={headerBusy}
                    className="rounded-md border border-danger px-3 py-1.5 font-body text-sm font-medium text-danger transition-colors duration-fast hover:bg-danger-muted/40 disabled:opacity-50"
                  >
                    {stopRun.isPending ? 'Stopping…' : 'Stop'}
                  </button>
                )}
              </>
            )}

            <button
              type="button"
              onClick={onClose}
              className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              Close
            </button>
          </div>
        </header>

        <nav
          role="tablist"
          aria-label="Ticket sections"
          className="flex shrink-0 gap-1 border-b border-border px-6"
        >
          {TAB_ORDER.map((key) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={tab === key}
              onClick={() => setTab(key)}
              className={[
                'border-b-2 px-4 py-3 font-body text-sm transition-colors duration-fast',
                tab === key
                  ? 'border-accent text-accent'
                  : 'border-transparent text-text-secondary hover:text-text-primary',
              ].join(' ')}
            >
              {TAB_LABELS[key]}
            </button>
          ))}
        </nav>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {isLoading && (
            <p className="px-6 py-5 font-body text-sm text-text-muted">
              Loading ticket…
            </p>
          )}

          {isError && (
            <div className="px-6 py-5">
              <div className="rounded-lg border border-danger-muted bg-danger-muted/40 p-4">
                <p className="font-body text-sm text-danger">
                  Unable to load ticket.
                </p>
                <button
                  type="button"
                  onClick={() => void refetch()}
                  className="mt-2 font-body text-sm font-medium text-moss-700 underline-offset-2 hover:underline"
                >
                  Try again
                </button>
              </div>
            </div>
          )}

          {ticket && tab === 'detail' && (
            <div className="grid h-full gap-0 md:grid-cols-[minmax(0,1fr)_minmax(0,16rem)]">
              <div className="min-h-0 overflow-y-auto px-6 py-5">
                <TicketDetailPanel ticket={ticket} />
              </div>
              <div className="min-h-0 overflow-y-auto border-border px-4 py-5 md:border-l">
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
}
