import { z } from 'zod';
import { apiFetch } from '../../lib/api';
import { SHORTCUT_ACTIONS } from '../keyboard/registry';

/** Data and wording for the Settings UI mode (`SettingsUi`). */

export const SETTINGS_UI_QUERY_KEY = ['settings-config-ui'] as const;

const tomlValue: z.ZodType<TomlValue> = z.lazy(() =>
  z.union([z.string(), z.number(), z.boolean(), z.array(tomlValue)]),
);
export type TomlValue = string | number | boolean | TomlValue[];

const kindSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('bool') }),
  z.object({ type: z.literal('optionalBool'), unsetLabel: z.string() }),
  z.object({
    type: z.literal('integer'),
    min: z.number().nullable(),
    max: z.number().nullable(),
  }),
  z.object({ type: z.literal('integerChoice'), options: z.array(z.number()) }),
  z.object({ type: z.literal('string') }),
  z.object({ type: z.literal('choice'), options: z.array(z.string()) }),
  z.object({
    type: z.literal('stringList'),
    options: z.array(z.string()).nullable(),
  }),
  z.object({ type: z.literal('shortcut') }),
]);
export type SettingKind = z.infer<typeof kindSchema>;

const settingSchema = z.object({
  key: z.string(),
  path: z.array(z.string()),
  group: z.string(),
  description: z.string(),
  kind: kindSchema,
  default: tomlValue.nullable(),
  defaultLabel: z.string().nullable(),
  restart: z.boolean(),
  value: tomlValue.nullable(),
});
export type Setting = z.infer<typeof settingSchema>;

const configUiSchema = z.object({
  path: z.string(),
  revision: z.string(),
  backupAvailable: z.boolean(),
  fileError: z.string().nullable(),
  settings: z.array(settingSchema),
});
export type ConfigUi = z.infer<typeof configUiSchema>;

export const saveResultSchema = z.object({
  revision: z.string(),
  restartRequired: z.boolean(),
});

export const SETTINGS_UI_COPY = {
  search: 'Search settings',
  noMatches: 'No settings match.',
  reset: 'Reset',
  modified: 'Modified',
  saved: 'Saved.',
  savedRestart: 'Saved. Restart Coppice to apply.',
  restartHint: 'Needs a restart.',
  conflict: 'config.toml changed outside Settings. Try again.',
  fileError: (message: string) =>
    `config.toml can't be read: ${message}. Turn on Edit as TOML to fix it.`,
  listHint: 'Separate values with commas.',
  shortcutHint: 'Separate keys with " or ". Leave empty to turn the shortcut off.',
  on: 'On',
  off: 'Off',
  defaultIs: (text: string) => `Default: ${text}`,
} as const;

export async function readConfigUi(): Promise<ConfigUi> {
  const response = await apiFetch('/api/settings/config/ui');
  return configUiSchema.parse(await response.json());
}

const SHORTCUT_LABELS = new Map(SHORTCUT_ACTIONS.map((action) => [action.id, action.label]));

function humanize(segment: string): string {
  const words = segment.replace(/[_-]+/g, ' ').trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** A readable title: the shortcut's name, or the key's last parts. */
export function settingTitle(setting: Setting): string {
  if (setting.kind.type === 'shortcut') {
    return SHORTCUT_LABELS.get(setting.path[1] ?? '') ?? setting.path[1] ?? setting.key;
  }
  const parts = setting.path;
  // `Auto assign · Backlog` reads better than `Backlog` alone.
  const nested = (parts[0] === 'workflow' || parts[0] === 'knowledge') && parts.length > 2;
  return (nested ? parts.slice(1) : parts.slice(-1)).map(humanize).join(' · ');
}

export function shortcutText(value: TomlValue | null): string {
  if (value == null) return '';
  if (Array.isArray(value)) return value.map(String).join(' or ');
  return String(value);
}

export function listText(value: TomlValue | null): string {
  return Array.isArray(value) ? value.map(String).join(', ') : '';
}

function formatValue(kind: SettingKind, value: TomlValue | null): string {
  if (value == null) return '';
  switch (kind.type) {
    case 'bool':
    case 'optionalBool':
      return value ? SETTINGS_UI_COPY.on : SETTINGS_UI_COPY.off;
    case 'shortcut':
      return shortcutText(value) || '(none)';
    case 'stringList':
      return listText(value) || '(none)';
    default:
      return String(value);
  }
}

export function defaultText(setting: Setting): string {
  return setting.defaultLabel ?? formatValue(setting.kind, setting.default);
}

export function matches(setting: Setting, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return [setting.key, setting.group, setting.description, settingTitle(setting)].some((text) =>
    text.toLowerCase().includes(needle),
  );
}
