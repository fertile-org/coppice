import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { RepoDrawer } from '../repos/RepoDrawer';
import type { Plugin, PluginInstall } from '../../lib/schemas/plugin';
import { cn } from '../../lib/utils';
import { PluginDetails } from './PluginDetails';
import { PluginRow } from './PluginRow';
import {
  type InstalledFilter,
  visibleInstalledPlugins,
} from './installedPlugins';
import { usePluginInstall, usePlugins, useUpdatePlugin } from './usePlugins';

const FILTERS: { id: InstalledFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'enabled', label: 'Enabled' },
  { id: 'disabled', label: 'Disabled' },
  { id: 'attention', label: 'Needs attention' },
];

function PluginInstallPoller({
  pluginId,
  installId,
  onChange,
}: {
  pluginId: string;
  installId: string;
  onChange: (pluginId: string, install: PluginInstall | undefined) => void;
}) {
  const { data } = usePluginInstall(installId);
  useEffect(() => {
    onChange(pluginId, data);
  }, [pluginId, data, onChange]);
  return null;
}

interface InstalledPluginsListProps {
  onInstallFromGit: (url: string) => void;
  highlightIds: string[];
}

export function InstalledPluginsList({
  onInstallFromGit,
  highlightIds,
}: InstalledPluginsListProps) {
  const pluginsQuery = usePlugins();
  const plugins = pluginsQuery.data ?? [];
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<InstalledFilter>('all');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [updateIds, setUpdateIds] = useState<Record<string, string>>({});
  const [updates, setUpdates] = useState<Record<string, PluginInstall | undefined>>({});
  const searchRef = useRef<HTMLInputElement>(null);
  const rowButtons = useRef(new Map<string, HTMLButtonElement>());
  const focusIdRef = useRef<string | null>(null);
  const updatePlugin = useUpdatePlugin();

  const { visible, counts } = useMemo(
    () => visibleInstalledPlugins(plugins, query, filter),
    [plugins, query, filter],
  );
  const selected = plugins.find((plugin) => plugin.id === selectedId) ?? null;
  const clonePluginCounts = useMemo(() => {
    const map = new Map<string, number>();
    for (const plugin of plugins) {
      if (plugin.gitRoot) {
        map.set(plugin.gitRoot, (map.get(plugin.gitRoot) ?? 0) + 1);
      }
    }
    return map;
  }, [plugins]);

  useEffect(() => {
    if (highlightIds.length === 1) {
      focusIdRef.current = highlightIds[0];
      setSelectedId(highlightIds[0]);
    }
  }, [highlightIds]);

  const handleInstallChange = useCallback(
    (pluginId: string, install: PluginInstall | undefined) => {
      setUpdates((prev) => (prev[pluginId] === install ? prev : { ...prev, [pluginId]: install }));
    },
    [],
  );

  function open(id: string) {
    focusIdRef.current = id;
    setSelectedId(id);
  }

  function close() {
    setSelectedId(null);
  }

  function restoreFocus() {
    const id = focusIdRef.current;
    const button = id ? rowButtons.current.get(id) : undefined;
    if (button) button.focus();
    else searchRef.current?.focus();
  }

  async function startUpdate(plugin: Plugin) {
    const started = await updatePlugin.mutateAsync(plugin.id);
    setUpdateIds((prev) => ({ ...prev, [plugin.id]: started.id }));
  }

  return (
    <div className="space-y-3">
      {Object.entries(updateIds).map(([pluginId, installId]) => (
        <PluginInstallPoller
          key={pluginId}
          pluginId={pluginId}
          installId={installId}
          onChange={handleInstallChange}
        />
      ))}
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
        <input
          ref={searchRef}
          type="search"
          aria-label="Search installed plugins"
          placeholder="Search installed plugins"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="field-control min-w-0 flex-1 px-3 py-2 font-body text-sm"
        />
        <div className="flex flex-wrap gap-1.5">
          {FILTERS.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-pressed={filter === item.id}
              onClick={() => setFilter(item.id)}
              className={cn(
                'rounded-full border px-2.5 py-1 font-body text-xs',
                filter === item.id
                  ? 'border-moss-600 bg-moss-500/15 text-text-primary'
                  : 'border-border text-text-secondary hover:text-text-primary',
                item.id === 'attention' && 'text-warning',
              )}
            >
              {item.label} {counts[item.id]}
            </button>
          ))}
        </div>
      </div>
      {visible.length === 0 ? (
        <p className="rounded-xl border border-border bg-surface-raised px-4 py-8 text-center font-body text-sm text-text-muted shadow-card">
          No plugins match.
        </p>
      ) : (
        <ul className="overflow-hidden rounded-xl border border-border bg-surface-raised">
          {visible.map((plugin) => {
            const update = updates[plugin.id];
            const actionError =
              update?.status === 'failed'
                ? `Update failed: ${update.error ?? 'unknown error'}`
                : null;
            return (
              <PluginRow
                key={plugin.id}
                plugin={plugin}
                selected={plugin.id === selectedId}
                highlighted={highlightIds.includes(plugin.id)}
                actionError={actionError}
                onOpen={() => open(plugin.id)}
                onButtonRef={(el) => {
                  if (el) rowButtons.current.set(plugin.id, el);
                  else rowButtons.current.delete(plugin.id);
                }}
              />
            );
          })}
        </ul>
      )}
      <RepoDrawer
        open={selected !== null}
        ariaLabel="Plugin details"
        title={selected?.name ?? ''}
        description={selected?.description || undefined}
        onClose={close}
        onRestoreFocus={restoreFocus}
      >
        {selected && (
          <PluginDetails
            key={selected.id}
            plugin={selected}
            onInstallFromGit={onInstallFromGit}
            siblingCount={
              selected.gitRoot ? clonePluginCounts.get(selected.gitRoot) : undefined
            }
            update={updates[selected.id]}
            updating={
              updatePlugin.isPending ||
              updates[selected.id]?.status === 'running' ||
              (Boolean(updateIds[selected.id]) &&
                updates[selected.id]?.status !== 'failed' &&
                updates[selected.id]?.status !== 'succeeded')
            }
            onUpdate={() => startUpdate(selected)}
          />
        )}
      </RepoDrawer>
    </div>
  );
}
