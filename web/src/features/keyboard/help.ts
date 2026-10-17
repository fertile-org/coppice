import { paletteShortcut } from './palette';
import { SHORTCUT_ACTIONS } from './registry';
import { SHORTCUT_GROUP_ORDER, type ShortcutAction, type ShortcutGroup } from './types';
import type { Platform } from './platform';

export const HELP_COPY = {
  title: 'Keyboard shortcuts',
  footer: 'Change shortcuts in Settings.',
  skipped:
    "Some shortcuts in Settings couldn't be used. Those rows show the defaults.",
} as const;

export type HelpRow = {
  id: string;
  label: string;
  shortcut: string;
};

export type HelpSection = {
  group: ShortcutGroup;
  rows: HelpRow[];
};

/**
 * Help rows use the registry label. Column jumps stay "Go to column {n}".
 * The palette is the surface that renames those rows to the column.
 */
export function helpSections(input: {
  omitted: ReadonlySet<string>;
  platform: Platform;
  actions?: readonly ShortcutAction[];
}): HelpSection[] {
  const actions = (input.actions ?? SHORTCUT_ACTIONS).filter(
    (action) => !input.omitted.has(action.id),
  );
  return SHORTCUT_GROUP_ORDER.map((group) => ({
    group,
    rows: actions
      .filter((action) => action.group === group)
      .map((action) => ({
        id: action.id,
        label: action.label,
        shortcut: paletteShortcut(action, input.platform),
      })),
  })).filter((section) => section.rows.length > 0);
}
