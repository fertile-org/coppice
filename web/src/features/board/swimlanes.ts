import { isTicketStatus, type TicketStatus } from './columns';
import type { SplitProgress, Ticket } from './useTickets';

export interface SplitLane {
  parentId: string;
  title: string;
  progress: SplitProgress;
  tickets: Ticket[];
}

/**
 * Plan-review splits (`splitProgress`) share a lane.
 * Other parent links stay in the columns.
 */
export function splitLanes(tickets: readonly Ticket[]): {
  lanes: SplitLane[];
  rest: Ticket[];
} {
  const lanes: SplitLane[] = [];
  const byParent = new Map<string, SplitLane>();
  const rest: Ticket[] = [];

  for (const ticket of tickets) {
    const progress = ticket.splitProgress;
    if (!progress) {
      rest.push(ticket);
      continue;
    }
    let lane = byParent.get(progress.parentTicketId);
    if (!lane) {
      lane = {
        parentId: progress.parentTicketId,
        title: progress.parentTitle,
        progress,
        tickets: [],
      };
      byParent.set(progress.parentTicketId, lane);
      lanes.push(lane);
    }
    lane.tickets.push(ticket);
    lane.title = progress.parentTitle;
    if (progress.position == null) lane.progress = progress;
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

/** Lane cards sit above the column stack, so they come first for J/K. */
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
  return `↳ ${progress.position}/${progress.count} · ${progress.parentTitle}`;
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
