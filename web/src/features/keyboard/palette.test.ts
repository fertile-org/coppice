import { describe, expect, it } from 'vitest';
import { omittedActionIds } from './registry';
import {
  layoutPalette,
  listPaletteActions,
  PALETTE_HELP_LABEL,
  PALETTE_RECENT_KEY,
  readRecentIds,
  rememberRecent,
  writeRecentIds,
} from './palette';

const PAGE_LABELS = [
  'Go to Boards',
  'Go to Chat',
  'Go to Agents',
  'Go to Knowledge',
  'Go to Repositories',
  'Go to Plugins',
  'Go to Users',
  'Go to System',
  'Go to Settings',
  PALETTE_HELP_LABEL,
] as const;

function pageActions() {
  return listPaletteActions({
    omitted: new Set(),
    platform: 'linux',
  });
}

describe('command palette commands', () => {
  it('lists only page jumps and Keyboard shortcuts', () => {
    const entries = pageActions();

    expect(entries.map((entry) => entry.label)).toEqual([...PAGE_LABELS]);
    expect(entries.find((entry) => entry.id === 'help.show')?.shortcut).toBe('?');
    expect(entries.find((entry) => entry.id === 'nav.boards')?.shortcut).toBe('G then B');
    expect(entries.some((entry) => entry.id === 'palette.open')).toBe(false);
    expect(entries.some((entry) => entry.id === 'close')).toBe(false);
    expect(entries.some((entry) => entry.id === 'board.nextCard')).toBe(false);
    expect(entries.some((entry) => entry.id.startsWith('board.column.'))).toBe(false);
    expect(entries.some((entry) => entry.id === 'ticket.create')).toBe(false);
  });

  it('hides Go to Users in the desktop app and shows it for an admin in the browser', () => {
    const desktop = listPaletteActions({
      omitted: omittedActionIds({ isAdmin: true, desktopMode: true }),
      platform: 'linux',
    }).map((entry) => entry.label);
    const browser = listPaletteActions({
      omitted: omittedActionIds({ isAdmin: true, desktopMode: false }),
      platform: 'linux',
    }).map((entry) => entry.label);

    expect(desktop).not.toContain('Go to Users');
    expect(desktop).toContain('Go to Settings');
    expect(desktop).toContain(PALETTE_HELP_LABEL);
    expect(browser).toContain('Go to Users');
    expect(browser).toContain('Go to Plugins');
  });
});

describe('command palette search', () => {
  const actions = pageActions();

  it('shows recent page jumps first when the query is empty and caps them at eight', () => {
    let ids: string[] = [];
    for (const id of [
      'nav.boards',
      'nav.agents',
      'nav.chat',
      'nav.knowledge',
      'nav.repositories',
      'nav.plugins',
      'nav.users',
      'nav.tools',
      'nav.settings',
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

    expect(layout.recent).toHaveLength(8);
    expect(layout.recent[0]?.id).toBe('nav.settings');
    const pages = layout.groups.find((group) => group.group === 'Navigate');
    expect(pages?.items.map((entry) => entry.id)).not.toContain('nav.settings');
    expect(pages?.items.map((entry) => entry.id)).toContain('nav.boards');
    expect(pages?.items.map((entry) => entry.id)).toContain('help.show');
    expect(layout.matches).toEqual([]);
    expect(layout.empty).toBe(false);
  });

  it('fuzzy-matches page labels, ticket titles, ticket ids, and agent names', () => {
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
    expect(byLabel.groups.flatMap((group) => group.items.map((entry) => entry.label))).toEqual([]);
    expect(byLabel.matches.map((entry) => entry.label)).toEqual([
      'Plan the release',
      'Unrelated work',
      'Planner',
    ]);
    expect(byLabel.empty).toBe(false);

    const pages = layoutPalette({
      actions,
      tickets: [],
      agents: [],
      recentIds: [],
      query: 'go to boards',
    });
    expect(pages.groups.flatMap((group) => group.items.map((entry) => entry.id))).toEqual([
      'nav.boards',
    ]);

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
