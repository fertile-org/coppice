import { useDroppable } from '@dnd-kit/core';
import { useState, type FormEvent } from 'react';
import {
  columnAccentClass,
  columnBgClass,
  columnBorderClass,
  type BoardColumnDef,
} from './columns';
import { boardFocusDomId, type BoardFocus } from './boardFocus';
import { resolveAssigneeName, TicketCard } from './TicketCard';
import type { TicketHierarchyIndex } from './ticketHierarchy';
import type { Ticket } from './useTickets';

interface BoardColumnProps {
  column: BoardColumnDef;
  tickets: Ticket[];
  hierarchyIndex: TicketHierarchyIndex;
  agentsById: Map<string, string>;
  showQuickAdd?: boolean;
  onQuickAdd?: (title: string) => Promise<void>;
  isAdding?: boolean;
  onOpenTicket: (ticketId: string) => void;
  onRunPlan?: (ticketId: string) => void;
  planRunTicketId?: string | null;
  columnIndex?: number;
  boardFocus?: BoardFocus;
  onCardFocus?: (ticketId: string) => void;
  onHeadingFocus?: (columnIndex: number) => void;
}

export function BoardColumn({
  column,
  tickets,
  hierarchyIndex,
  agentsById,
  showQuickAdd = false,
  onQuickAdd,
  isAdding = false,
  onOpenTicket,
  onRunPlan,
  planRunTicketId = null,
  columnIndex = 0,
  boardFocus,
  onCardFocus,
  onHeadingFocus,
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

  const headingFocusable = tickets.length === 0;
  const headingSelected =
    headingFocusable &&
    boardFocus?.type === 'heading' &&
    boardFocus.column === columnIndex;
  const headingFocusId = `heading:${columnIndex}`;

  return (
    <section
      ref={setNodeRef}
      className={[
        'flex min-h-[calc(100svh-14rem)] w-72 shrink-0 flex-col rounded-lg border',
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
          tabIndex={headingFocusable ? (headingSelected ? 0 : -1) : undefined}
          data-board-focus-id={headingFocusable ? headingFocusId : undefined}
          data-board-tabstop={headingSelected ? 'true' : undefined}
          onFocus={() => onHeadingFocus?.(columnIndex)}
          className={[
            'keyboard-focus rounded-sm font-display text-sm font-semibold',
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

      <div className="flex flex-1 flex-col gap-2 p-2">
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
              className="field-control w-full px-2 py-1.5 font-body text-sm placeholder:text-text-muted"
            />
          </form>
        )}

        {tickets.map((ticket) => {
          const selected =
            boardFocus?.type === 'card' && boardFocus.ticketId === ticket.id;
          const focusId = boardFocusDomId({ type: 'card', ticketId: ticket.id });
          return (
            <TicketCard
              key={ticket.id}
              ticket={ticket}
              hierarchy={hierarchyIndex.get(ticket.id)}
              onOpen={onOpenTicket}
              isLive={ticket.hasActiveRun ?? false}
              assigneeName={resolveAssigneeName(
                ticket.assigneeAgentId,
                agentsById,
              )}
              onRunPlan={onRunPlan}
              planRunPending={planRunTicketId === ticket.id}
              tabIndex={boardFocus ? (selected ? 0 : -1) : 0}
              boardFocusId={boardFocus ? focusId : undefined}
              boardTabStop={Boolean(selected)}
              onFocusCard={() => onCardFocus?.(ticket.id)}
            />
          );
        })}
      </div>
    </section>
  );
}
