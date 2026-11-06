import { useDroppable } from '@dnd-kit/core';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { boardFocusDomId } from './boardFocus';
import { BOARD_COLUMNS, type TicketStatus } from './columns';
import { resolveAssigneeName, TicketCard } from './TicketCard';
import type { TicketHierarchyIndex } from './ticketHierarchy';
import type { Ticket } from './useTickets';
import {
  laneDropId,
  splitNeedsReview,
  ticketsInLaneColumn,
  type SplitLane,
} from './swimlanes';

interface SplitLaneRowProps {
  lane: SplitLane;
  collapsed: boolean;
  onToggle: (parentId: string) => void;
  hierarchyIndex: TicketHierarchyIndex;
  agentsById: Map<string, string>;
  onOpenTicket: (ticketId: string) => void;
  onRunPlan?: (ticketId: string) => void;
  planRunTicketId?: string | null;
  onAssignSuggested?: (ticketId: string, agentId: string) => void;
  assignSuggestedTicketId?: string | null;
  hasBoardFocus?: boolean;
  focusedTicketId?: string | null;
  onCardFocus?: (ticketId: string) => void;
}

export function SplitLaneRow({
  lane,
  collapsed,
  onToggle,
  hierarchyIndex,
  agentsById,
  onOpenTicket,
  onRunPlan,
  planRunTicketId = null,
  onAssignSuggested,
  assignSuggestedTicketId = null,
  hasBoardFocus = false,
  focusedTicketId = null,
  onCardFocus,
}: SplitLaneRowProps) {
  const needsReview = splitNeedsReview(lane.progress);

  return (
    <section aria-label={lane.title} className="flex flex-col gap-2">
      <header className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-border bg-surface-raised px-3 py-2">
        <button
          type="button"
          aria-expanded={!collapsed}
          onClick={() => onToggle(lane.parentId)}
          className="keyboard-focus flex min-w-0 items-center gap-2 rounded-sm font-display text-sm font-semibold text-text-primary"
        >
          {collapsed ? (
            <ChevronRight className="h-4 w-4 shrink-0" aria-hidden="true" />
          ) : (
            <ChevronDown className="h-4 w-4 shrink-0" aria-hidden="true" />
          )}
          <span className="sr-only">{collapsed ? 'Expand' : 'Collapse'}</span>
          <span className="truncate">{lane.title}</span>
        </button>
        <span className="shrink-0 font-body text-xs text-text-secondary">
          {lane.progress.mergedCount}/{lane.progress.count} merged
        </span>
        {needsReview && (
          <span className="shrink-0 rounded-full border border-warning-muted bg-warning-muted px-2 py-0.5 font-body text-xs font-medium text-warning">
            Needs review
          </span>
        )}
      </header>
      {!collapsed && (
        <div data-testid="swimlane-row" className="flex items-stretch gap-4">
          {BOARD_COLUMNS.map((column) => (
            <LaneCell
              key={column.status}
              parentId={lane.parentId}
              status={column.status}
              label={column.label}
              tickets={ticketsInLaneColumn(lane, column.status)}
              hierarchyIndex={hierarchyIndex}
              agentsById={agentsById}
              onOpenTicket={onOpenTicket}
              onRunPlan={onRunPlan}
              planRunTicketId={planRunTicketId}
              onAssignSuggested={onAssignSuggested}
              assignSuggestedTicketId={assignSuggestedTicketId}
              hasBoardFocus={hasBoardFocus}
              focusedTicketId={focusedTicketId}
              onCardFocus={onCardFocus}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function LaneCell({
  parentId,
  status,
  label,
  tickets,
  hierarchyIndex,
  agentsById,
  onOpenTicket,
  onRunPlan,
  planRunTicketId,
  onAssignSuggested,
  assignSuggestedTicketId,
  hasBoardFocus,
  focusedTicketId,
  onCardFocus,
}: {
  parentId: string;
  status: TicketStatus;
  label: string;
  tickets: Ticket[];
  hierarchyIndex: TicketHierarchyIndex;
  agentsById: Map<string, string>;
  onOpenTicket: (ticketId: string) => void;
  onRunPlan?: (ticketId: string) => void;
  planRunTicketId: string | null;
  onAssignSuggested?: (ticketId: string, agentId: string) => void;
  assignSuggestedTicketId: string | null;
  hasBoardFocus: boolean;
  focusedTicketId: string | null;
  onCardFocus?: (ticketId: string) => void;
}) {
  const { setNodeRef, isOver } = useDroppable({
    id: laneDropId(parentId, status),
    data: { status, type: 'column' },
  });

  return (
    <div
      ref={setNodeRef}
      data-testid={`swimlane-cell-${status}`}
      className={[
        'flex w-72 shrink-0 flex-col gap-2',
        tickets.length === 0 ? 'min-h-12' : '',
        isOver ? 'rounded-md ring-2 ring-accent ring-offset-2 ring-offset-background' : '',
      ].join(' ')}
    >
      {tickets.length > 0 && <span className="sr-only">{label}</span>}
      {tickets.map((ticket) => {
        const selected = focusedTicketId === ticket.id;
        return (
          <TicketCard
            key={ticket.id}
            ticket={ticket}
            hierarchy={hierarchyIndex.get(ticket.id)}
            onOpen={onOpenTicket}
            isLive={ticket.hasActiveRun ?? false}
            assigneeName={resolveAssigneeName(ticket.assigneeAgentId, agentsById)}
            onRunPlan={onRunPlan}
            planRunPending={planRunTicketId === ticket.id}
            onAssignSuggested={onAssignSuggested}
            assignSuggestedPending={assignSuggestedTicketId === ticket.id}
            tabIndex={hasBoardFocus ? (selected ? 0 : -1) : 0}
            boardFocusId={
              hasBoardFocus
                ? boardFocusDomId({ type: 'card', ticketId: ticket.id })
                : undefined
            }
            boardTabStop={selected}
            onFocusCard={onCardFocus}
          />
        );
      })}
    </div>
  );
}
