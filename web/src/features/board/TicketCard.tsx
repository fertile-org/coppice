import { useDraggable } from '@dnd-kit/core';
import { CSS } from '@dnd-kit/utilities';
import { Bot, GitBranch, Network } from 'lucide-react';
import type { CSSProperties } from 'react';
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

interface TicketCardProps {
  ticket: Ticket;
  hierarchy?: TicketHierarchy;
  onOpen: (ticketId: string) => void;
  isLive?: boolean;
  /** Resolved agent display name; omit when ticket has no assignee. */
  assigneeName?: string;
}

export function TicketCard({
  ticket,
  hierarchy,
  onOpen,
  isLive = false,
  assigneeName,
}: TicketCardProps) {
  const { attributes, listeners, setNodeRef, transform, isDragging } =
    useDraggable({
      id: ticket.id,
      data: { ticket, type: 'ticket' },
    });

  const style = transform
    ? { transform: CSS.Translate.toString(transform) }
    : undefined;

  return (
    <div
      ref={setNodeRef}
      style={style}
      {...listeners}
      {...attributes}
      role="button"
      tabIndex={0}
      onClick={() => {
        if (!isDragging) onOpen(ticket.id);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onOpen(ticket.id);
        }
      }}
      className={[
        'cursor-grab rounded-md border border-border bg-surface-raised p-3 shadow-card transition-shadow duration-fast',
        'hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
        isDragging ? 'z-10 opacity-60 shadow-lg' : '',
      ].join(' ')}
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
        <span>{ticket.title}</span>
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
  );
}

/** Resolve assignee label for a card; returns undefined when unassigned. */
export function resolveAssigneeName(
  assigneeAgentId: string | undefined,
  agentsById: Map<string, string>,
): string | undefined {
  if (!assigneeAgentId) return undefined;
  return agentsById.get(assigneeAgentId) ?? 'Unknown agent';
}
