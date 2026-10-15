import { describe, expect, it } from 'vitest';
import { detectPlatform, modPressed, type Platform } from './platform';
import {
  omittedActionIds,
  sequenceLeaders,
  SHORTCUT_ACTIONS,
  shortcutAction,
} from './registry';
import { isReservedChord, RESERVED_RULES } from './reserved';
import {
  formatBinding,
  SHORTCUT_GROUP_ORDER,
  type Chord,
  type KeyEventLike,
  type ShortcutGroup,
} from './types';

const NAVIGATE_LABELS = [
  'Open command palette',
  'Show keyboard shortcuts',
  'Close',
  'Next card',
  'Previous card',
  'Next column',
  'Previous column',
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => `Go to column ${n}`),
  'Go to Boards',
  'Go to Agents',
  'Go to Chat',
  'Go to Knowledge',
  'Go to Repositories',
  'Go to Plugins',
  'Go to Users',
  'Go to Tools',
  'Go to Settings',
  'Next section',
  'Previous section',
];

const TICKET_LABELS = [
  'Open ticket',
  'Create ticket',
  'Assign ticket',
  'Move to next column',
  'Move to previous column',
  'Post comment',
];

const REVIEW_LABELS = ['Approve plan', 'Accept…', 'Merge…', 'Open code review'];

function eventForChord(chord: Chord, platform: Platform): KeyEventLike {
  const key =
    chord.key === 'arrowdown'
      ? 'ArrowDown'
      : chord.key === 'arrowup'
        ? 'ArrowUp'
        : chord.key === 'arrowleft'
          ? 'ArrowLeft'
          : chord.key === 'arrowright'
            ? 'ArrowRight'
            : chord.key === 'escape'
              ? 'Escape'
              : chord.key === 'enter'
                ? 'Enter'
                : chord.key === 'space'
                  ? ' '
                  : chord.key === 'tab'
                    ? 'Tab'
                    : chord.key;
  return {
    key,
    metaKey: Boolean(chord.mod) && platform === 'macos',
    ctrlKey: Boolean(chord.mod) && platform !== 'macos',
    altKey: Boolean(chord.alt),
    shiftKey: Boolean(chord.shift) || chord.key === '?',
  };
}

describe('shortcut registry', () => {
  it('locks Content names into Navigate, Tickets, and Review', () => {
    expect(SHORTCUT_GROUP_ORDER).toEqual(['Navigate', 'Tickets', 'Review']);
    const labels = {
      Navigate: NAVIGATE_LABELS,
      Tickets: TICKET_LABELS,
      Review: REVIEW_LABELS,
    } satisfies Record<ShortcutGroup, readonly string[]>;

    for (const group of SHORTCUT_GROUP_ORDER) {
      expect(
        SHORTCUT_ACTIONS.filter((action) => action.group === group).map(
          (action) => action.label,
        ),
      ).toEqual(labels[group]);
    }
    expect(new Set(SHORTCUT_ACTIONS.map((action) => action.id)).size).toBe(
      SHORTCUT_ACTIONS.length,
    );
    expect(new Set(SHORTCUT_ACTIONS.map((action) => action.label)).size).toBe(
      SHORTCUT_ACTIONS.length,
    );
  });

  it('stores Mod as a modifier and draws ⌘ on macOS, Ctrl elsewhere', () => {
    const palette = shortcutAction('palette.open').bindings[0];
    const accept = shortcutAction('review.accept').bindings[0];
    expect(formatBinding(palette, 'macos')).toBe('⌘+K');
    expect(formatBinding(palette, 'linux')).toBe('Ctrl+K');
    expect(formatBinding(palette, 'windows')).toBe('Ctrl+K');
    expect(formatBinding(palette, 'wsl')).toBe('Ctrl+K');
    expect(formatBinding(accept, 'macos')).toBe('⌘+Shift+Enter');
    expect(formatBinding(accept, 'linux')).toBe('Ctrl+Shift+Enter');
  });

  it('encodes G, M, and R sequences and does not make P a leader', () => {
    expect(sequenceLeaders()).toEqual(new Set(['g', 'm', 'r']));
    expect(formatBinding(shortcutAction('nav.boards').bindings[0], 'linux')).toBe(
      'G then B',
    );
    expect(formatBinding(shortcutAction('board.column.1').bindings[0], 'linux')).toBe(
      'G then 1',
    );
    expect(formatBinding(shortcutAction('board.column.9').bindings[0], 'linux')).toBe(
      'G then 9',
    );
    expect(formatBinding(shortcutAction('ticket.moveNext').bindings[0], 'linux')).toBe(
      'M then L',
    );
    expect(
      formatBinding(shortcutAction('ticket.movePrevious').bindings[0], 'linux'),
    ).toBe('M then H');
    expect(formatBinding(shortcutAction('review.approvePlan').bindings[0], 'linux')).toBe(
      'R then P',
    );
    expect(formatBinding(shortcutAction('review.merge').bindings[0], 'linux')).toBe(
      'R then M',
    );
    expect(formatBinding(shortcutAction('help.show').bindings[0], 'linux')).toBe('?');
    expect(formatBinding(shortcutAction('close').bindings[0], 'linux')).toBe('Esc');
    expect(formatBinding(shortcutAction('board.nextCard').bindings[1], 'linux')).toBe(
      'Down',
    );
  });

  it('omits admin Go to rows for members and Users in the desktop app', () => {
    expect(omittedActionIds({ isAdmin: false, desktopMode: false })).toEqual(
      new Set(['nav.plugins', 'nav.users', 'nav.tools', 'nav.settings']),
    );
    expect(omittedActionIds({ isAdmin: true, desktopMode: false }).size).toBe(0);
    expect(omittedActionIds({ isAdmin: true, desktopMode: true })).toEqual(
      new Set(['nav.users']),
    );
  });

  it('leaves reserved browser and OS chords out of the defaults', () => {
    expect(RESERVED_RULES.map((rule) => rule.id)).toContain('profile-menu');
    expect(RESERVED_RULES.some((rule) => rule.kind === 'chord' && rule.key === 'k')).toBe(
      false,
    );

    for (const action of SHORTCUT_ACTIONS) {
      for (const binding of action.bindings) {
        if (binding.kind !== 'chord') continue;
        for (const platform of ['macos', 'linux', 'windows', 'wsl'] as const) {
          expect(
            isReservedChord(eventForChord(binding.chord, platform), platform),
          ).toBe(false);
        }
      }
    }
  });
});

describe('platform Mod', () => {
  it('uses the meta key on macOS and Ctrl on Linux, Windows, and WSL', () => {
    expect(detectPlatform({ platform: 'MacIntel', userAgent: '' })).toBe('macos');
    expect(detectPlatform({ platform: 'Linux x86_64', userAgent: '' })).toBe('linux');
    expect(detectPlatform({ platform: 'Win32', userAgent: '' })).toBe('windows');
    expect(
      detectPlatform({ platform: 'Linux', userAgent: 'X11; Linux x86_64 Microsoft' }),
    ).toBe('wsl');
    expect(modPressed({ metaKey: true, ctrlKey: false }, 'macos')).toBe(true);
    expect(modPressed({ metaKey: false, ctrlKey: true }, 'macos')).toBe(false);
    expect(modPressed({ metaKey: false, ctrlKey: true }, 'linux')).toBe(true);
    expect(modPressed({ metaKey: true, ctrlKey: false }, 'wsl')).toBe(false);
    expect(modPressed({ metaKey: false, ctrlKey: true }, 'windows')).toBe(true);
  });
});

describe('reserved chords', () => {
  const base: KeyEventLike = {
    key: '',
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
  };

  it('reserves the browser and OS list, including Mod+Shift+M, and not Mod+K', () => {
    expect(
      isReservedChord(
        { ...base, key: 'k', metaKey: true },
        'macos',
      ),
    ).toBe(false);
    expect(isReservedChord({ ...base, key: 'k', ctrlKey: true }, 'linux')).toBe(false);
    expect(
      isReservedChord({ ...base, key: 'm', shiftKey: true, metaKey: true }, 'macos'),
    ).toBe(true);
    expect(
      isReservedChord({ ...base, key: 'm', shiftKey: true, ctrlKey: true }, 'linux'),
    ).toBe(true);
    expect(
      isReservedChord({ ...base, key: 'm', shiftKey: true, ctrlKey: true }, 'windows'),
    ).toBe(true);
    expect(
      isReservedChord(
        { ...base, key: '{', code: 'BracketLeft', shiftKey: true, metaKey: true },
        'macos',
      ),
    ).toBe(true);
    expect(
      isReservedChord(
        { ...base, key: '{', code: 'BracketLeft', shiftKey: true, ctrlKey: true },
        'linux',
      ),
    ).toBe(false);
    expect(
      isReservedChord(
        { ...base, key: 'ArrowLeft', altKey: true, metaKey: true },
        'macos',
      ),
    ).toBe(true);
    expect(isReservedChord({ ...base, key: 'ArrowLeft', altKey: true }, 'linux')).toBe(
      true,
    );
    expect(isReservedChord({ ...base, key: 'q', metaKey: true }, 'macos')).toBe(true);
    expect(isReservedChord({ ...base, key: 'q', ctrlKey: true }, 'wsl')).toBe(true);
    expect(isReservedChord({ ...base, key: '1', ctrlKey: true }, 'windows')).toBe(true);
    expect(
      isReservedChord({ ...base, key: 't', ctrlKey: true, altKey: true }, 'linux'),
    ).toBe(true);
    expect(
      isReservedChord({ ...base, key: 't', ctrlKey: true, altKey: true }, 'macos'),
    ).toBe(false);
    expect(
      isReservedChord({ ...base, key: 'k', ctrlKey: true }, 'linux', { terminal: true }),
    ).toBe(true);
    expect(
      isReservedChord({ ...base, key: 'k', ctrlKey: true }, 'linux', { terminal: false }),
    ).toBe(false);
    expect(
      isReservedChord({ ...base, key: 'k', ctrlKey: true }, 'macos', { terminal: true }),
    ).toBe(false);
    expect(isReservedChord({ ...base, key: 'c', metaKey: true }, 'macos')).toBe(true);
    expect(isReservedChord({ ...base, key: 'h', metaKey: true }, 'macos')).toBe(true);
    expect(isReservedChord({ ...base, key: 'h', ctrlKey: true }, 'linux')).toBe(false);
  });
});
