import { describe, expect, it } from 'vitest';
import type { Plugin } from '../../lib/schemas/plugin';
import {
  pluginNeedsAttention,
  rowSecondaryText,
  STATUS_HINTS,
  visibleInstalledPlugins,
} from './installedPlugins';

function plugin(overrides: Partial<Plugin> = {}): Plugin {
  return {
    id: '00000000-0000-4000-8000-000000000010',
    pluginDirId: '00000000-0000-4000-8000-000000000001',
    relPath: 'sample',
    name: 'sample',
    version: '1.0.0',
    description: 'A sample plugin',
    source: 'local',
    gitUrl: null,
    gitRef: null,
    gitCommit: null,
    status: 'ok',
    error: null,
    enabled: false,
    skills: [],
    mcpServers: [],
    settings: [],
    unsupported: [],
    marketplace: null,
    external: null,
    gitRoot: null,
    agentAccess: 'explicit',
    agentIds: [],
    ...overrides,
  };
}

describe('pluginNeedsAttention', () => {
  it('flags invalid and missing, not shadowed or external', () => {
    expect(pluginNeedsAttention(plugin({ status: 'invalid' }))).toBe(true);
    expect(pluginNeedsAttention(plugin({ status: 'missing' }))).toBe(true);
    expect(pluginNeedsAttention(plugin({ status: 'shadowed' }))).toBe(false);
    expect(pluginNeedsAttention(plugin({ status: 'external' }))).toBe(false);
    expect(pluginNeedsAttention(plugin({ status: 'ok' }))).toBe(false);
  });

  it('flags an ok plugin with a missing setting, re-entry, or unhealthy MCP', () => {
    expect(
      pluginNeedsAttention(
        plugin({ settings: [{ key: 'TOKEN', configured: false, source: 'missing' }] }),
      ),
    ).toBe(true);
    expect(
      pluginNeedsAttention(
        plugin({
          settings: [
            { key: 'TOKEN', configured: false, source: 'setting', needsReentry: true },
          ],
        }),
      ),
    ).toBe(true);
    expect(
      pluginNeedsAttention(
        plugin({ mcpServers: [{ name: 'docs', kind: 'stdio', health: 'unhealthy' }] }),
      ),
    ).toBe(true);
  });
});

describe('visibleInstalledPlugins', () => {
  const invalid = plugin({
    id: '00000000-0000-4000-8000-000000000001',
    name: 'broken',
    status: 'invalid',
    description: 'Cannot load',
  });
  const missingSetting = plugin({
    id: '00000000-0000-4000-8000-000000000002',
    name: 'github',
    description: 'Issues and PRs',
    settings: [{ key: 'GITHUB_TOKEN', configured: false, source: 'missing' }],
  });
  const reentry = plugin({
    id: '00000000-0000-4000-8000-000000000003',
    name: 'secrets',
    description: 'Needs the token again',
    settings: [{ key: 'API_TOKEN', configured: false, source: 'missing', needsReentry: true }],
  });
  const unhealthy = plugin({
    id: '00000000-0000-4000-8000-000000000004',
    name: 'mcp-fake',
    description: 'Talks to a server',
    mcpServers: [{ name: 'fake', kind: 'stdio', health: 'unhealthy' }],
  });
  const shadowed = plugin({
    id: '00000000-0000-4000-8000-000000000005',
    name: 'shadowed-plugin',
    status: 'shadowed',
    description: 'Duplicate name',
  });
  const enabled = plugin({
    id: '00000000-0000-4000-8000-000000000006',
    name: 'ponytail',
    enabled: true,
    description: 'Lazy senior mode',
  });

  const all = [enabled, shadowed, unhealthy, reentry, missingSetting, invalid];

  it('search matches name and description; counts follow the search', () => {
    const { visible, counts } = visibleInstalledPlugins(all, 'token', 'all');
    expect(visible.map((p) => p.name)).toEqual(['secrets']);
    expect(counts).toEqual({ all: 1, enabled: 0, disabled: 1, attention: 1 });
  });

  it('each filter narrows the list and counts match plugins shown for that search', () => {
    const searched = visibleInstalledPlugins(all, '', 'all');
    expect(searched.counts).toEqual({ all: 6, enabled: 1, disabled: 5, attention: 4 });
    expect(visibleInstalledPlugins(all, '', 'enabled').visible.map((p) => p.name)).toEqual([
      'ponytail',
    ]);
    expect(visibleInstalledPlugins(all, '', 'disabled').visible).toHaveLength(5);
    expect(visibleInstalledPlugins(all, '', 'attention').visible.map((p) => p.name)).toEqual([
      'broken',
      'github',
      'mcp-fake',
      'secrets',
    ]);
  });

  it('sorts needs-attention first, then by name; shadowed is not attention', () => {
    const { visible } = visibleInstalledPlugins(all, '', 'all');
    expect(visible.map((p) => p.name)).toEqual([
      'broken',
      'github',
      'mcp-fake',
      'secrets',
      'ponytail',
      'shadowed-plugin',
    ]);
  });
});

describe('rowSecondaryText', () => {
  it('prefers action error, then plugin.error, missing hint, attention reason, description', () => {
    const base = plugin({
      error: 'could not parse',
      status: 'missing',
      description: 'The folder is gone',
      settings: [{ key: 'ROOT', configured: false, source: 'missing' }],
    });
    expect(rowSecondaryText(base, 'Update failed: network')).toEqual({
      text: 'Update failed: network',
      alert: true,
      tone: 'danger',
    });
    expect(rowSecondaryText(base, null)).toEqual({
      text: 'could not parse',
      alert: false,
      tone: 'danger',
    });
    expect(rowSecondaryText({ ...base, error: null }, null)).toEqual({
      text: STATUS_HINTS.missing,
      alert: false,
      tone: 'warning',
    });
    expect(
      rowSecondaryText(
        plugin({ settings: [{ key: 'ROOT', configured: false, source: 'missing' }] }),
        null,
      ),
    ).toEqual({ text: 'ROOT', alert: false, tone: 'warning' });
    expect(
      rowSecondaryText(
        plugin({ mcpServers: [{ name: 'fake', kind: 'stdio', health: 'unhealthy' }] }),
        null,
      ),
    ).toEqual({ text: 'fake', alert: false, tone: 'warning' });
    expect(rowSecondaryText(plugin(), null)).toEqual({
      text: 'A sample plugin',
      alert: false,
      tone: 'muted',
    });
  });
});
