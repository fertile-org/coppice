import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { apiFetch } from '../../lib/api';
import { helpSections } from './help';
import {
  applyKeybindingOverrides,
  configBinding,
  readKeybindings,
  type KeybindingOverrides,
} from './keybindings';

vi.mock('../../lib/api', () => ({
  apiFetch: vi.fn(),
}));
import { EMPTY_SEQUENCE_SESSION, reduceShortcut } from './match';
import { listPaletteActions } from './palette';
import { SHORTCUT_ACTIONS } from './registry';
import type { KeyEventLike } from './types';

function defaultKeybindingsFile(): Record<string, string[]> {
  const text = readFileSync(
    resolve(process.cwd(), '../config/default-keybindings.toml'),
    'utf8',
  );
  const parsed: Record<string, string[]> = {};
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const match = /^"([^"]+)"\s*=\s*\[(.*)\]\s*$/.exec(trimmed);
    if (!match) throw new Error(`unparsed default keybinding: ${line}`);
    parsed[match[1]] = [...match[2].matchAll(/"((?:\\.|[^"\\])*)"/g)].map((found) => found[1]);
  }
  return parsed;
}

const quiet: KeyEventLike = {
  key: '',
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
};

describe('keybinding overrides', () => {
  it('keeps the registry defaults aligned with config/default-keybindings.toml', () => {
    const defaults = defaultKeybindingsFile();
    expect(Object.keys(defaults).sort()).toEqual(SHORTCUT_ACTIONS.map((action) => action.id).sort());
    for (const action of SHORTCUT_ACTIONS) {
      expect(action.bindings.map(configBinding)).toEqual(defaults[action.id]);
    }
  });

  it('replaces a command, unbinds another, and shows the effective keys', () => {
    const overrides: KeybindingOverrides = {
      'palette.open': [{ kind: 'chord', chord: { key: 'p', mod: true } }],
      'ticket.create': [],
    };
    const actions = applyKeybindingOverrides(SHORTCUT_ACTIONS, overrides);
    expect(actions.find((action) => action.id === 'palette.open')?.bindings).toEqual(
      overrides['palette.open'],
    );
    expect(actions.find((action) => action.id === 'ticket.create')?.bindings).toEqual([]);
    expect(actions.find((action) => action.id === 'ticket.assign')?.bindings).toEqual(
      SHORTCUT_ACTIONS.find((action) => action.id === 'ticket.assign')?.bindings,
    );

    const help = helpSections({ omitted: new Set(), platform: 'linux', actions });
    const rows = help.flatMap((section) => section.rows);
    expect(rows.find((row) => row.id === 'palette.open')?.shortcut).toBe('Ctrl+P');
    expect(rows.find((row) => row.id === 'ticket.create')?.shortcut).toBe('');

    const palette = listPaletteActions({
      omitted: new Set(),
      scopes: ['global', 'text'],
      enabled: new Set(['palette.open']),
      platform: 'macos',
      actions,
    });
    expect(palette.find((entry) => entry.id === 'palette.open')?.shortcut).toBe('⌘+P');
  });

  it('reads chords and an empty unbind from the keybindings API', async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      json: async () => ({
        overrides: {
          'palette.open': [{ kind: 'chord', chord: { key: 'p', mod: true } }],
          'ticket.create': [],
        },
      }),
    } as Response);
    await expect(readKeybindings()).resolves.toEqual({
      'palette.open': [{ kind: 'chord', chord: { key: 'p', mod: true } }],
      'ticket.create': [],
    });
  });

  it('runs the override and leaves the default key alone', () => {
    const actions = applyKeybindingOverrides(SHORTCUT_ACTIONS, {
      'ticket.create': [{ kind: 'chord', chord: { key: 'x' } }],
    });
    const match = {
      platform: 'linux' as const,
      scopes: ['board' as const],
      enabled: new Set(['ticket.create']),
      omitted: new Set<string>(),
      actions,
    };
    expect(
      reduceShortcut(EMPTY_SEQUENCE_SESSION, { ...quiet, key: 'x' }, match, 0).effect,
    ).toEqual({ type: 'run', actionId: 'ticket.create' });
    expect(
      reduceShortcut(EMPTY_SEQUENCE_SESSION, { ...quiet, key: 'c' }, match, 0).effect,
    ).toEqual({ type: 'none' });
  });
});
