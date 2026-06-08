import { useEffect, useState } from 'react';
import { TicketCommentsTab } from './TicketCommentsTab';
import { TicketDescriptionTab } from './TicketDescriptionTab';
import { TicketMetadataTab } from './TicketMetadataTab';
import { TicketRunsTab } from './TicketRunsTab';
import { useRepos } from '../repos/useRepos';
import {
  isActiveRunStatus,
  useAgentRuns,
  useRunAgent,
  useStopRun,
} from './useAgentRuns';
import { statusLabel, useTicket } from './useTicket';

type DrawerTab = 'description' | 'comments' | 'runs' | 'metadata';

interface TicketDrawerProps {
  ticketId: string;
  onClose: () => void;
}

const TAB_LABELS: Record<DrawerTab, string> = {
  description: 'Description',
  comments: 'Comments',
  runs: 'Runs',
  metadata: 'Metadata',
};

const TAB_ORDER: DrawerTab[] = ['description', 'comments', 'runs', 'metadata'];

export function TicketDrawer({ ticketId, onClose }: TicketDrawerProps) {
  const { data: ticket, isLoading, isError, refetch } = useTicket(ticketId);
  const { data: repos } = useRepos();
  const { data: runs } = useAgentRuns(ticketId);
  const runAgent = useRunAgent(ticketId);
  const stopRun = useStopRun(ticketId);
  const [tab, setTab] = useState<DrawerTab>('description');
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
    setTab('description');
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
    <div
      className="fixed inset-0 z-50 flex bg-bark-950/40 backdrop-blur-[1px]"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Ticket detail"
        className="flex h-full w-full flex-col bg-surface-raised shadow-2xl"
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
              <p className="mt-1 font-body text-sm text-text-secondary">
                {statusLabel(ticket.status)}
                {ticket.substatusDisplay && (
                  <>
                    {' · '}
                    <span title={ticket.substatusDisplay.detail}>
                      {ticket.substatusDisplay.label}
                    </span>
                  </>
                )}
              </p>
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
          className="flex shrink-0 gap-1 border-b border-border px-6"
          aria-label="Ticket sections"
        >
          {TAB_ORDER.map((key) => (
            <button
              key={key}
              type="button"
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

        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          {isLoading && (
            <p className="font-body text-sm text-text-muted">Loading ticket…</p>
          )}

          {isError && (
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
          )}

          {ticket && tab === 'description' && (
            <TicketDescriptionTab ticket={ticket} />
          )}

          {ticket && tab === 'comments' && (
            <TicketCommentsTab ticketId={ticket.id} />
          )}

          {ticket && tab === 'runs' && <TicketRunsTab ticketId={ticket.id} />}

          {ticket && tab === 'metadata' && <TicketMetadataTab ticket={ticket} />}
        </div>
      </div>
    </div>
  );
}
