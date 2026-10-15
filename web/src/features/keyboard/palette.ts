import { BOARD_COLUMNS } from '../board/columns';
import { activeScopes } from './match';
import { SHORTCUT_ACTIONS } from './registry';
import {
  formatBinding,
  SHORTCUT_GROUP_ORDER,
  type ShortcutAction,
  type ShortcutGroup,
  type ShortcutScope,
} from './types';
import type { Platform } from './platform';

export const PALETTE_RECENT_LIMIT = 8;
export const PALETTE_RECENT_KEY = 'coppice.palette.recent';

export const PALETTE_COPY = {
  title: 'Command palette',
  placeholder: 'Search actions, tickets, and agents…',
  recent: 'Recent',
  empty: 'No matches',
} as const;

/** Page jumps the palette and the G-then shortcuts share. */
export const PAGE_ACTION_ROUTES: Readonly<Record<string, string>> = {
  'nav.boards': '/boards',
  'nav.agents': '/agents',
  'nav.chat': '/chat',
  'nav.knowledge': '/knowledge',
  'nav.repositories': '/settings/repositories',
  'nav.plugins': '/settings/plugins',
  'nav.users': '/settings/users',
  'nav.tools': '/tools',
  'nav.settings': '/settings',
};

export type PaletteEntry = {
  id: string;
  label: string;
  group: ShortcutGroup;
  shortcut: string | null;
  keywords: string;
  kind: 'action' | 'ticket' | 'agent';
};

export type PaletteTicket = { id: string; title: string; boardId?: string };

/** Scopes for the page underneath the palette, ignoring the palette field itself. */
export function paletteScopes(input: {
  pathname: string;
  drawerOpen: boolean;
}): ShortcutScope[] {
  return activeScopes({
    pathname: input.pathname,
    drawerOpen: input.drawerOpen,
    modalOpen: false,
    targetKind: 'plain',
    foreignDialog: false,
  });
}

export function boardIdFromPath(pathname: string): string | undefined {
  const match = /^\/boards\/([^/]+)\/?$/.exec(pathname);
  return match?.[1];
}
export type PaletteAgent = { id: string; name: string };

export type PaletteLayout = {
  recent: PaletteEntry[];
  groups: { group: ShortcutGroup; items: PaletteEntry[] }[];
  matches: PaletteEntry[];
  empty: boolean;
};

export function columnJumpPaletteLabel(columnName: string): string {
  return `Go to ${columnName} column`;
}

export function paletteActionLabel(
  action: Pick<ShortcutAction, 'id' | 'label'>,
  columns: readonly { label: string }[] = BOARD_COLUMNS,
): string {
  const match = /^board\.column\.(\d+)$/.exec(action.id);
  if (!match) return action.label;
  const column = columns[Number(match[1]) - 1];
  return column ? columnJumpPaletteLabel(column.label) : action.label;
}

export function paletteShortcut(action: ShortcutAction, platform: Platform): string {
  return action.bindings.map((binding) => formatBinding(binding, platform)).join(' or ');
}

function inScope(action: ShortcutAction, scopes: readonly ShortcutScope[]): boolean {
  return action.scopes.some((scope) => scopes.includes(scope));
}

export function listPaletteActions(input: {
  omitted: ReadonlySet<string>;
  scopes: readonly ShortcutScope[];
  enabled: ReadonlySet<string>;
  platform: Platform;
  actions?: readonly ShortcutAction[];
  columns?: readonly { label: string }[];
}): PaletteEntry[] {
  const actions = input.actions ?? SHORTCUT_ACTIONS;
  const columns = input.columns ?? BOARD_COLUMNS;
  const entries: PaletteEntry[] = [];
  for (const action of actions) {
    if (input.omitted.has(action.id)) continue;
    if (!inScope(action, input.scopes)) continue;
    const pageJump = Object.prototype.hasOwnProperty.call(PAGE_ACTION_ROUTES, action.id);
    if (!input.enabled.has(action.id) && !pageJump) continue;
    const label = paletteActionLabel(action, columns);
    const shortcut = action.bindings.length ? paletteShortcut(action, input.platform) : null;
    entries.push({
      id: action.id,
      label,
      group: action.group,
      shortcut,
      keywords: action.label === label ? '' : action.label,
      kind: 'action',
    });
  }
  return entries;
}

export function fuzzyIncludes(query: string, text: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  const haystack = text.toLowerCase();
  if (haystack.includes(needle)) return true;
  let index = 0;
  for (const char of haystack) {
    if (char === needle[index]) {
      index += 1;
      if (index === needle.length) return true;
    }
  }
  return false;
}

function ticketEntries(tickets: readonly PaletteTicket[]): PaletteEntry[] {
  return tickets.map((ticket) => ({
    id: `ticket:${ticket.id}`,
    label: ticket.title,
    group: 'Tickets',
    shortcut: null,
    keywords: `${ticket.title} ${ticket.id}`,
    kind: 'ticket',
  }));
}

function agentEntries(agents: readonly PaletteAgent[]): PaletteEntry[] {
  return agents.map((agent) => ({
    id: `agent:${agent.id}`,
    label: agent.name,
    group: 'Navigate',
    shortcut: null,
    keywords: agent.name,
    kind: 'agent',
  }));
}

function matchesQuery(entry: PaletteEntry, query: string): boolean {
  if (fuzzyIncludes(query, entry.label)) return true;
  if (entry.keywords && fuzzyIncludes(query, entry.keywords)) return true;
  if (entry.shortcut && fuzzyIncludes(query, entry.shortcut)) return true;
  return false;
}

export function layoutPalette(input: {
  actions: readonly PaletteEntry[];
  tickets: readonly PaletteTicket[];
  agents: readonly PaletteAgent[];
  recentIds: readonly string[];
  query: string;
}): PaletteLayout {
  const tickets = ticketEntries(input.tickets);
  const agents = agentEntries(input.agents);
  const catalog = new Map<string, PaletteEntry>();
  for (const entry of [...input.actions, ...tickets, ...agents]) {
    catalog.set(entry.id, entry);
  }

  const query = input.query.trim();
  if (!query) {
    const recent: PaletteEntry[] = [];
    for (const id of input.recentIds) {
      const entry = catalog.get(id);
      if (!entry || recent.some((item) => item.id === id)) continue;
      recent.push(entry);
      if (recent.length === PALETTE_RECENT_LIMIT) break;
    }
    const recentIds = new Set(recent.map((entry) => entry.id));
    const groups = SHORTCUT_GROUP_ORDER.map((group) => ({
      group,
      items: input.actions.filter(
        (entry) => entry.group === group && !recentIds.has(entry.id),
      ),
    })).filter((group) => group.items.length > 0);
    return {
      recent,
      groups,
      matches: [],
      empty: recent.length === 0 && groups.length === 0,
    };
  }

  const groups = SHORTCUT_GROUP_ORDER.map((group) => ({
    group,
    items: input.actions.filter(
      (entry) => entry.group === group && matchesQuery(entry, query),
    ),
  })).filter((group) => group.items.length > 0);
  const matches = [...tickets, ...agents].filter((entry) => matchesQuery(entry, query));
  return {
    recent: [],
    groups,
    matches,
    empty: groups.length === 0 && matches.length === 0,
  };
}

export function rememberRecent(ids: readonly string[], id: string): string[] {
  return [id, ...ids.filter((existing) => existing !== id)].slice(0, PALETTE_RECENT_LIMIT);
}

export function readRecentIds(storage: Pick<Storage, 'getItem'>): string[] {
  try {
    const raw = storage.getItem(PALETTE_RECENT_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((item): item is string => typeof item === 'string')
      .slice(0, PALETTE_RECENT_LIMIT);
  } catch {
    return [];
  }
}

export function writeRecentIds(
  storage: Pick<Storage, 'setItem'>,
  ids: readonly string[],
): void {
  try {
    storage.setItem(PALETTE_RECENT_KEY, JSON.stringify(ids.slice(0, PALETTE_RECENT_LIMIT)));
  } catch {
    /* ignore quota / private mode */
  }
}
