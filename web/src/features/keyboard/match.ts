import { modPressed, type Platform } from './platform';
import { SHORTCUT_ACTIONS } from './registry';
import {
  normalizeEventKey,
  type KeyEventLike,
  type ShortcutAction,
  type ShortcutScope,
} from './types';

/** A leader waits this long, then the sequence cancels. */
export const SEQUENCE_TIMEOUT_MS = 1000;

export type SequenceSession = {
  pending: { leader: string; at: number } | null;
};

export const EMPTY_SEQUENCE_SESSION: SequenceSession = { pending: null };

export type MatchContext = {
  platform: Platform;
  scopes: readonly ShortcutScope[];
  enabled: ReadonlySet<string>;
  omitted: ReadonlySet<string>;
};

export type ShortcutEffect =
  | { type: 'none' }
  | { type: 'run'; actionId: string }
  | { type: 'swallow' };

export function activeScopes(input: {
  pathname: string;
  drawerOpen: boolean;
  modalOpen: boolean;
  targetKind: 'terminal' | 'text' | 'plain';
  foreignDialog: boolean;
}): ShortcutScope[] {
  if (input.modalOpen || input.foreignDialog) return [];
  if (input.targetKind === 'terminal') return ['terminal'];
  if (input.targetKind === 'text') return ['text'];

  const scopes: ShortcutScope[] = ['global'];
  if (input.pathname === '/code' || input.pathname.startsWith('/code/')) {
    scopes.push('diff');
  }
  const onBoard = /^\/boards\/[^/]+\/?$/.test(input.pathname);
  if (onBoard && input.drawerOpen) scopes.push('ticket');
  if (onBoard && !input.drawerOpen) scopes.push('board');
  return scopes;
}

function inScope(action: ShortcutAction, scopes: readonly ShortcutScope[]): boolean {
  return action.scopes.some((scope) => scopes.includes(scope));
}

function candidates(ctx: MatchContext): ShortcutAction[] {
  return SHORTCUT_ACTIONS.filter(
    (action) => !ctx.omitted.has(action.id) && inScope(action, ctx.scopes),
  );
}

function chordMatches(
  event: KeyEventLike,
  chord: { key: string; mod?: boolean; shift?: boolean; alt?: boolean },
  platform: Platform,
): boolean {
  if (event.altKey !== Boolean(chord.alt)) return false;
  if (Boolean(chord.mod) !== modPressed(event, platform)) return false;
  if (!chord.mod && (event.metaKey || event.ctrlKey)) return false;
  if (chord.mod && platform === 'macos' && event.ctrlKey) return false;
  if (chord.mod && platform !== 'macos' && event.metaKey) return false;

  if (chord.key === '?') {
    return event.key === '?' && !event.altKey && !event.metaKey && !event.ctrlKey;
  }

  if (normalizeEventKey(event) !== chord.key) return false;

  const plainLetter = /^[a-z]$/.test(chord.key) && !chord.mod && !chord.alt;
  if (plainLetter) return true;
  return event.shiftKey === Boolean(chord.shift);
}

/** Letter or digit without Ctrl, ⌘, or Alt. Shift is ignored for letters. */
export function sequenceToken(event: KeyEventLike): string | null {
  if (event.metaKey || event.ctrlKey || event.altKey || event.isComposing) return null;
  if (event.key === 'Process') return null;
  if (/^[a-z0-9]$/i.test(event.key)) return event.key.toLowerCase();
  return null;
}

function findSequence(
  ctx: MatchContext,
  leader: string,
  second: string,
): ShortcutAction | null {
  for (const action of candidates(ctx)) {
    for (const binding of action.bindings) {
      if (
        binding.kind === 'sequence' &&
        binding.keys[0] === leader &&
        binding.keys[1] === second
      ) {
        return action;
      }
    }
  }
  return null;
}

function isLeader(token: string, ctx: MatchContext): boolean {
  for (const action of candidates(ctx)) {
    for (const binding of action.bindings) {
      if (binding.kind === 'sequence' && binding.keys[0] === token) return true;
    }
  }
  return false;
}

function findChord(event: KeyEventLike, ctx: MatchContext): ShortcutAction | null {
  let best: { action: ShortcutAction; weight: number } | null = null;
  for (const action of candidates(ctx)) {
    for (const binding of action.bindings) {
      if (binding.kind !== 'chord') continue;
      if (!chordMatches(event, binding.chord, ctx.platform)) continue;
      const weight =
        (binding.chord.mod ? 1 : 0) +
        (binding.chord.shift ? 1 : 0) +
        (binding.chord.alt ? 1 : 0);
      if (!best || weight > best.weight) best = { action, weight };
    }
  }
  return best?.action ?? null;
}

function reduceFresh(
  event: KeyEventLike,
  ctx: MatchContext,
  now: number,
): { session: SequenceSession; effect: ShortcutEffect } {
  const token = sequenceToken(event);
  if (token && isLeader(token, ctx) && !event.repeat) {
    return { session: { pending: { leader: token, at: now } }, effect: { type: 'none' } };
  }
  const chord = findChord(event, ctx);
  if (!chord || !ctx.enabled.has(chord.id)) {
    return { session: EMPTY_SEQUENCE_SESSION, effect: { type: 'none' } };
  }
  return {
    session: EMPTY_SEQUENCE_SESSION,
    effect: { type: 'run', actionId: chord.id },
  };
}

export function reduceShortcut(
  session: SequenceSession,
  event: KeyEventLike,
  ctx: MatchContext,
  now: number,
): { session: SequenceSession; effect: ShortcutEffect } {
  if (event.isComposing || event.key === 'Process') {
    return { session, effect: { type: 'none' } };
  }
  if (ctx.scopes.length === 0) {
    return { session: EMPTY_SEQUENCE_SESSION, effect: { type: 'none' } };
  }

  let current = session;
  if (current.pending && now - current.pending.at >= SEQUENCE_TIMEOUT_MS) {
    current = EMPTY_SEQUENCE_SESSION;
  }

  if (current.pending && event.repeat && sequenceToken(event) === current.pending.leader) {
    return { session: current, effect: { type: 'none' } };
  }

  if (current.pending) {
    const second = sequenceToken(event);
    if (second == null) return reduceFresh(event, ctx, now);
    const match = findSequence(ctx, current.pending.leader, second);
    if (!match) return reduceFresh(event, ctx, now);
    if (!ctx.enabled.has(match.id)) {
      return { session: EMPTY_SEQUENCE_SESSION, effect: { type: 'swallow' } };
    }
    return {
      session: EMPTY_SEQUENCE_SESSION,
      effect: { type: 'run', actionId: match.id },
    };
  }

  return reduceFresh(event, ctx, now);
}
