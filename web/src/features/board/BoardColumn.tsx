import { useDroppable } from '@dnd-kit/core';
import { useState, type FormEvent } from 'react';
import {
  columnAccentClass,
  columnBgClass,
  columnBorderClass,
  type BoardColumnDef,
} from './columns';
import { TicketCard } from './TicketCard';
import type { Ticket } from './useTickets';

interface BoardColumnProps {
  column: BoardColumnDef;
  tickets: Ticket[];
  showQuickAdd?: boolean;
  onQuickAdd?: (title: string) => Promise<void>;
  isAdding?: boolean;
  onOpenTicket: (ticketId: string) => void;
}

export function BoardColumn({
  column,
  tickets,
  showQuickAdd = false,
  onQuickAdd,
  isAdding = false,
  onOpenTicket,
}: BoardColumnProps) {
  const [title, setTitle] = useState('');
  const { setNodeRef, isOver } = useDroppable({
    id: column.status,
    data: { status: column.status, type: 'column' },
  });

  async function handleQuickAdd(e: FormEvent) {
    e.preventDefault();
    const trimmed = title.trim();
    if (!trimmed || !onQuickAdd) return;
    await onQuickAdd(trimmed);
    setTitle('');
  }

  return (
    <section
      ref={setNodeRef}
      className={[
        'flex w-72 shrink-0 flex-col rounded-lg border',
        columnBgClass(column.colorKey),
        columnBorderClass(column.colorKey),
        isOver ? 'ring-2 ring-accent ring-offset-2 ring-offset-background' : '',
      ].join(' ')}
      aria-label={column.label}
    >
      <header
        className={[
          'flex items-center justify-between gap-2 border-b px-3 py-2.5',
          columnBorderClass(column.colorKey),
        ].join(' ')}
      >
        <h2
          className={[
            'font-display text-sm font-semibold',
            columnAccentClass(column.colorKey),
          ].join(' ')}
        >
          {column.label}
        </h2>
        <span
          className={[
            'rounded-full px-2 py-0.5 font-mono text-xs',
            columnBgClass(column.colorKey),
            columnAccentClass(column.colorKey),
          ].join(' ')}
        >
          {tickets.length}
        </span>
      </header>

      <div className="flex min-h-[12rem] flex-1 flex-col gap-2 p-2">
        {showQuickAdd && onQuickAdd && (
          <form
            onSubmit={(e) => void handleQuickAdd(e)}
            className="mb-1 border-b border-border/60 pb-2"
          >
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Add ticket…"
              disabled={isAdding}
              className="w-full rounded-md border border-border bg-surface-raised px-2 py-1.5 font-body text-sm text-text-primary outline-none transition-colors duration-fast placeholder:text-text-muted focus:border-accent focus:ring-1 focus:ring-accent-muted disabled:opacity-60"
            />
          </form>
        )}

        {tickets.map((ticket) => (
          <TicketCard key={ticket.id} ticket={ticket} onOpen={onOpenTicket} />
        ))}
      </div>
    </section>
  );
}
