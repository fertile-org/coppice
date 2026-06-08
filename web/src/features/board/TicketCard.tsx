import { useDraggable } from '@dnd-kit/core';
import { CSS } from '@dnd-kit/utilities';
import type { Ticket } from './useTickets';

interface TicketCardProps {
  ticket: Ticket;
  onOpen: (ticketId: string) => void;
  isLive?: boolean;
}

export function TicketCard({ ticket, onOpen, isLive = false }: TicketCardProps) {
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
        'hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted',
        isDragging ? 'z-10 opacity-60 shadow-lg' : '',
      ].join(' ')}
    >
      <p className="flex items-start gap-1.5 font-body text-sm font-medium leading-snug text-text-primary">
        {isLive && (
          <span
            className="mt-1.5 inline-block h-2 w-2 shrink-0 animate-pulse rounded-full bg-accent"
            aria-label="Agent running"
          />
        )}
        <span>{ticket.title}</span>
      </p>

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
            <span className="rounded-full border border-border bg-paper-200 px-2 py-0.5 font-body text-xs capitalize text-text-secondary">
              {ticket.priority}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
