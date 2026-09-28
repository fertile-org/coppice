import { isTicketStatus, type TicketStatus } from './columns';
import type { Ticket } from './useTickets';

export type TicketVisibility = 'active' | 'include_archived' | 'archived';

export type ActivityPreset = 'any' | '7d' | '30d' | '90d' | 'custom';

export interface BoardFilters {
  q: string;
  visibility: TicketVisibility;
  statuses: TicketStatus[];
  activityPreset: ActivityPreset;
  /** YYYY-MM-DD when activityPreset is custom */
  activitySince: string | null;
  /** YYYY-MM-DD when activityPreset is custom */
  activityUntil: string | null;
}

export const DEFAULT_BOARD_FILTERS: BoardFilters = {
  q: '',
  visibility: 'active',
  statuses: [],
  activityPreset: 'any',
  activitySince: null,
  activityUntil: null,
};

const VISIBILITIES = new Set<string>([
  'active',
  'include_archived',
  'archived',
]);

const ACTIVITY_PRESETS = new Set<string>(['any', '7d', '30d', '90d', 'custom']);

function isVisibility(value: string): value is TicketVisibility {
  return VISIBILITIES.has(value);
}

function isActivityPreset(value: string): value is ActivityPreset {
  return ACTIVITY_PRESETS.has(value);
}

function isIsoDate(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value);
}

export function parseBoardFilters(
  params: URLSearchParams,
): BoardFilters {
  const q = params.get('q')?.trim() ?? '';

  const visibilityRaw = params.get('visibility') ?? 'active';
  const visibility = isVisibility(visibilityRaw) ? visibilityRaw : 'active';

  const statuses = (params.get('status') ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(isTicketStatus);

  const activityRaw = params.get('activity') ?? 'any';
  const activityPreset = isActivityPreset(activityRaw) ? activityRaw : 'any';

  const sinceRaw = params.get('since');
  const untilRaw = params.get('until');
  const activitySince =
    activityPreset === 'custom' && sinceRaw && isIsoDate(sinceRaw)
      ? sinceRaw
      : null;
  const activityUntil =
    activityPreset === 'custom' && untilRaw && isIsoDate(untilRaw)
      ? untilRaw
      : null;

  return {
    q,
    visibility,
    statuses,
    activityPreset,
    activitySince,
    activityUntil,
  };
}

/** Writes filter params into `params`, preserving unrelated keys (e.g. ticket). */
export function writeBoardFilters(
  params: URLSearchParams,
  filters: BoardFilters,
): URLSearchParams {
  const next = new URLSearchParams(params);

  if (filters.q.trim()) next.set('q', filters.q.trim());
  else next.delete('q');

  if (filters.visibility !== 'active') {
    next.set('visibility', filters.visibility);
  } else {
    next.delete('visibility');
  }

  if (filters.statuses.length > 0) {
    next.set('status', filters.statuses.join(','));
  } else {
    next.delete('status');
  }

  if (filters.activityPreset !== 'any') {
    next.set('activity', filters.activityPreset);
  } else {
    next.delete('activity');
  }

  if (filters.activityPreset === 'custom' && filters.activitySince) {
    next.set('since', filters.activitySince);
  } else {
    next.delete('since');
  }

  if (filters.activityPreset === 'custom' && filters.activityUntil) {
    next.set('until', filters.activityUntil);
  } else {
    next.delete('until');
  }

  return next;
}

export function countActiveBoardFilters(filters: BoardFilters): number {
  let count = 0;
  if (filters.q.trim()) count += 1;
  if (filters.visibility !== 'active') count += 1;
  if (filters.statuses.length > 0) count += 1;
  if (filters.activityPreset === 'custom') {
    if (filters.activitySince || filters.activityUntil) count += 1;
  } else if (filters.activityPreset !== 'any') {
    count += 1;
  }
  return count;
}

export function boardFiltersAreDefault(filters: BoardFilters): boolean {
  return (
    filters.q.trim() === '' &&
    filters.visibility === 'active' &&
    filters.statuses.length === 0 &&
    filters.activityPreset === 'any' &&
    filters.activitySince == null &&
    filters.activityUntil == null
  );
}

export function includeArchivedForVisibility(
  visibility: TicketVisibility,
): boolean {
  return visibility !== 'active';
}

function daysAgo(now: Date, days: number): Date {
  return new Date(now.getTime() - days * 24 * 60 * 60 * 1000);
}

function startOfLocalDay(isoDate: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDate);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  return new Date(year, month - 1, day, 0, 0, 0, 0);
}

function endOfLocalDay(isoDate: string): Date | null {
  const start = startOfLocalDay(isoDate);
  if (!start) return null;
  return new Date(
    start.getFullYear(),
    start.getMonth(),
    start.getDate(),
    23,
    59,
    59,
    999,
  );
}

export function activityWindow(
  filters: BoardFilters,
  now: Date = new Date(),
): { since: Date | null; until: Date | null } {
  switch (filters.activityPreset) {
    case 'any':
      return { since: null, until: null };
    case '7d':
      return { since: daysAgo(now, 7), until: null };
    case '30d':
      return { since: daysAgo(now, 30), until: null };
    case '90d':
      return { since: daysAgo(now, 90), until: null };
    case 'custom':
      return {
        since: filters.activitySince
          ? startOfLocalDay(filters.activitySince)
          : null,
        until: filters.activityUntil
          ? endOfLocalDay(filters.activityUntil)
          : null,
      };
  }
}

export function filterTickets(
  tickets: Ticket[],
  filters: BoardFilters,
  now: Date = new Date(),
): Ticket[] {
  const q = filters.q.trim().toLowerCase();
  const statusSet =
    filters.statuses.length > 0 ? new Set(filters.statuses) : null;
  const { since, until } = activityWindow(filters, now);

  return tickets.filter((ticket) => {
    if (filters.visibility === 'archived' && !ticket.archivedAt) {
      return false;
    }
    if (filters.visibility === 'active' && ticket.archivedAt) {
      return false;
    }
    if (q && !ticket.title.toLowerCase().includes(q)) {
      return false;
    }
    if (statusSet && !statusSet.has(ticket.status)) {
      return false;
    }
    if (since || until) {
      const activity = Date.parse(ticket.lastActivityAt);
      if (Number.isNaN(activity)) return false;
      if (since && activity < since.getTime()) return false;
      if (until && activity > until.getTime()) return false;
    }
    return true;
  });
}
