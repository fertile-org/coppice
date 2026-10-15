import type { Binding, Chord, ShortcutAction } from './types';

/**
 * One registry for the palette, shortcuts, help, and later the app menu.
 * Labels and groups are the Content-locked names. Column jumps keep the
 * label "Go to column {n}" here and in the help overlay. The palette shows
 * "Go to {column} column".
 */
function chord(key: string, extras?: Omit<Chord, 'key'>): Binding {
  return { kind: 'chord', chord: { key, ...extras } };
}

function sequence(leader: string, second: string): Binding {
  return { kind: 'sequence', keys: [leader, second] };
}

const COLUMN_JUMPS: ShortcutAction[] = [1, 2, 3, 4, 5, 6, 7, 8, 9].map(
  (n) => ({
    id: `board.column.${n}`,
    label: `Go to column ${n}`,
    group: 'Navigate' as const,
    bindings: [sequence('g', String(n))],
    scopes: ['global' as const],
  }),
);

export const SHORTCUT_ACTIONS: readonly ShortcutAction[] = [
  {
    id: 'palette.open',
    label: 'Open command palette',
    group: 'Navigate',
    bindings: [chord('k', { mod: true })],
    scopes: ['global', 'text'],
  },
  {
    id: 'help.show',
    label: 'Show keyboard shortcuts',
    group: 'Navigate',
    bindings: [chord('?')],
    scopes: ['global'],
  },
  {
    id: 'close',
    label: 'Close',
    group: 'Navigate',
    bindings: [chord('escape')],
    scopes: ['global', 'text', 'terminal'],
  },
  {
    id: 'board.nextCard',
    label: 'Next card',
    group: 'Navigate',
    bindings: [chord('j'), chord('arrowdown')],
    scopes: ['board'],
  },
  {
    id: 'board.previousCard',
    label: 'Previous card',
    group: 'Navigate',
    bindings: [chord('k'), chord('arrowup')],
    scopes: ['board'],
  },
  {
    id: 'board.nextColumn',
    label: 'Next column',
    group: 'Navigate',
    bindings: [chord('l'), chord('arrowright')],
    scopes: ['board'],
  },
  {
    id: 'board.previousColumn',
    label: 'Previous column',
    group: 'Navigate',
    bindings: [chord('h'), chord('arrowleft')],
    scopes: ['board'],
  },
  ...COLUMN_JUMPS,
  {
    id: 'nav.boards',
    label: 'Go to Boards',
    group: 'Navigate',
    bindings: [sequence('g', 'b')],
    scopes: ['global'],
  },
  {
    id: 'nav.agents',
    label: 'Go to Agents',
    group: 'Navigate',
    bindings: [sequence('g', 'a')],
    scopes: ['global'],
  },
  {
    id: 'nav.chat',
    label: 'Go to Chat',
    group: 'Navigate',
    bindings: [sequence('g', 'c')],
    scopes: ['global'],
  },
  {
    id: 'nav.knowledge',
    label: 'Go to Knowledge',
    group: 'Navigate',
    bindings: [sequence('g', 'k')],
    scopes: ['global'],
  },
  {
    id: 'nav.repositories',
    label: 'Go to Repositories',
    group: 'Navigate',
    bindings: [sequence('g', 'r')],
    scopes: ['global'],
  },
  {
    id: 'nav.plugins',
    label: 'Go to Plugins',
    group: 'Navigate',
    bindings: [sequence('g', 'p')],
    scopes: ['global'],
    adminOnly: true,
  },
  {
    id: 'nav.users',
    label: 'Go to Users',
    group: 'Navigate',
    bindings: [sequence('g', 'u')],
    scopes: ['global'],
    adminOnly: true,
    desktopOmit: true,
  },
  {
    id: 'nav.tools',
    label: 'Go to Tools',
    group: 'Navigate',
    bindings: [sequence('g', 't')],
    scopes: ['global'],
    adminOnly: true,
  },
  {
    id: 'nav.settings',
    label: 'Go to Settings',
    group: 'Navigate',
    bindings: [sequence('g', 's')],
    scopes: ['global'],
    adminOnly: true,
  },
  {
    id: 'drawer.nextSection',
    label: 'Next section',
    group: 'Navigate',
    bindings: [chord(']')],
    scopes: ['ticket'],
  },
  {
    id: 'drawer.previousSection',
    label: 'Previous section',
    group: 'Navigate',
    bindings: [chord('[')],
    scopes: ['ticket'],
  },
  {
    id: 'ticket.open',
    label: 'Open ticket',
    group: 'Tickets',
    bindings: [chord('enter')],
    scopes: ['board'],
  },
  {
    id: 'ticket.create',
    label: 'Create ticket',
    group: 'Tickets',
    bindings: [chord('c')],
    scopes: ['board'],
  },
  {
    id: 'ticket.assign',
    label: 'Assign ticket',
    group: 'Tickets',
    bindings: [chord('a')],
    scopes: ['board', 'ticket'],
  },
  {
    id: 'ticket.moveNext',
    label: 'Move to next column',
    group: 'Tickets',
    bindings: [sequence('m', 'l')],
    scopes: ['board', 'ticket'],
  },
  {
    id: 'ticket.movePrevious',
    label: 'Move to previous column',
    group: 'Tickets',
    bindings: [sequence('m', 'h')],
    scopes: ['board', 'ticket'],
  },
  {
    id: 'ticket.comment',
    label: 'Post comment',
    group: 'Tickets',
    bindings: [chord('enter', { mod: true })],
    scopes: ['text'],
  },
  {
    id: 'review.approvePlan',
    label: 'Approve plan',
    group: 'Review',
    bindings: [sequence('r', 'p')],
    scopes: ['board', 'ticket'],
  },
  {
    id: 'review.accept',
    label: 'Accept…',
    group: 'Review',
    bindings: [chord('enter', { mod: true, shift: true })],
    scopes: ['board', 'ticket'],
  },
  {
    id: 'review.merge',
    label: 'Merge…',
    group: 'Review',
    bindings: [sequence('r', 'm')],
    scopes: ['board', 'ticket'],
  },
  {
    id: 'review.openCode',
    label: 'Open code review',
    group: 'Review',
    bindings: [chord('d')],
    scopes: ['board', 'ticket'],
  },
];

const ACTIONS_BY_ID = new Map(SHORTCUT_ACTIONS.map((action) => [action.id, action]));

export function shortcutAction(id: string): ShortcutAction {
  const action = ACTIONS_BY_ID.get(id);
  if (!action) throw new Error(`Unknown shortcut action: ${id}`);
  return action;
}

export function omittedActionIds(input: {
  isAdmin: boolean;
  desktopMode: boolean;
}): Set<string> {
  const ids = new Set<string>();
  for (const action of SHORTCUT_ACTIONS) {
    if (action.adminOnly && !input.isAdmin) ids.add(action.id);
    if (action.desktopOmit && input.desktopMode) ids.add(action.id);
  }
  return ids;
}

/** Leaders are `g`, `m`, and `r`. `p` is only a second key. */
export function sequenceLeaders(
  actions: readonly ShortcutAction[] = SHORTCUT_ACTIONS,
): Set<string> {
  const leaders = new Set<string>();
  for (const action of actions) {
    for (const binding of action.bindings) {
      if (binding.kind === 'sequence') leaders.add(binding.keys[0]);
    }
  }
  return leaders;
}
