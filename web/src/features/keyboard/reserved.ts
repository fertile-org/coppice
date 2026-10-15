import { modPressed, type Platform } from './platform';
import { normalizeEventKey, type KeyEventLike } from './types';

const ALL_PLATFORMS: readonly Platform[] = ['macos', 'linux', 'windows', 'wsl'];
const NON_MAC: readonly Platform[] = ['linux', 'windows', 'wsl'];

export type ReservedRule =
  | {
      kind: 'chord';
      id: string;
      key: string;
      mod?: boolean;
      shift?: boolean;
      alt?: boolean;
      platforms: readonly Platform[];
    }
  | { kind: 'ctrl-alt'; id: string; platforms: readonly Platform[] }
  | { kind: 'ctrl-k-terminal'; id: string; platforms: readonly Platform[] };

function modChord(
  id: string,
  key: string,
  extras?: { shift?: boolean; alt?: boolean; platforms?: readonly Platform[] },
): ReservedRule {
  return {
    kind: 'chord',
    id,
    key,
    mod: true,
    shift: extras?.shift,
    alt: extras?.alt,
    platforms: extras?.platforms ?? ALL_PLATFORMS,
  };
}

/**
 * Chords the default keymap leaves to the browser or the OS.
 * `Mod+K` is intentionally absent: the palette reclaims it later.
 */
export const RESERVED_RULES: readonly ReservedRule[] = [
  modChord('quit', 'q'),
  modChord('close-tab', 'w'),
  modChord('new-tab', 't'),
  modChord('new-window', 'n'),
  modChord('reload', 'r'),
  modChord('address-bar', 'l'),
  modChord('reopen-tab', 't', { shift: true }),
  modChord('close-window', 'w', { shift: true }),
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => modChord(`tab-${n}`, String(n))),
  modChord('safari-previous-tab', '[', { shift: true, platforms: ['macos'] }),
  modChord('safari-next-tab', ']', { shift: true, platforms: ['macos'] }),
  modChord('chrome-tab-left', 'arrowleft', { alt: true, platforms: ['macos'] }),
  modChord('chrome-tab-right', 'arrowright', { alt: true, platforms: ['macos'] }),
  modChord('chrome-tab-up', 'arrowup', { alt: true, platforms: ['macos'] }),
  modChord('chrome-tab-down', 'arrowdown', { alt: true, platforms: ['macos'] }),
  {
    kind: 'chord',
    id: 'history-back',
    key: 'arrowleft',
    alt: true,
    platforms: ALL_PLATFORMS,
  },
  {
    kind: 'chord',
    id: 'history-forward',
    key: 'arrowright',
    alt: true,
    platforms: ALL_PLATFORMS,
  },
  modChord('hide', 'h', { platforms: ['macos'] }),
  modChord('minimize', 'm', { platforms: ['macos'] }),
  modChord('switch-app', 'tab', { platforms: ['macos'] }),
  modChord('spotlight', 'space', { platforms: ['macos'] }),
  modChord('copy', 'c'),
  modChord('paste', 'v'),
  modChord('cut', 'x'),
  modChord('select-all', 'a'),
  modChord('undo', 'z'),
  modChord('profile-menu', 'm', { shift: true }),
  { kind: 'ctrl-alt', id: 'ctrl-alt', platforms: NON_MAC },
  { kind: 'ctrl-k-terminal', id: 'terminal-kill', platforms: NON_MAC },
];

function reservedKeyMatches(ruleKey: string, event: KeyEventLike): boolean {
  if (ruleKey === '[') {
    return event.key === '[' || event.key === '{' || event.code === 'BracketLeft';
  }
  if (ruleKey === ']') {
    return event.key === ']' || event.key === '}' || event.code === 'BracketRight';
  }
  return normalizeEventKey(event) === ruleKey;
}

function chordRuleMatches(
  rule: Extract<ReservedRule, { kind: 'chord' }>,
  event: KeyEventLike,
  platform: Platform,
): boolean {
  if (!rule.platforms.includes(platform)) return false;
  if (Boolean(rule.mod) !== modPressed(event, platform)) return false;
  if (Boolean(rule.alt) !== event.altKey) return false;
  if (Boolean(rule.shift) !== event.shiftKey) return false;
  if (!rule.mod && (event.ctrlKey || event.metaKey)) return false;
  if (rule.mod && platform === 'macos' && event.ctrlKey) return false;
  if (rule.mod && platform !== 'macos' && event.metaKey) return false;
  return reservedKeyMatches(rule.key, event);
}

export function isReservedChord(
  event: KeyEventLike,
  platform: Platform,
  options?: { terminal?: boolean },
): boolean {
  for (const rule of RESERVED_RULES) {
    if (!rule.platforms.includes(platform)) continue;
    if (rule.kind === 'ctrl-alt') {
      if (event.ctrlKey && event.altKey && !event.metaKey) return true;
      continue;
    }
    if (rule.kind === 'ctrl-k-terminal') {
      if (!options?.terminal) continue;
      if (
        event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !event.shiftKey &&
        normalizeEventKey(event) === 'k'
      ) {
        return true;
      }
      continue;
    }
    if (chordRuleMatches(rule, event, platform)) return true;
  }
  return false;
}
