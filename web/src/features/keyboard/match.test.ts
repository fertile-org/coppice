import { describe, expect, it } from 'vitest';
import {
  activeScopes,
  EMPTY_SEQUENCE_SESSION,
  reduceShortcut,
  SEQUENCE_TIMEOUT_MS,
  type MatchContext,
  type SequenceSession,
} from './match';
import { omittedActionIds } from './registry';
import type { KeyEventLike } from './types';

const baseEvent: KeyEventLike = {
  key: '',
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
};

function ctx(overrides: Partial<MatchContext> = {}): MatchContext {
  return {
    platform: 'linux',
    scopes: ['global', 'board'],
    enabled: new Set([
      'board.nextCard',
      'board.previousCard',
      'board.nextColumn',
      'board.column.3',
      'close',
    ]),
    omitted: new Set(),
    ...overrides,
  };
}

function key(keyName: string, extras: Partial<KeyEventLike> = {}): KeyEventLike {
  return { ...baseEvent, key: keyName, ...extras };
}

function press(
  session: SequenceSession,
  event: KeyEventLike,
  match: MatchContext,
  now = 0,
) {
  return reduceShortcut(session, event, match, now);
}

describe('shortcut matching', () => {
  it('resolves scopes and suppresses shortcuts while typing or in a dialog', () => {
    expect(
      activeScopes({
        pathname: '/boards/board-1',
        drawerOpen: false,
        modalOpen: false,
        targetKind: 'plain',
        foreignDialog: false,
      }),
    ).toEqual(['global', 'board']);
    expect(
      activeScopes({
        pathname: '/boards/board-1',
        drawerOpen: true,
        modalOpen: false,
        targetKind: 'plain',
        foreignDialog: false,
      }),
    ).toEqual(['global', 'ticket']);
    expect(
      activeScopes({
        pathname: '/boards/board-1',
        drawerOpen: false,
        modalOpen: false,
        targetKind: 'text',
        foreignDialog: false,
      }),
    ).toEqual(['text']);
    expect(
      activeScopes({
        pathname: '/boards/board-1',
        drawerOpen: true,
        modalOpen: false,
        targetKind: 'terminal',
        foreignDialog: false,
      }),
    ).toEqual(['terminal']);
    expect(
      activeScopes({
        pathname: '/code',
        drawerOpen: false,
        modalOpen: false,
        targetKind: 'plain',
        foreignDialog: false,
      }),
    ).toEqual(['global', 'diff']);
    expect(
      activeScopes({
        pathname: '/boards/board-1',
        drawerOpen: false,
        modalOpen: true,
        targetKind: 'plain',
        foreignDialog: false,
      }),
    ).toEqual([]);
  });

  it('moves with J and K, ignores them while typing, and lets a disabled Mod+K through', () => {
    const board = ctx();
    expect(press(EMPTY_SEQUENCE_SESSION, key('j'), board).effect).toEqual({
      type: 'run',
      actionId: 'board.nextCard',
    });
    expect(press(EMPTY_SEQUENCE_SESSION, key('K'), board).effect).toEqual({
      type: 'run',
      actionId: 'board.previousCard',
    });
    expect(press(EMPTY_SEQUENCE_SESSION, key('ArrowDown'), board).effect).toEqual({
      type: 'run',
      actionId: 'board.nextCard',
    });
    expect(
      press(EMPTY_SEQUENCE_SESSION, key('j'), ctx({ scopes: ['text'] })).effect,
    ).toEqual({ type: 'none' });
    expect(
      press(
        EMPTY_SEQUENCE_SESSION,
        key('k', { ctrlKey: true }),
        ctx({ platform: 'linux', scopes: ['global', 'text'] }),
      ).effect,
    ).toEqual({ type: 'none' });
    expect(
      press(
        EMPTY_SEQUENCE_SESSION,
        key('k', { metaKey: true }),
        ctx({ platform: 'macos', scopes: ['global'] }),
      ).effect,
    ).toEqual({ type: 'none' });
    expect(
      press(
        EMPTY_SEQUENCE_SESSION,
        key('k', { metaKey: true }),
        ctx({
          platform: 'macos',
          scopes: ['global', 'text'],
          enabled: new Set(['palette.open']),
        }),
      ).effect,
    ).toEqual({ type: 'run', actionId: 'palette.open' });
  });

  it('runs G then a column, cancels an unmatched second key into its own shortcut, and swallows a disabled match', () => {
    const board = ctx();
    const led = press(EMPTY_SEQUENCE_SESSION, key('g'), board);
    expect(led.effect).toEqual({ type: 'none' });
    expect(led.session.pending?.leader).toBe('g');

    expect(press(led.session, key('3'), board, 100).effect).toEqual({
      type: 'run',
      actionId: 'board.column.3',
    });

    const unmatched = press(led.session, key('j'), board, 100);
    expect(unmatched.effect).toEqual({ type: 'run', actionId: 'board.nextCard' });

    const knowledge = press(led.session, key('k'), board, 100);
    expect(knowledge.effect).toEqual({ type: 'swallow' });
    expect(knowledge.session.pending).toBeNull();
  });

  it('cancels a leader after one second and does not reuse a disabled M or R sequence', () => {
    const board = ctx();
    const leader = press(EMPTY_SEQUENCE_SESSION, key('m'), board, 0);
    expect(leader.session.pending?.leader).toBe('m');
    expect(press(leader.session, key('l'), board, 999).effect).toEqual({
      type: 'swallow',
    });
    expect(press(leader.session, key('l'), board, SEQUENCE_TIMEOUT_MS).effect).toEqual({
      type: 'run',
      actionId: 'board.nextColumn',
    });

    const review = press(EMPTY_SEQUENCE_SESSION, key('r'), board, 0);
    expect(press(review.session, key('p'), board, 10).effect).toEqual({
      type: 'swallow',
    });
    expect(press(review.session, key('m'), board, 10).effect).toEqual({
      type: 'swallow',
    });
    expect(press(EMPTY_SEQUENCE_SESSION, key('p'), board).effect).toEqual({
      type: 'none',
    });
    expect(press(EMPTY_SEQUENCE_SESSION, key('p'), board).session.pending).toBeNull();
  });

  it('omits admin sequences for members instead of swallowing the second key', () => {
    const member = ctx({
      scopes: ['global'],
      enabled: new Set(),
      omitted: omittedActionIds({ isAdmin: false, desktopMode: false }),
    });
    const leader = press(EMPTY_SEQUENCE_SESSION, key('g'), member);
    expect(press(leader.session, key('p'), member, 10).effect).toEqual({
      type: 'none',
    });
    expect(press(leader.session, key('b'), member, 10).effect).toEqual({
      type: 'swallow',
    });
  });

  it('keeps Mod+Shift+Enter distinct from Enter and Mod+Enter', () => {
    const enabled = ctx({
      scopes: ['board', 'text'],
      enabled: new Set(['ticket.open', 'ticket.comment', 'review.accept']),
    });
    expect(press(EMPTY_SEQUENCE_SESSION, key('Enter'), enabled).effect).toEqual({
      type: 'run',
      actionId: 'ticket.open',
    });
    expect(
      press(EMPTY_SEQUENCE_SESSION, key('Enter', { ctrlKey: true }), enabled).effect,
    ).toEqual({ type: 'run', actionId: 'ticket.comment' });
    expect(
      press(
        EMPTY_SEQUENCE_SESSION,
        key('Enter', { ctrlKey: true, shiftKey: true }),
        enabled,
      ).effect,
    ).toEqual({ type: 'run', actionId: 'review.accept' });
    expect(
      press(
        EMPTY_SEQUENCE_SESSION,
        key('Enter', { metaKey: true, shiftKey: true }),
        ctx({
          platform: 'macos',
          scopes: ['board'],
          enabled: new Set(['review.accept']),
        }),
      ).effect,
    ).toEqual({ type: 'run', actionId: 'review.accept' });
  });

  it('handles Esc in text and the terminal and ignores other terminal keys', () => {
    expect(
      press(EMPTY_SEQUENCE_SESSION, key('Escape'), ctx({ scopes: ['text'] })).effect,
    ).toEqual({ type: 'run', actionId: 'close' });
    expect(
      press(EMPTY_SEQUENCE_SESSION, key('Escape'), ctx({ scopes: ['terminal'] })).effect,
    ).toEqual({ type: 'run', actionId: 'close' });
    expect(
      press(
        EMPTY_SEQUENCE_SESSION,
        key('k', { ctrlKey: true }),
        ctx({ scopes: ['terminal'], enabled: new Set(['palette.open', 'close']) }),
      ).effect,
    ).toEqual({ type: 'none' });
    expect(
      press(EMPTY_SEQUENCE_SESSION, key('j'), ctx({ scopes: ['terminal'] })).effect,
    ).toEqual({ type: 'none' });
  });
});
