import { z } from 'zod';
import { apiFetch } from '../../lib/api';
import { SHORTCUT_ACTIONS } from './registry';
import { formatKey, type Binding, type ShortcutAction } from './types';

const bindingSchema = z.discriminatedUnion('kind', [
  z.object({
    kind: z.literal('chord'),
    chord: z.object({
      key: z.string(),
      mod: z.boolean().optional(),
      shift: z.boolean().optional(),
      alt: z.boolean().optional(),
    }),
  }),
  z.object({
    kind: z.literal('sequence'),
    keys: z.tuple([z.string(), z.string()]),
  }),
]);

const responseSchema = z.object({
  overrides: z.record(z.string(), z.array(bindingSchema)),
});

export type KeybindingOverrides = Record<string, Binding[]>;

/** Config-file form of a binding: `Mod+K`, `G then B`. */
export function configBinding(binding: Binding): string {
  if (binding.kind === 'sequence') {
    return `${formatKey(binding.keys[0])} then ${formatKey(binding.keys[1])}`;
  }
  const parts: string[] = [];
  if (binding.chord.mod) parts.push('Mod');
  if (binding.chord.alt) parts.push('Alt');
  if (binding.chord.shift) parts.push('Shift');
  parts.push(formatKey(binding.chord.key));
  return parts.join('+');
}

export function applyKeybindingOverrides(
  actions: readonly ShortcutAction[],
  overrides: KeybindingOverrides,
): ShortcutAction[] {
  return actions.map((action) => {
    if (!Object.prototype.hasOwnProperty.call(overrides, action.id)) return action;
    return { ...action, bindings: overrides[action.id] ?? [] };
  });
}

let current: KeybindingOverrides = {};
const listeners = new Set<(overrides: KeybindingOverrides) => void>();

export function currentKeybindings(): KeybindingOverrides {
  return current;
}

export function publishKeybindings(overrides: KeybindingOverrides): void {
  if (JSON.stringify(current) === JSON.stringify(overrides)) return;
  current = overrides;
  for (const listener of listeners) listener(overrides);
}

export function subscribeKeybindings(
  listener: (overrides: KeybindingOverrides) => void,
): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export async function readKeybindings(): Promise<KeybindingOverrides> {
  const response = await apiFetch('/api/keybindings');
  return responseSchema.parse(await response.json()).overrides;
}

export function effectiveShortcutActions(
  overrides: KeybindingOverrides = current,
): ShortcutAction[] {
  return applyKeybindingOverrides(SHORTCUT_ACTIONS, overrides);
}
