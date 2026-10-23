import { useEffect, useRef, useState, type AnimationEvent } from 'react';
import type { TicketParentSummary } from '../board/ticketHierarchy';
import { useConnectors } from '../agents/useAgents';
import { LiveRunView } from '../runs/LiveRunView';
import { TicketDetailPanel } from './TicketDetailPanel';
import { TicketMetadataPanel } from './TicketMetadataPanel';
import { TicketRunsTab } from './TicketRunsTab';
import { useRepos } from '../repos/useRepos';
import {
  isActiveRunStatus,
  shouldPollRunForReconciliation,
  useAgentRuns,
  useRunAgent,
  useStopRun,
} from './useAgentRuns';
import { TicketStatusBadge } from './TicketStatusBadge';
import { Sheet, SheetContent, SheetTitle } from '../../components/ui/sheet';
import { acceptShortcutEnabled } from '../board/ticketShortcutRules';
import {
  DRAWER_SECTION_LABELS,
  stepDrawerSection,
  type DrawerSection,
} from '../keyboard/drawerSections';
import {
  useKeyboardLayer,
  useRegisterShortcut,
} from '../keyboard/useKeyboard';
import { parseApiErrorMessage } from '../../lib/api';
import { AcceptTicketDialog } from './AcceptTicketDialog';
import {
  useArchiveTicket,
  useFinalApprove,
  useTicket,
  useUnarchiveTicket,
} from './useTicket';

type DrawerTab = DrawerSection;

interface TicketDrawerProps {
  /** Keep the drawer mounted and toggle this so the exit animation plays. */
  open?: boolean;
  ticketId: string;
  parentTicket?: TicketParentSummary | null;
  onClose: () => void;
  /** Focus the card that opened the drawer. */
  onRestoreFocus?: () => void;
}

const TAB_ORDER: DrawerTab[] = ['detail', 'live', 'runs'];

/** The slide is 240ms; settle anyway if `animationend` never arrives. */
const SETTLE_FALLBACK_MS = 300;

function expectsSlide(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return false;
  }
  return !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/**
 * False while the sheet slides in, so heavy content (comments, the TipTap
 * composer) mounts after the slide instead of competing with it. Switching
 * tickets while open has no slide, so it settles right away.
 */
function useSheetSettled(open: boolean, ticketId: string) {
  const [settled, setSettled] = useState(() => !open || !expectsSlide());
  const [prevOpen, setPrevOpen] = useState(open);
  const [prevTicketId, setPrevTicketId] = useState(ticketId);
  if (open !== prevOpen || ticketId !== prevTicketId) {
    const opening = open && !prevOpen;
    setPrevOpen(open);
    setPrevTicketId(ticketId);
    if (opening) setSettled(!expectsSlide());
    else if (open) setSettled(true);
  }

  useEffect(() => {
    if (!open || settled) return;
    const timer = window.setTimeout(() => setSettled(true), SETTLE_FALLBACK_MS);
    return () => window.clearTimeout(timer);
  }, [open, settled]);

  function onAnimationEnd(event: AnimationEvent<HTMLDivElement>) {
    // Child animations (fade-in, spinners) bubble up here too.
    if (event.target !== event.currentTarget) return;
    if (event.animationName !== 'sheet-in-right') return;
    setSettled(true);
  }

  return { settled, onAnimationEnd };
}

export function TicketDrawer({
  open = true,
  onRestoreFocus,
  ...panelProps
}: TicketDrawerProps) {
  const { settled, onAnimationEnd } = useSheetSettled(open, panelProps.ticketId);
  return (
    <Sheet open={open} onOpenChange={(next) => !next && panelProps.onClose()}>
      <SheetContent
        aria-label="Ticket detail"
        aria-labelledby={undefined}
        aria-describedby={undefined}
        className="w-[90%] max-w-[90vw]"
        data-shortcut-layer="ticket"
        onAnimationEnd={onAnimationEnd}
        onCloseAutoFocus={(event) => {
          if (!onRestoreFocus) return;
          event.preventDefault();
          onRestoreFocus();
        }}
      >
        <TicketDrawerPanel {...panelProps} settled={settled} />
      </SheetContent>
    </Sheet>
  );
}

function TicketDrawerPanel({
  ticketId,
  parentTicket = null,
  onClose,
  settled,
}: Omit<TicketDrawerProps, 'open' | 'onRestoreFocus'> & { settled: boolean }) {
  const {
    data: ticket,
    isLoading,
    isError,
    isPlaceholderData,
    refetch,
  } = useTicket(ticketId, { placeholderFromList: true });
  // List data lacks the description and detail fields: show the header only.
  const detailLoaded = Boolean(ticket) && !isPlaceholderData;
  const { data: repos } = useRepos();
  const { data: runs } = useAgentRuns(ticketId);
  const { data: connectors } = useConnectors();
  const runAgent = useRunAgent(ticketId);
  const stopRun = useStopRun(ticketId);
  const finalApprove = useFinalApprove(ticketId);
  const archiveTicket = useArchiveTicket(ticketId);
  const unarchiveTicket = useUnarchiveTicket(ticketId);
  const [tab, setTab] = useState<DrawerTab>('detail');
  const [tabTicketId, setTabTicketId] = useState(ticketId);
  const [sectionFocusTick, setSectionFocusTick] = useState(0);
  const [actionError, setActionError] = useState<string | null>(null);
  const [acceptOpen, setAcceptOpen] = useState(false);
  if (ticketId !== tabTicketId) {
    setTabTicketId(ticketId);
    setTab('detail');
    setActionError(null);
  }
  const tabRefs = useRef<Record<DrawerTab, HTMLButtonElement | null>>({
    detail: null,
    live: null,
    runs: null,
  });

  const selectedRepo = repos?.find((repo) => repo.id === ticket?.repoId);
  const repoNotReady = Boolean(
    ticket?.repoId && selectedRepo && selectedRepo.verificationStatus !== 'ready',
  );
  const isArchived = Boolean(ticket?.archivedAt);
  const canRunAgent = Boolean(
    ticket?.assigneeAgentId &&
      ticket?.repoId &&
      !repoNotReady &&
      !isArchived,
  );
  const activeRun = runs?.find((run) => isActiveRunStatus(run.status));
  const reconnectableRun = runs?.find(shouldPollRunForReconciliation);
  const latestRun = runs?.[0] ?? null;
  const liveRun = activeRun ?? reconnectableRun ?? latestRun;
  const shouldReconnectLiveRun = liveRun
    ? shouldPollRunForReconciliation(liveRun)
    : false;
  // A run's own format wins: OpenCode 1.x and 2.x runs render differently.
  const liveConsole =
    liveRun?.consoleKind ??
    connectors?.find((c) => c.id === liveRun?.connector)?.console;
  const runAgentDisabledReason = isArchived
    ? 'Unarchive this ticket before running an agent.'
    : !ticket?.assigneeAgentId
      ? 'Assign an agent before running.'
      : !ticket?.repoId
        ? 'Link a repository before running.'
        : repoNotReady
          ? 'Repository path is not ready. Ask an admin to verify in Settings → Repositories.'
          : null;

  function moveSection(direction: 1 | -1) {
    setTab((current) => stepDrawerSection(current, direction));
    setSectionFocusTick((tick) => tick + 1);
  }

  useEffect(() => {
    if (sectionFocusTick === 0) return;
    tabRefs.current[tab]?.focus();
  }, [sectionFocusTick, tab]);

  useRegisterShortcut('drawer.nextSection', () => moveSection(1));
  useRegisterShortcut('drawer.previousSection', () => moveSection(-1));
  useRegisterShortcut(
    'review.accept',
    () => setAcceptOpen(true),
    detailLoaded && acceptShortcutEnabled(ticket),
  );
  useKeyboardLayer({
    focusDrawerTabs: () => {
      const button = tabRefs.current[tab];
      button?.focus();
      return Boolean(button);
    },
  });

  async function handleRunAgent() {
    if (!canRunAgent) return;
    setActionError(null);
    try {
      await runAgent.mutateAsync();
      setTab('live');
    } catch (err) {
      setActionError(parseApiErrorMessage(err, 'Unable to start agent run.'));
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

  async function handleFinalApprove() {
    setActionError(null);
    try {
      await finalApprove.mutateAsync();
    } catch {
      setActionError('Unable to accept ticket.');
    }
  }

  async function handleArchive() {
    setActionError(null);
    try {
      await archiveTicket.mutateAsync();
    } catch {
      setActionError(
        ticket?.hasActiveRun
          ? 'Cannot archive while an agent run is queued or running.'
          : 'Unable to archive ticket.',
      );
    }
  }

  async function handleUnarchive() {
    setActionError(null);
    try {
      await unarchiveTicket.mutateAsync();
    } catch {
      setActionError('Unable to unarchive ticket.');
    }
  }

  const headerBusy =
    !detailLoaded ||
    runAgent.isPending ||
    stopRun.isPending ||
    finalApprove.isPending ||
    archiveTicket.isPending ||
    unarchiveTicket.isPending;
  const archiveDisabledReason = ticket?.hasActiveRun
    ? 'Stop or wait for the active agent run before archiving.'
    : null;

  return (
    <>
      <header className="flex shrink-0 items-end justify-between gap-4 border-b border-border px-6 py-4">
        <div className="min-w-0">
          <p className="font-body text-xs uppercase tracking-wide text-text-muted">
            Ticket
          </p>
          <SheetTitle className="truncate text-text-primary">
            {isLoading ? 'Loading…' : (ticket?.title ?? 'Ticket detail')}
          </SheetTitle>
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
                className="rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
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

              {ticket.status === 'wait_for_final_review' && !isArchived && (
                <button
                  type="button"
                  onClick={() => void handleFinalApprove()}
                  disabled={headerBusy}
                  className="rounded-md bg-moss-600 px-3 py-1.5 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-500 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {finalApprove.isPending ? 'Accepting…' : 'Accept'}
                </button>
              )}

              {isArchived ? (
                <button
                  type="button"
                  onClick={() => void handleUnarchive()}
                  disabled={headerBusy}
                  className="rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-paper-200 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {unarchiveTicket.isPending ? 'Unarchiving…' : 'Unarchive'}
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => void handleArchive()}
                  disabled={headerBusy || Boolean(ticket.hasActiveRun)}
                  title={archiveDisabledReason ?? undefined}
                  className="rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {archiveTicket.isPending ? 'Archiving…' : 'Archive'}
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

      {ticket && isArchived && (
        <div
          role="status"
          className="shrink-0 border-b border-border bg-paper-200 px-6 py-3 font-body text-sm text-text-secondary"
        >
          This ticket is archived. Unarchive it before changing status,
          assigning agents, running work, or updating fields.
        </div>
      )}

      <nav
        role="tablist"
        aria-label="Ticket sections"
        className="flex shrink-0 gap-1 border-b border-border px-6"
      >
        {TAB_ORDER.map((key) => (
          <button
            key={key}
            ref={(node) => {
              tabRefs.current[key] = node;
            }}
            id={`ticket-section-${key}`}
            type="button"
            role="tab"
            aria-selected={tab === key}
            onClick={() => setTab(key)}
            className={[
              'keyboard-focus border-b-2 px-4 py-3 font-body text-sm transition-colors duration-fast',
              tab === key
                ? 'border-accent text-accent'
                : 'border-transparent text-text-secondary hover:text-text-primary',
            ].join(' ')}
          >
            {DRAWER_SECTION_LABELS[key]}
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

        {ticket && !detailLoaded && tab === 'detail' && (
          <div
            data-testid="ticket-detail-skeleton"
            aria-busy="true"
            className="h-full min-h-64 px-6 py-5"
          >
            <span className="sr-only">Loading ticket…</span>
          </div>
        )}

        {ticket && detailLoaded && tab === 'detail' && (
          <div className="grid h-full gap-0 md:grid-cols-[minmax(0,1fr)_minmax(0,calc(16rem+50px))]">
            <div className="min-h-0 overflow-y-auto px-6 py-5">
              <TicketDetailPanel
                ticket={ticket}
                parentTicket={parentTicket}
                commentsReady={settled}
              />
            </div>
            <div className="min-h-0 overflow-y-auto border-border px-4 py-5 md:border-l">
              <TicketMetadataPanel ticket={ticket} />
            </div>
          </div>
        )}

        {ticket && tab === 'live' && (
          <div className="flex h-full min-h-0 flex-col px-6 py-5">
            <LiveRunView
              console={liveConsole}
              runId={liveRun?.id ?? null}
              runStatus={liveRun?.status ?? null}
              shouldReconnect={shouldReconnectLiveRun}
              startedAt={liveRun?.startedAt ?? null}
            />
          </div>
        )}

        {ticket && tab === 'runs' && (
          <div className="px-6 py-5">
            <TicketRunsTab ticketId={ticket.id} />
          </div>
        )}
      </div>
      {acceptOpen && ticket && (
        <AcceptTicketDialog
          ticketId={ticket.id}
          open
          onClose={() => setAcceptOpen(false)}
        />
      )}
    </>
  );
}
