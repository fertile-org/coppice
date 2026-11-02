import type { Plugin, PluginStatus } from '../../lib/schemas/plugin';

export function statusPillClass(status: PluginStatus): string {
  const base =
    'inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 font-body text-xs';
  switch (status) {
    case 'ok':
      return `${base} border-success-muted bg-success-muted text-success`;
    case 'shadowed':
      return `${base} border-info-muted bg-info-muted text-info`;
    case 'missing':
      return `${base} border-warning-muted bg-warning-muted text-warning`;
    case 'invalid':
      return `${base} border-danger-muted bg-danger-muted/40 text-danger`;
    case 'external':
      return `${base} border-border bg-paper-200 text-text-secondary`;
  }
}

export const STATUS_HINTS: Record<PluginStatus, string> = {
  ok: 'Loaded and ready to enable.',
  shadowed:
    'Another plugin with the same name in an earlier plugin directory is used instead.',
  missing: 'The folder is gone from disk. Agent assignments are kept until it returns.',
  invalid: 'The plugin could not be read. See the error below.',
  external: 'Lives in another repository; install it separately.',
};

export type InstalledFilter = 'all' | 'enabled' | 'disabled' | 'attention';

export type RowTone = 'danger' | 'warning' | 'muted';

export interface RowSecondary {
  text: string;
  alert: boolean;
  tone: RowTone;
}

export function pluginNeedsAttention(plugin: Plugin): boolean {
  if (plugin.status === 'invalid' || plugin.status === 'missing') return true;
  if (plugin.status !== 'ok') return false;
  if (plugin.settings.some((s) => s.source === 'missing' || s.needsReentry === true)) {
    return true;
  }
  return plugin.mcpServers.some((s) => s.health === 'unhealthy');
}

export function attentionReason(plugin: Plugin): string | null {
  const setting = plugin.settings.find(
    (s) => s.source === 'missing' || s.needsReentry === true,
  );
  if (setting) return setting.key;
  const server = plugin.mcpServers.find((s) => s.health === 'unhealthy');
  return server?.name ?? null;
}

export function rowSecondaryText(
  plugin: Plugin,
  actionError: string | null,
): RowSecondary | null {
  if (actionError) return { text: actionError, alert: true, tone: 'danger' };
  if (plugin.error) return { text: plugin.error, alert: false, tone: 'danger' };
  if (plugin.status === 'missing') {
    return { text: STATUS_HINTS.missing, alert: false, tone: 'warning' };
  }
  const reason = attentionReason(plugin);
  if (reason) return { text: reason, alert: false, tone: 'warning' };
  if (plugin.description) return { text: plugin.description, alert: false, tone: 'muted' };
  return null;
}

export function matchesSearch(plugin: Plugin, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return (
    plugin.name.toLowerCase().includes(needle) ||
    plugin.description.toLowerCase().includes(needle)
  );
}

export function matchesFilter(plugin: Plugin, filter: InstalledFilter): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'enabled':
      return plugin.enabled;
    case 'disabled':
      return !plugin.enabled;
    case 'attention':
      return pluginNeedsAttention(plugin);
  }
}

export function compareInstalled(a: Plugin, b: Plugin): number {
  const attn = Number(pluginNeedsAttention(b)) - Number(pluginNeedsAttention(a));
  if (attn !== 0) return attn;
  return a.name.localeCompare(b.name);
}

export function visibleInstalledPlugins(
  plugins: Plugin[],
  query: string,
  filter: InstalledFilter,
): { visible: Plugin[]; counts: Record<InstalledFilter, number> } {
  const searched = plugins.filter((plugin) => matchesSearch(plugin, query));
  const counts = {
    all: searched.length,
    enabled: searched.filter((plugin) => plugin.enabled).length,
    disabled: searched.filter((plugin) => !plugin.enabled).length,
    attention: searched.filter(pluginNeedsAttention).length,
  };
  return {
    visible: searched.filter((plugin) => matchesFilter(plugin, filter)).sort(compareInstalled),
    counts,
  };
}
