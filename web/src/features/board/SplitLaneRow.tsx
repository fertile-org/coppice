import { useDroppable } from '@dnd-kit/core';
import { Bot, ChevronDown, ChevronRight } from 'lucide-react';
import type { CSSProperties } from 'react';
import { TicketStatusBadge } from '../tickets/TicketStatusBadge';
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

const PRIORITY_LEVELS = new Set(['low', 'medium', 'high', 'critical']);

function priorityBadgeStyle(priority: string): CSSProperties | undefined {
  if (!PRIORITY_LEVELS.has(priority)) return undefined;
  return {
    backgroundColor: `var(--badge-priority-${priority}-bg)`,
    color: `var(--badge-priority-${priority}-text)`,
    borderColor: `var(--badge-priority-${priority}-border)`,
  };
}

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
  const parent = lane.parent;
  const assigneeName = resolveAssigneeName(parent.assigneeAgentId, agentsById);

  return (
    <section
      aria-label={lane.title}
      className="col-span-full grid grid-cols-subgrid gap-y-2"
    >
      <header className="col-span-full flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-border bg-surface-raised px-3 py-2">
        <button
          type="button"
          aria-expanded={!collapsed}
          aria-label={collapsed ? `Expand ${lane.title}` : `Collapse ${lane.title}`}
          onClick={() => onToggle(lane.parentId)}
          className="keyboard-focus flex shrink-0 items-center rounded-sm text-text-primary"
        >
          {collapsed ? (
            <ChevronRight className="h-4 w-4" aria-hidden="true" />
          ) : (
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          )}
        </button>
        <span className="shrink-0 font-body text-[0.65rem] font-medium uppercase tracking-wide text-text-muted">
          Parent
        </span>
        <span
          className="min-w-0 truncate font-display text-sm font-semibold text-text-primary"
          title={lane.title}
        >
          {lane.title}
        </span>
        <TicketStatusBadge status={parent.status} className="shrink-0" />
        {assigneeName && (
          <span className="flex min-w-0 items-center gap-1.5 font-body text-xs text-text-secondary">
            <Bot className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
            <span className="min-w-0 truncate" title={assigneeName}>
              {assigneeName}
            </span>
          </span>
        )}
        {parent.priority && (
          <span
            className={[
              'shrink-0 rounded-full border px-2 py-0.5 font-body text-xs capitalize',
              PRIORITY_LEVELS.has(parent.priority)
                ? ''
                : 'border-border bg-paper-200 text-text-secondary',
            ].join(' ')}
            style={priorityBadgeStyle(parent.priority)}
            data-priority={parent.priority}
          >
            {parent.priority}
          </span>
        )}
        <span className="shrink-0 font-body text-xs text-text-secondary">
          {lane.progress.mergedCount}/{lane.progress.count} merged
        </span>
        {needsReview && (
          <span className="shrink-0 rounded-full border border-warning-muted bg-warning-muted px-2 py-0.5 font-body text-xs font-medium text-warning">
            Needs review
          </span>
        )}
        <button
          type="button"
          onClick={() => onOpenTicket(lane.parentId)}
          className="keyboard-focus ml-auto shrink-0 rounded-sm font-body text-xs font-medium text-accent hover:text-accent-hover"
        >
          Open parent →
        </button>
      </header>
      {!collapsed && (
        <div data-testid="swimlane-row" className="contents">
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
        'flex min-w-0 flex-col gap-2',
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
