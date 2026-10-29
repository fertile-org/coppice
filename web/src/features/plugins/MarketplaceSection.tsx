import { useEffect, useMemo, useState } from 'react';
import { Badge } from '../../components/ui/badge';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import { parseApiErrorMessage } from '../../lib/api';
import type { MarketplaceEntry, MarketplaceSource } from '../../lib/schemas/marketplace';
import { PLUGIN_COPY } from './copy';
import { MarketplacePluginIcon } from './MarketplacePluginIcon';
import { MarketplaceSourcesDialog } from './MarketplaceSourcesDialog';
import { trustLabel } from './marketplaceTrust';
import {
  useInstallMarketplaceEntry,
  useMarketplaceFirstFetch,
  useMarketplaceJob,
  useUpdateMarketplacePlugin,
  type MarketplaceStart,
} from './useMarketplace';

const ERROR_CLASS =
  'rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger';

const ALL = 'all';

const PAGE_SIZE = 24;

interface Pending {
  key: string;
  name: string;
  /** Set only for object entries, which install as a background job. */
  jobId: string | null;
  update: boolean;
}

function matches(entry: MarketplaceEntry, query: string): boolean {
  if (!query) return true;
  const needle = query.toLowerCase();
  return (
    entry.name.toLowerCase().includes(needle) ||
    entry.description.toLowerCase().includes(needle)
  );
}

interface MarketplaceSectionProps {
  sources: MarketplaceSource[];
  entries: MarketplaceEntry[];
  /** Called with a notice and the installed plugin ids once a plugin is installed or updated. */
  onDone: (notice: string, pluginIds: string[]) => void;
}

export function MarketplaceSection({ sources, entries, onDone }: MarketplaceSectionProps) {
  const [query, setQuery] = useState('');
  const [sourceId, setSourceId] = useState(ALL);
  const [category, setCategory] = useState(ALL);
  const [limit, setLimit] = useState(PAGE_SIZE);
  const [pending, setPending] = useState<Pending | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const installEntry = useInstallMarketplaceEntry();
  const updatePlugin = useUpdateMarketplacePlugin();
  const job = useMarketplaceJob(pending?.jobId ?? null);
  const firstFetch = useMarketplaceFirstFetch(sources);

  const sourcesById = useMemo(
    () => new Map(sources.map((source) => [source.id, source])),
    [sources],
  );
  const categories = useMemo(
    () =>
      [...new Set(entries.map((entry) => entry.category).filter((c): c is string => Boolean(c)))].sort(),
    [entries],
  );
  const visible = entries.filter(
    (entry) =>
      matches(entry, query.trim()) &&
      (sourceId === ALL || entry.sourceId === sourceId) &&
      (category === ALL || entry.category === category),
  );
  const shown = visible.slice(0, limit);

  function changeQuery(value: string) {
    setQuery(value);
    setLimit(PAGE_SIZE);
  }
  function changeSource(value: string) {
    setSourceId(value);
    setLimit(PAGE_SIZE);
  }
  function changeCategory(value: string) {
    setCategory(value);
    setLimit(PAGE_SIZE);
  }

  const jobStatus = pending?.jobId ? job.data?.status : undefined;
  const jobFailure = jobStatus === 'failed' ? (job.data?.error ?? 'unknown error') : null;
  const jobPluginIds = job.data?.pluginIds;
  const active = pending !== null && jobStatus !== 'failed';
  useEffect(() => {
    if (!pending || jobStatus !== 'succeeded') return;
    onDone(
      pending.update
        ? PLUGIN_COPY.updatedNotice(pending.name)
        : PLUGIN_COPY.installedNotice(pending.name),
      jobPluginIds ?? [],
    );
  }, [jobStatus, pending, onDone, jobPluginIds]);

  async function run(
    key: string,
    name: string,
    update: boolean,
    action: () => Promise<MarketplaceStart>,
  ) {
    setError(null);
    setPending({ key, name, jobId: null, update });
    try {
      const started = await action();
      if (started.type === 'plugin') {
        setPending(null);
        onDone(
          update
            ? PLUGIN_COPY.updatedNotice(name)
            : PLUGIN_COPY.installedNotice(name),
          [started.plugin.id],
        );
      } else {
        setPending({ key, name, jobId: started.install.id, update });
      }
    } catch (err) {
      setPending(null);
      setError(parseApiErrorMessage(err, 'Unable to start install.'));
    }
  }

  function install(entry: MarketplaceEntry) {
    void run(entryKey(entry), entry.name, false, () =>
      installEntry.mutateAsync({ sourceId: entry.sourceId, entry: entry.name }),
    );
  }

  function update(entry: MarketplaceEntry) {
    if (!entry.installedPluginId) return;
    const pluginId = entry.installedPluginId;
    void run(entryKey(entry), entry.name, true, () => updatePlugin.mutateAsync(pluginId));
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-display text-lg font-semibold text-bark-900">
          {PLUGIN_COPY.marketplaceTitle}
        </h2>
        <Button type="button" variant="secondary" onClick={() => setSourcesOpen(true)}>
          Manage sources
        </Button>
      </div>
      <div className="space-y-3">
        <div className="flex flex-wrap gap-3">
          <div className="min-w-[14rem] flex-[2]">
            <label
              htmlFor="marketplace-search"
              className="mb-1 block font-body text-sm font-medium text-bark-800"
            >
              Search
            </label>
            <input
              id="marketplace-search"
              type="search"
              autoComplete="off"
              placeholder="Search plugins"
              value={query}
              onChange={(e) => changeQuery(e.target.value)}
              className="field-control w-full px-3 py-2 font-body text-sm"
            />
          </div>
          <div className="min-w-[10rem] flex-1">
            <span className="mb-1 block font-body text-sm font-medium text-bark-800">
              Source
            </span>
            <Combobox
              aria-label="Source"
              value={sourceId}
              onValueChange={changeSource}
              searchPlaceholder="Search sources…"
              options={[
                { value: ALL, label: 'All sources' },
                ...sources.map((source) => ({ value: source.id, label: source.name })),
              ]}
            />
          </div>
          <div className="min-w-[10rem] flex-1">
            <span className="mb-1 block font-body text-sm font-medium text-bark-800">
              Category
            </span>
            <Combobox
              aria-label="Category"
              value={category}
              onValueChange={changeCategory}
              searchPlaceholder="Search categories…"
              options={[
                { value: ALL, label: 'All categories' },
                ...categories.map((name) => ({ value: name, label: name })),
              ]}
            />
          </div>
        </div>
        <p className="font-body text-sm text-text-secondary">{PLUGIN_COPY.marketplaceIntro}</p>
      </div>

      {pending?.jobId && active && (
        <p className="font-body text-sm text-text-muted">
          {PLUGIN_COPY.installing} {pending.name}
        </p>
      )}
      {(error ?? jobFailure) && (
        <p role="alert" className={ERROR_CLASS}>
          {error ?? `Install failed: ${jobFailure}`}
        </p>
      )}
      {firstFetch.pendingCount > 0 && (
        <p role="status" className="font-body text-sm text-text-muted">
          Loading marketplace sources…
        </p>
      )}
      {sources.some((source) => source.lastError) && (
        <button
          type="button"
          onClick={() => setSourcesOpen(true)}
          className={`${ERROR_CLASS} text-left`}
        >
          Some sources couldn’t load. Open Manage sources to see the error and retry.
        </button>
      )}

      {visible.length === 0 ? (
        <p className="rounded-xl border border-border bg-surface-raised px-4 py-8 text-center font-body text-sm text-text-muted shadow-card">
          {entries.length === 0 ? 'No plugins in the marketplace yet.' : PLUGIN_COPY.marketplaceEmpty}
        </p>
      ) : (
        <div className="space-y-4">
          <ul className="grid gap-3 md:grid-cols-2">
          {shown.map((entry) => {
            const key = entryKey(entry);
            const working = active && pending?.key === key;
            const installed = entry.installedPluginId !== null;
            return (
              <li
                key={key}
                data-testid={`marketplace-entry-${key}`}
                className="flex flex-col gap-3 rounded-xl border border-border bg-surface-raised p-4 shadow-card"
              >
                <div className="flex items-start gap-3">
                  <MarketplacePluginIcon name={entry.name} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <h3 className="truncate font-display text-base font-semibold text-bark-900">
                          {entry.name}
                        </h3>
                        <p className="font-mono text-xs text-text-muted">
                          {entry.version && `v${entry.version} · `}
                          {entry.sourceName}
                        </p>
                      </div>
                      <Badge variant="outline" className="shrink-0">
                        {trustLabel(sourcesById.get(entry.sourceId))}
                      </Badge>
                    </div>
                  </div>
                </div>
                {entry.description && (
                  <p className="font-body text-sm text-text-secondary">{entry.description}</p>
                )}
                <div className="mt-auto flex flex-wrap items-center gap-2">
                  {entry.category && <Badge>{entry.category}</Badge>}
                  {!entry.installable && (
                    <span className="font-body text-sm text-text-muted">
                      {entry.reason ?? PLUGIN_COPY.marketplaceNotInstallable}
                    </span>
                  )}
                  {entry.installable && installed && !entry.updateAvailable && (
                    <span className="font-body text-sm font-medium text-success">
                      {PLUGIN_COPY.installed}
                    </span>
                  )}
                  {entry.installable && installed && entry.updateAvailable && (
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      loading={working}
                      disabled={active && !working}
                      onClick={() => update(entry)}
                    >
                      {PLUGIN_COPY.updateAvailable}
                    </Button>
                  )}
                  {entry.installable && !installed && (
                    <Button
                      type="button"
                      size="sm"
                      loading={working}
                      disabled={active && !working}
                      onClick={() => install(entry)}
                    >
                      Install
                    </Button>
                  )}
                </div>
              </li>
            );
          })}
          </ul>
          {visible.length > limit && (
            <div className="flex justify-center">
              <Button
                type="button"
                variant="secondary"
                onClick={() => setLimit((n) => n + PAGE_SIZE)}
              >
                Load more
              </Button>
            </div>
          )}
        </div>
      )}

      <MarketplaceSourcesDialog
        sources={sources}
        open={sourcesOpen}
        onOpenChange={setSourcesOpen}
        refreshOnce={firstFetch.refreshOnce}
      />
    </div>
  );
}

function entryKey(entry: MarketplaceEntry): string {
  return `${entry.sourceId}/${entry.name}`;
}
