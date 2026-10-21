import { useDraggable } from '@dnd-kit/core';
import { CSS } from '@dnd-kit/utilities';
import { Bot, GitBranch, Network } from 'lucide-react';
import { memo, type CSSProperties } from 'react';
import { PLAN_COPY } from '../plan/copy';
import { planCardState } from '../plan/planCardState';
import { CreatePrButton } from '../tickets/TicketGitActions';
import { useTicketGitInfo } from '../tickets/useTicket';
import { AcceptedNotMerged } from './AcceptedNotMerged';
import { showsAcceptedNotMerged } from './showsAcceptedNotMerged';
import type { TicketHierarchy } from './ticketHierarchy';
import type { Ticket } from './useTickets';

const PRIORITY_LEVELS = new Set(['low', 'medium', 'high', 'critical']);

function priorityBadgeStyle(priority: string): CSSProperties | undefined {
  if (!PRIORITY_LEVELS.has(priority)) return undefined;
  return {
    backgroundColor: `var(--badge-priority-${priority}-bg)`,
    color: `var(--badge-priority-${priority}-text)`,
    borderColor: `var(--badge-priority-${priority}-border)`,
  };
}

function DoneCardPullRequest({ ticketId }: { ticketId: string }) {
  const gitInfo = useTicketGitInfo(ticketId, true);
  if (!gitInfo.data?.canCreatePr) {
    return null;
  }

  return (
    <div
      className="mt-2"
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      <CreatePrButton ticketId={ticketId} size="sm" />
    </div>
  );
}

interface TicketCardProps {
  ticket: Ticket;
  hierarchy?: TicketHierarchy;
  onOpen: (ticketId: string) => void;
  isLive?: boolean;
  /** Resolved agent display name; omit when ticket has no assignee. */
  assigneeName?: string;
  /** Starts the PM planning run. Omit on the drag overlay. */
  onRunPlan?: (ticketId: string) => void;
  planRunPending?: boolean;
  /** Roving tabindex. Defaults to 0 so a card stays keyboard-activatable alone. */
  tabIndex?: number;
  boardFocusId?: string;
  boardTabStop?: boolean;
  onFocusCard?: (ticketId: string) => void;
}

/** Memoized: opening the drawer must not re-render every card on the board. */
export const TicketCard = memo(function TicketCard({
  ticket,
  hierarchy,
  onOpen,
  isLive = false,
  assigneeName,
  onRunPlan,
  planRunPending = false,
  tabIndex = 0,
  boardFocusId,
  boardTabStop = false,
  onFocusCard,
}: TicketCardProps) {
  const isArchived = Boolean(ticket.archivedAt);
  const { attributes, listeners, setNodeRef, transform, isDragging } =
    useDraggable({
      id: ticket.id,
      data: { ticket, type: 'ticket' },
      disabled: isArchived,
    });

  const style = transform
    ? { transform: CSS.Translate.toString(transform) }
    : undefined;
  const planState = planCardState(ticket);
  const planLabel =
    planState === 'writing'
      ? PLAN_COPY.writingPlan
      : planState === 'answers'
        ? PLAN_COPY.waitingForAnswers
        : planState === 'waiting'
          ? PLAN_COPY.waitingForApproval
          : planState === 'needs-run'
            ? PLAN_COPY.planNeedsRun
            : null;

  const showMerge = showsAcceptedNotMerged(ticket);

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={[
        'rounded-md border border-border bg-surface-raised p-3 shadow-card transition-shadow duration-fast',
        'hover:shadow-md',
        isArchived ? 'cursor-pointer opacity-60' : 'cursor-grab',
        isDragging ? 'z-10 opacity-60 shadow-lg' : '',
      ].join(' ')}
    >
      <div
        {...listeners}
        {...attributes}
        role="button"
        tabIndex={tabIndex}
        data-board-focus-id={boardFocusId}
        data-board-tabstop={boardTabStop ? 'true' : undefined}
        onClick={() => {
          if (!isDragging) onOpen(ticket.id);
        }}
        onFocus={() => onFocusCard?.(ticket.id)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            onOpen(ticket.id);
          }
        }}
        className="keyboard-focus rounded-sm"
      >
      {(hierarchy?.parent || hierarchy?.parentUnavailable) && (
        <div className="mb-2 flex min-w-0 items-center gap-1.5 font-body text-xs leading-tight text-text-secondary">
          <GitBranch
            className="h-3.5 w-3.5 shrink-0"
            aria-hidden="true"
          />
          {hierarchy.parent ? (
            <span
              className="min-w-0 truncate"
              title={hierarchy.parent.title}
            >
              Child of {hierarchy.parent.title}
            </span>
          ) : (
            <span className="min-w-0 truncate">
              Child ticket · Parent unavailable
            </span>
          )}
        </div>
      )}

      <p className="flex items-start gap-1.5 font-body text-sm font-medium leading-snug text-text-primary">
        {isLive && (
          <span
            className="mt-1.5 inline-block h-2 w-2 shrink-0 animate-pulse rounded-full bg-accent"
            aria-label="Agent running"
          />
        )}
        <span className="min-w-0 flex-1">{ticket.title}</span>
        {isArchived && (
          <span className="shrink-0 rounded-full border border-border bg-paper-200 px-2 py-0.5 font-body text-[0.65rem] font-normal uppercase tracking-wide text-text-muted">
            Archived
          </span>
        )}
      </p>

      {assigneeName && (
        <div className="mt-1.5 flex min-w-0 items-center gap-1.5 font-body text-xs leading-tight text-text-secondary">
          <Bot className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span className="min-w-0 truncate" title={assigneeName}>
            {assigneeName}
          </span>
        </div>
      )}

      {(ticket.substatusDisplay || ticket.priority) && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {ticket.substatusDisplay && (
            <span
              className="inline-flex max-w-full items-center gap-1 rounded-full border border-info-muted bg-info-muted px-2 py-0.5 font-body text-xs text-info"
              title={ticket.substatusDisplay.detail}
            >
              <span className="truncate">{ticket.substatusDisplay.label}</span>
              {ticket.substatusDisplay.detail && (
                <span className="truncate font-mono text-[0.65rem] opacity-80">
                  · {ticket.substatusDisplay.detail}
                </span>
              )}
            </span>
          )}
          {ticket.priority && (
            <span
              className={[
                'rounded-full border px-2 py-0.5 font-body text-xs capitalize',
                PRIORITY_LEVELS.has(ticket.priority)
                  ? ''
                  : 'border-border bg-paper-200 text-text-secondary',
              ].join(' ')}
              style={priorityBadgeStyle(ticket.priority)}
              data-priority={ticket.priority}
            >
              {ticket.priority}
            </span>
          )}
        </div>
      )}

      {planLabel && (
        <div className="mt-2">
          <p className="font-body text-xs leading-tight text-text-secondary">
            {planLabel}
          </p>
          {planState === 'needs-run' && onRunPlan && (
            <button
              type="button"
              disabled={planRunPending}
              onPointerDown={(event) => event.stopPropagation()}
              onClick={(event) => {
                event.stopPropagation();
                onRunPlan(ticket.id);
              }}
              className="mt-1.5 rounded-md border border-border bg-surface px-2 py-1 font-body text-xs font-medium text-text-primary hover:bg-paper-200 disabled:opacity-50"
            >
              {PLAN_COPY.runAgent}
            </button>
          )}
        </div>
      )}

      {hierarchy && hierarchy.directChildCount > 0 && (
        <div className="mt-2 flex min-w-0 items-center gap-1.5 font-body text-xs leading-tight text-text-secondary">
          <Network className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span className="min-w-0 truncate">
            Parent · {hierarchy.directChildCount} children ·{' '}
            {hierarchy.doneChildCount}/{hierarchy.directChildCount} done
          </span>
        </div>
      )}
      </div>
      {showMerge && (
        <AcceptedNotMerged ticket={ticket} shortcutActive={boardTabStop} />
      )}
      {ticket.status === 'done' && ticket.repoId && !isArchived && (
        <DoneCardPullRequest ticketId={ticket.id} />
      )}
    </div>
  );
});

/** Resolve assignee label for a card; returns undefined when unassigned. */
export function resolveAssigneeName(
  assigneeAgentId: string | undefined,
  agentsById: Map<string, string>,
): string | undefined {
  if (!assigneeAgentId) return undefined;
  return agentsById.get(assigneeAgentId) ?? 'Unknown agent';
}
