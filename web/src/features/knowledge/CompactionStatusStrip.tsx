import { AlertTriangle, Loader2, PauseCircle, Sparkles, X } from 'lucide-react';
import { useEffect, useState, type ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { ApiError } from '../../lib/api';
import type { CompactionBatch, CompactionStatus } from '../../lib/schemas/knowledge';
import { useConnectors } from '../agents/useAgents';
import { LiveRunView } from '../runs/LiveRunView';
import { formatNotificationTimestamp } from '../notifications/notification-format';
import { formatInterval, pluralTickets } from './compactionFormat';
import {
  useCancelCompaction,
  useCompactNow,
  useCompactionStatus,
  useRetryCompaction,
} from './useCompaction';

const AGENTS_ANCHOR_PATH = '/agents#knowledge-compaction';
const MAX_ERROR_CHARS = 180;

function relative(iso: string | null): string {
  return iso ? formatNotificationTimestamp(iso).toLowerCase() : 'never';
}

function nextRun(iso: string | null): string | null {
  if (!iso) return null;
  return new Date(iso).getTime() <= Date.now() ? 'soon' : relative(iso);
}

function truncate(value: string): string {
  return value.length > MAX_ERROR_CHARS ? `${value.slice(0, MAX_ERROR_CHARS - 1)}…` : value;
}

function actionError(error: unknown): string {
  if (error instanceof ApiError) {
    try {
      const body = JSON.parse(error.body) as { message?: string };
      if (body.message) return body.message;
    } catch {
      // fall through
    }
  }
  return 'Knowledge compaction request failed.';
}

function RunDialog({
  batch,
  connector,
  onClose,
}: {
  batch: CompactionBatch;
  connector: string | null;
  onClose: () => void;
}) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [onClose]);
  const { data: connectors } = useConnectors();
  const liveConsole = connectors?.find((c) => c.id === connector)?.console;
  const active = batch.status === 'queued' || batch.status === 'running';
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay px-4"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="compaction-run-title"
        className="flex max-h-[85vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-border bg-paper-50 shadow-lg"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-center justify-between gap-3 border-b border-border px-5 py-3">
          <h2 id="compaction-run-title" className="font-display text-lg font-semibold text-bark-900">
            Compaction run · {pluralTickets(batch.ticketCount)}
          </h2>
          <button
            type="button"
            aria-label="Close"
            onClick={onClose}
            className="rounded-md p-1 text-text-secondary hover:bg-paper-200 hover:text-text-primary"
          >
            <X className="size-4" aria-hidden="true" />
          </button>
        </div>
        <div className="min-h-[20rem] flex-1 overflow-auto">
          <LiveRunView
            console={liveConsole}
            runId={batch.runId}
            runStatus={active ? batch.status : batch.status === 'succeeded' ? 'succeeded' : 'failed'}
            shouldReconnect={active}
            startedAt={batch.startedAt}
          />
        </div>
      </div>
    </div>
  );
}

function Strip({
  tone,
  icon,
  children,
  actions,
}: {
  tone: 'warning' | 'muted' | 'running' | 'danger';
  icon: ReactNode;
  children: ReactNode;
  actions?: ReactNode;
}) {
  const toneClass = {
    warning: 'border-amber-200 bg-amber-100 text-amber-900',
    muted: 'border-border bg-surface text-text-secondary',
    running: 'border-moss-200 bg-moss-50 text-moss-800',
    danger: 'border-danger-muted bg-danger-muted text-danger',
  }[tone];
  return (
    <div
      role={tone === 'danger' || tone === 'warning' ? 'alert' : 'status'}
      data-testid="compaction-status"
      className={`mt-6 flex flex-wrap items-center justify-between gap-3 rounded-lg border px-4 py-3 ${toneClass}`}
    >
      <div className="flex min-w-0 flex-1 items-start gap-2 font-body text-sm">
        <span className="mt-0.5 shrink-0">{icon}</span>
        <p className="min-w-0">{children}</p>
      </div>
      {actions && <div className="flex shrink-0 flex-wrap gap-2">{actions}</div>}
    </div>
  );
}

export function CompactionStatusStrip() {
  const navigate = useNavigate();
  const { data: status } = useCompactionStatus();
  const compactNow = useCompactNow();
  const retry = useRetryCompaction();
  const cancel = useCancelCompaction();
  const [viewBatch, setViewBatch] = useState<CompactionBatch | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (!status) return null;

  async function act(mutation: { mutateAsync: () => Promise<unknown> }) {
    setError(null);
    try {
      await mutation.mutateAsync();
    } catch (err) {
      setError(actionError(err));
    }
  }

  const busy = compactNow.isPending || retry.isPending || cancel.isPending;
  const agentName = status.agent?.name ?? 'The compaction agent';
  const strip = renderState(status);

  function renderState(current: CompactionStatus) {
    const waiting = pluralTickets(current.queuedCount, 'Done ticket');
    switch (current.state) {
      case 'not_configured':
        return (
          <Strip
            tone="warning"
            icon={<AlertTriangle className="size-4" aria-hidden="true" />}
            actions={
              <Button type="button" size="sm" onClick={() => void navigate(AGENTS_ANCHOR_PATH)}>
                Choose agent
              </Button>
            }
          >
            Knowledge compaction is off. {waiting} {current.queuedCount === 1 ? 'is' : 'are'}{' '}
            waiting; choose a compaction agent to turn them into knowledge candidates.
          </Strip>
        );
      case 'agent_disabled':
        return (
          <Strip
            tone="warning"
            icon={<PauseCircle className="size-4" aria-hidden="true" />}
            actions={
              <Button type="button" size="sm" variant="secondary" onClick={() => void navigate(AGENTS_ANCHOR_PATH)}>
                Open agents
              </Button>
            }
          >
            Compaction is paused — {agentName} is disabled. {pluralTickets(current.queuedCount)} waiting.
          </Strip>
        );
      case 'running': {
        const batch = current.activeBatch!;
        return (
          <Strip
            tone="running"
            icon={<Loader2 className="size-4 animate-spin" aria-hidden="true" />}
            actions={
              <>
                {batch.runId && (
                  <Button type="button" size="sm" variant="secondary" onClick={() => setViewBatch(batch)}>
                    View run
                  </Button>
                )}
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void act(cancel)}
                >
                  Cancel
                </Button>
              </>
            }
          >
            {agentName} is compacting {pluralTickets(batch.ticketCount)}… started{' '}
            {relative(batch.startedAt ?? batch.createdAt)}
          </Strip>
        );
      }
      case 'failed': {
        const batch = current.lastBatch!;
        const cancelled = batch.errorMessage === 'cancelled';
        return (
          <Strip
            tone="danger"
            icon={<AlertTriangle className="size-4" aria-hidden="true" />}
            actions={
              <>
                {cancelled ? (
                  <Button
                    type="button"
                    size="sm"
                    disabled={busy || current.queuedCount === 0}
                    onClick={() => void act(compactNow)}
                  >
                    Compact now
                  </Button>
                ) : (
                  <Button type="button" size="sm" disabled={busy} onClick={() => void act(retry)}>
                    Retry
                  </Button>
                )}
                {batch.runId && (
                  <Button type="button" size="sm" variant="secondary" onClick={() => setViewBatch(batch)}>
                    View run
                  </Button>
                )}
              </>
            }
          >
            {cancelled
              ? `Last compaction was cancelled. ${pluralTickets(batch.ticketCount)} not compacted.`
              : `Last compaction failed: ${truncate(batch.errorMessage ?? 'unknown error')}. ${pluralTickets(batch.ticketCount)} not compacted.`}
          </Strip>
        );
      }
      case 'idle': {
        const last = current.lastBatch;
        const next = nextRun(current.nextScheduledAt);
        const parts = [
          `Compacted by ${agentName} every ${formatInterval(current.intervalSecs)}`,
          `${pluralTickets(current.queuedCount)} waiting`,
          last
            ? `last run ${relative(last.endedAt ?? last.createdAt)}, ${pluralTickets(last.candidateCount ?? 0, 'candidate')}`
            : 'no runs yet',
          ...(next ? [`next run ${next}`] : []),
          ...(current.blockedCount > 0
            ? [`${pluralTickets(current.blockedCount)} need a retry`]
            : []),
        ];
        return (
          <Strip
            tone="muted"
            icon={<Sparkles className="size-4 text-moss-700" aria-hidden="true" />}
            actions={
              <>
                {current.blockedCount > 0 && (
                  <Button type="button" size="sm" variant="secondary" disabled={busy} onClick={() => void act(retry)}>
                    Retry
                  </Button>
                )}
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  disabled={busy || current.queuedCount === 0}
                  onClick={() => void act(compactNow)}
                >
                  Compact now
                </Button>
              </>
            }
          >
            {parts.join(' · ')}
          </Strip>
        );
      }
    }
  }

  return (
    <>
      {strip}
      {error && (
        <p role="alert" className="mt-2 font-body text-sm text-danger">
          {error}
        </p>
      )}
      {viewBatch && (
        <RunDialog
          batch={viewBatch}
          connector={status.agent?.connector ?? null}
          onClose={() => setViewBatch(null)}
        />
      )}
    </>
  );
}
