import type { Platform } from './platform';

export type ShortcutGroup = 'Navigate' | 'Tickets' | 'Review';

export const SHORTCUT_GROUP_ORDER: readonly ShortcutGroup[] = [
  'Navigate',
  'Tickets',
  'Review',
];

/** Where a shortcut is allowed to fire. */
export type ShortcutScope =
  | 'global'
  | 'board'
  | 'ticket'
  | 'text'
  | 'terminal'
  | 'diff';

export type Chord = {
  /** Lowercase name: `k`, `enter`, `arrowdown`, `escape`, `?`, `[`. */
  key: string;
  mod?: boolean;
  shift?: boolean;
  alt?: boolean;
};

export type Binding =
  | { kind: 'chord'; chord: Chord }
  | { kind: 'sequence'; keys: readonly [string, string] };

export type ShortcutAction = {
  id: string;
  /** Content-locked action name. */
  label: string;
  group: ShortcutGroup;
  bindings: readonly Binding[];
  scopes: readonly ShortcutScope[];
  /** Omitted for members, same as the admin sidebar rows. */
  adminOnly?: boolean;
  /** Omitted in the desktop app, same as the sidebar. */
  desktopOmit?: boolean;
};

export type KeyEventLike = {
  key: string;
  code?: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  repeat?: boolean;
  isComposing?: boolean;
};

export function normalizeEventKey(event: { key: string }): string {
  if (event.key === ' ') return 'space';
  return event.key.toLowerCase();
}

const KEY_LABELS: Record<string, string> = {
  arrowdown: 'Down',
  arrowup: 'Up',
  arrowleft: 'Left',
  arrowright: 'Right',
  enter: 'Enter',
  escape: 'Esc',
  space: 'Space',
  tab: 'Tab',
};

export function formatKey(key: string): string {
  if (KEY_LABELS[key]) return KEY_LABELS[key];
  if (/^[a-z]$/.test(key)) return key.toUpperCase();
  return key;
}

/** `⌘+K` on macOS, `Ctrl+K` elsewhere. Sequences render as `G then B`. */
export function formatBinding(binding: Binding, platform: Platform): string {
  if (binding.kind === 'sequence') {
    return `${formatKey(binding.keys[0])} then ${formatKey(binding.keys[1])}`;
  }
  const parts: string[] = [];
  if (binding.chord.mod) parts.push(platform === 'macos' ? '⌘' : 'Ctrl');
  if (binding.chord.alt) parts.push(platform === 'macos' ? 'Option' : 'Alt');
  if (binding.chord.shift) parts.push('Shift');
  parts.push(formatKey(binding.chord.key));
  return parts.join('+');
}
