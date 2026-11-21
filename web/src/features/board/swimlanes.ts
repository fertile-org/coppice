import { isTicketStatus, type TicketStatus } from './columns';
import type { SplitProgress, Ticket } from './useTickets';

export interface SplitLane {
  parentId: string;
  title: string;
  progress: SplitProgress;
  /** Parent ticket from the unfiltered board list; never a lane card. */
  parent: Ticket;
  /** Sub-tickets only. */
  tickets: Ticket[];
}

function familyFinished(progress: SplitProgress): boolean {
  return (
    progress.children.length > 0 &&
    progress.children.every((child) => child.archived || child.status === 'done')
  );
}

/**
 * Plan-review splits (`splitProgress`) share a lane while the parent is on the
 * board, not archived, and not Done. A missing, archived, or finished parent
 * sends the family back to the columns, and so does a family whose children are all
 * Done or archived (an archived child is hidden, so the lane would stay empty).
 * The parent lives in `lane.parent` (header), never in `lane.tickets`. A parent
 * with no visible children stays in `rest` as a normal card.
 * Other parent links stay in the columns.
 */
export function splitLanes(
  tickets: readonly Ticket[],
  boardTickets: readonly Ticket[] = tickets,
): {
  lanes: SplitLane[];
  rest: Ticket[];
} {
  const boardById = new Map(boardTickets.map((ticket) => [ticket.id, ticket]));
  const lanes: SplitLane[] = [];
  const byParent = new Map<string, SplitLane>();
  const deferredParents: Ticket[] = [];
  const rest: Ticket[] = [];

  for (const ticket of tickets) {
    const progress = ticket.splitProgress;
    const parent = progress ? boardById.get(progress.parentTicketId) : undefined;
    if (
      !progress ||
      !parent ||
      parent.archivedAt ||
      parent.status === 'done' ||
      familyFinished(progress)
    ) {
      rest.push(ticket);
      continue;
    }

    if (progress.position == null) {
      deferredParents.push(ticket);
      continue;
    }

    let lane = byParent.get(progress.parentTicketId);
    if (!lane) {
      const parentProgress = parent.splitProgress;
      lane = {
        parentId: progress.parentTicketId,
        title: parentProgress?.parentTitle ?? progress.parentTitle,
        progress: parentProgress ?? progress,
        parent,
        tickets: [],
      };
      byParent.set(progress.parentTicketId, lane);
      lanes.push(lane);
    }
    lane.tickets.push(ticket);
    lane.title = progress.parentTitle;
  }

  for (const ticket of deferredParents) {
    const lane = byParent.get(ticket.id);
    if (lane && ticket.splitProgress) {
      lane.progress = ticket.splitProgress;
      lane.title = ticket.splitProgress.parentTitle;
      continue;
    }
    rest.push(ticket);
  }

  return { lanes, rest };
}

export function ticketsInLaneColumn(lane: SplitLane, status: TicketStatus): Ticket[] {
  return lane.tickets
    .filter((ticket) => ticket.status === status)
    .sort(
      (left, right) =>
        (left.splitProgress?.position ?? 0) - (right.splitProgress?.position ?? 0),
    );
}

/** Lane cards precede ungrouped cards in the same status for J/K. */
export function columnTicketIds(
  lanes: readonly SplitLane[],
  collapsed: ReadonlySet<string>,
  status: TicketStatus,
  columnTickets: readonly Ticket[],
): string[] {
  const ids: string[] = [];
  for (const lane of lanes) {
    if (collapsed.has(lane.parentId)) continue;
    for (const ticket of ticketsInLaneColumn(lane, status)) ids.push(ticket.id);
  }
  for (const ticket of columnTickets) ids.push(ticket.id);
  return ids;
}

export function splitChildLabel(
  progress: SplitProgress | null | undefined,
): string | null {
  if (progress?.position == null) return null;
  return `↳ ${progress.position} of ${progress.count}`;
}

export function splitNeedsReview(progress: SplitProgress): boolean {
  return progress.nextActor === 'you';
}

export function laneDropId(parentId: string, status: TicketStatus): string {
  return `lane:${parentId}:${status}`;
}

const LANE_DROP_ID = /^lane:[^:]+:([a-z0-9_]+)$/;

export function statusFromLaneDropId(id: string): TicketStatus | null {
  const status = LANE_DROP_ID.exec(id)?.[1];
  if (!status || !isTicketStatus(status)) return null;
  return status;
}
