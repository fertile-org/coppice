import { useDroppable } from '@dnd-kit/core';
import { useVirtualizer } from '@tanstack/react-virtual';
import {
  Fragment,
  memo,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from 'react';
import {
  columnAccentClass,
  columnBgClass,
  columnBorderClass,
  type BoardColumnDef,
} from './columns';
import { boardFocusDomId } from './boardFocus';
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
  /** Board keyboard focus is active (roving tabindex on). */
  hasBoardFocus?: boolean;
  /** The focused card when it is in this column, else null. */
  focusedTicketId?: string | null;
  /** Board focus is on this column's heading. */
  headingFocused?: boolean;
  onCardFocus?: (ticketId: string) => void;
  onHeadingFocus?: (columnIndex: number) => void;
}

/**
 * Memoized with primitive focus props so a focus or URL change re-renders only
 * the columns it touches.
 */
export const BoardColumn = memo(function BoardColumn({
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
  hasBoardFocus = false,
  focusedTicketId = null,
  headingFocused = false,
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
  const headingSelected = headingFocusable && headingFocused;
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
              data-add-ticket=""
              data-column-index={columnIndex}
              className="field-control w-full px-2 py-1.5 font-body text-sm placeholder:text-text-muted"
            />
          </form>
        )}

        <ColumnCards
          tickets={tickets}
          focusedTicketId={focusedTicketId}
          renderCard={(ticket) => {
            const selected = focusedTicketId === ticket.id;
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
                tabIndex={hasBoardFocus ? (selected ? 0 : -1) : 0}
                boardFocusId={hasBoardFocus ? focusId : undefined}
                boardTabStop={selected}
                onFocusCard={onCardFocus}
              />
            );
          }}
        />
      </div>
    </section>
  );
});

/** Columns longer than this render only the cards in view. */
export const VIRTUALIZE_AFTER = 80;
const ESTIMATED_CARD_HEIGHT = 112;

/**
 * A short column renders every card. A long one scrolls on its own and
 * mounts only the cards near the viewport, keeping the keyboard-focused card
 * mounted by scrolling to it.
 */
function ColumnCards({
  tickets,
  focusedTicketId,
  renderCard,
}: {
  tickets: Ticket[];
  focusedTicketId: string | null;
  renderCard: (ticket: Ticket) => ReactNode;
}) {
  if (tickets.length <= VIRTUALIZE_AFTER) {
    return <>{tickets.map((ticket) => <Fragment key={ticket.id}>{renderCard(ticket)}</Fragment>)}</>;
  }
  return (
    <VirtualCards tickets={tickets} focusedTicketId={focusedTicketId} renderCard={renderCard} />
  );
}

function VirtualCards({
  tickets,
  focusedTicketId,
  renderCard,
}: {
  tickets: Ticket[];
  focusedTicketId: string | null;
  renderCard: (ticket: Ticket) => ReactNode;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: tickets.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ESTIMATED_CARD_HEIGHT,
    measureElement: (element) => {
      const measured = element.getBoundingClientRect().height;
      return measured > 0 ? measured : ESTIMATED_CARD_HEIGHT;
    },
    overscan: 6,
    initialRect: { width: 280, height: 800 },
    getItemKey: (index) => tickets[index]?.id ?? index,
  });

  const focusedIndex = focusedTicketId
    ? tickets.findIndex((ticket) => ticket.id === focusedTicketId)
    : -1;
  useEffect(() => {
    if (focusedIndex >= 0) virtualizer.scrollToIndex(focusedIndex, { align: 'auto' });
  }, [focusedIndex, virtualizer]);

  return (
    <div
      ref={parentRef}
      data-virtual-column=""
      className="max-h-[calc(100svh-16rem)] overflow-y-auto"
    >
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((item) => {
          const ticket = tickets[item.index];
          if (!ticket) return null;
          return (
            <div
              key={item.key}
              data-index={item.index}
              ref={virtualizer.measureElement}
              className="absolute left-0 top-0 w-full pb-2"
              style={{ transform: `translateY(${item.start}px)` }}
            >
              {renderCard(ticket)}
            </div>
          );
        })}
      </div>
    </div>
  );
}
