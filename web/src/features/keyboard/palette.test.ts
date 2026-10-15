import { describe, expect, it } from 'vitest';
import { formatBinding } from './types';
import { omittedActionIds, shortcutAction } from './registry';
import {
  layoutPalette,
  listPaletteActions,
  PALETTE_RECENT_KEY,
  readRecentIds,
  rememberRecent,
  writeRecentIds,
} from './palette';

const COLUMN_LABELS = [
  'Go to Backlog column',
  'Go to Plan Review column',
  'Go to Ready column',
  'Go to In Progress column',
  'Go to In Review column',
  'Go to In QA column',
  'Go to Wait for Human Review column',
  'Go to Done column',
  'Go to Blocked column',
] as const;

const COLUMN_SHORTCUTS = [
  'G then 1',
  'G then 2',
  'G then 3',
  'G then 4',
  'G then 5',
  'G then 6',
  'G then 7',
  'G then 8',
  'G then 9',
] as const;

describe('command palette commands', () => {
  it('names each column jump from that column and leaves G then 1–9 on the shortcut', () => {
    const enabled = new Set(
      [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => `board.column.${n}`),
    );
    const columns = listPaletteActions({
      omitted: new Set(),
      scopes: ['global', 'board'],
      enabled,
      platform: 'linux',
    }).filter((entry) => entry.id.startsWith('board.column.'));

    expect(columns.map((entry) => entry.label)).toEqual([...COLUMN_LABELS]);
    expect(columns.map((entry) => entry.shortcut)).toEqual([...COLUMN_SHORTCUTS]);
    expect(columns).toHaveLength(9);
    expect(shortcutAction('board.column.2').label).toBe('Go to column 2');
    expect(formatBinding(shortcutAction('board.column.2').bindings[0], 'linux')).toBe(
      'G then 2',
    );
    expect(columns.some((entry) => entry.label.includes('Go to column'))).toBe(false);
  });

  it('draws Mod+K as ⌘ on macOS and Ctrl elsewhere', () => {
    const linux = listPaletteActions({
      omitted: new Set(),
      scopes: ['global'],
      enabled: new Set(['palette.open']),
      platform: 'linux',
    }).find((entry) => entry.id === 'palette.open');
    const macos = listPaletteActions({
      omitted: new Set(),
      scopes: ['global'],
      enabled: new Set(['palette.open']),
      platform: 'macos',
    }).find((entry) => entry.id === 'palette.open');

    expect(linux?.shortcut).toBe('Ctrl+K');
    expect(macos?.shortcut).toBe('⌘+K');
  });

  it('lists enabled commands in scope and page jumps, and omits disabled and admin rows', () => {
    const labels = listPaletteActions({
      omitted: omittedActionIds({ isAdmin: false, desktopMode: false }),
      scopes: ['global'],
      enabled: new Set(['palette.open', 'close']),
      platform: 'linux',
    }).map((entry) => entry.label);

    expect(labels).toEqual([
      'Open command palette',
      'Close',
      'Go to Boards',
      'Go to Agents',
      'Go to Chat',
      'Go to Knowledge',
      'Go to Repositories',
    ]);
  });

  it('hides Go to Users in the desktop app and shows it for an admin in the browser', () => {
    const desktop = listPaletteActions({
      omitted: omittedActionIds({ isAdmin: true, desktopMode: true }),
      scopes: ['global'],
      enabled: new Set(['close']),
      platform: 'linux',
    }).map((entry) => entry.label);
    const browser = listPaletteActions({
      omitted: omittedActionIds({ isAdmin: true, desktopMode: false }),
      scopes: ['global'],
      enabled: new Set(['close']),
      platform: 'linux',
    }).map((entry) => entry.label);

    expect(desktop).not.toContain('Go to Users');
    expect(desktop).toContain('Go to Settings');
    expect(browser).toContain('Go to Users');
    expect(browser).toContain('Go to Plugins');
  });

  it('includes ticket commands only in the scope where they can run', () => {
    const enabled = new Set(['ticket.open', 'ticket.assign', 'drawer.nextSection']);
    const onBoard = listPaletteActions({
      omitted: new Set(),
      scopes: ['global', 'board'],
      enabled,
      platform: 'linux',
    }).map((entry) => entry.id);
    const inDrawer = listPaletteActions({
      omitted: new Set(),
      scopes: ['global', 'ticket'],
      enabled,
      platform: 'linux',
    }).map((entry) => entry.id);

    expect(onBoard).toContain('ticket.open');
    expect(onBoard).toContain('ticket.assign');
    expect(onBoard).not.toContain('drawer.nextSection');
    expect(inDrawer).not.toContain('ticket.open');
    expect(inDrawer).toContain('ticket.assign');
    expect(inDrawer).toContain('drawer.nextSection');
    expect(onBoard).not.toContain('review.approvePlan');
    expect(onBoard).not.toContain('help.show');
  });
});

describe('command palette search', () => {
  const actions = listPaletteActions({
    omitted: new Set(),
    scopes: ['global', 'board'],
    enabled: new Set([
      'palette.open',
      'close',
      ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => `board.column.${n}`),
      'board.nextColumn',
    ]),
    platform: 'linux',
  });

  it('shows recent commands first when the query is empty and caps them at eight', () => {
    let ids: string[] = [];
    for (const id of [
      'board.column.1',
      'board.column.2',
      'board.column.3',
      'board.column.4',
      'board.column.5',
      'board.column.6',
      'board.column.7',
      'board.column.8',
      'nav.boards',
    ]) {
      ids = rememberRecent(ids, id);
    }

    const layout = layoutPalette({
      actions,
      tickets: [{ id: 'tic-1', title: 'Plan the release' }],
      agents: [{ id: 'agt-1', name: 'Planner' }],
      recentIds: ids,
      query: '',
    });

    expect(layout.recent.map((entry) => entry.label)).toEqual([
      'Go to Boards',
      'Go to Done column',
      'Go to Wait for Human Review column',
      'Go to In QA column',
      'Go to In Review column',
      'Go to In Progress column',
      'Go to Ready column',
      'Go to Plan Review column',
    ]);
    expect(layout.recent).toHaveLength(8);
    const navigate = layout.groups.find((group) => group.group === 'Navigate');
    expect(navigate?.items.map((entry) => entry.id)).not.toContain('nav.boards');
    expect(navigate?.items.map((entry) => entry.id)).toContain('board.column.1');
    expect(layout.matches).toEqual([]);
    expect(layout.empty).toBe(false);
  });

  it('fuzzy-matches action labels, ticket titles, ticket ids, and agent names', () => {
    const byLabel = layoutPalette({
      actions,
      tickets: [
        { id: 'tic-1', title: 'Plan the release' },
        { id: 'plan-99', title: 'Unrelated work' },
        { id: 'tic-2', title: 'Write the docs' },
      ],
      agents: [
        { id: 'agt-1', name: 'Planner' },
        { id: 'agt-2', name: 'Reviewer' },
      ],
      recentIds: ['nav.boards'],
      query: 'plan',
    });

    expect(byLabel.recent).toEqual([]);
    expect(byLabel.groups.flatMap((group) => group.items.map((entry) => entry.label))).toEqual([
      'Go to Plan Review column',
    ]);
    expect(byLabel.matches.map((entry) => entry.label)).toEqual([
      'Plan the release',
      'Unrelated work',
      'Planner',
    ]);
    expect(byLabel.empty).toBe(false);

    const byId = layoutPalette({
      actions,
      tickets: [{ id: 'plan-99', title: 'Unrelated work' }],
      agents: [],
      recentIds: [],
      query: 'plan-99',
    });
    expect(byId.matches.map((entry) => entry.id)).toEqual(['ticket:plan-99']);

    const none = layoutPalette({
      actions,
      tickets: [{ id: 'tic-1', title: 'Plan the release' }],
      agents: [{ id: 'agt-1', name: 'Planner' }],
      recentIds: [],
      query: 'zzzz-nope',
    });
    expect(none.empty).toBe(true);
    expect(none.groups).toEqual([]);
    expect(none.matches).toEqual([]);
  });

  it('remembers recent ids in order and ignores a corrupt store', () => {
    const storage = new Map<string, string>();
    const memory = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
    };

    expect(readRecentIds(memory)).toEqual([]);
    writeRecentIds(memory, rememberRecent([], 'nav.chat'));
    expect(storage.get(PALETTE_RECENT_KEY)).toBe(JSON.stringify(['nav.chat']));
    expect(readRecentIds(memory)).toEqual(['nav.chat']);

    memory.setItem(PALETTE_RECENT_KEY, '{');
    expect(readRecentIds(memory)).toEqual([]);
  });
});
