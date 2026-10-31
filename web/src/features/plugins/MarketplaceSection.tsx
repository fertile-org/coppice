import { useEffect, useMemo, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { Badge } from '../../components/ui/badge';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import { cn } from '../../lib/utils';
import { parseApiErrorMessage } from '../../lib/api';
import type { MarketplaceEntry, MarketplaceSource } from '../../lib/schemas/marketplace';
import { PLUGIN_COPY } from './copy';
import { MarketplacePluginIcon } from './MarketplacePluginIcon';
import { MarketplaceSourcesDialog } from './MarketplaceSourcesDialog';
import { trustLabel } from './marketplaceTrust';
import {
  useInstallMarketplaceEntry,
  useMarketplaceEntriesBySource,
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

export function rankMatch(entry: MarketplaceEntry, query: string): number | null {
  if (!query) return 0;
  const needle = query.toLowerCase();
  const name = entry.name.toLowerCase();
  if (name === needle) return 0;
  if (name.startsWith(needle)) return 1;
  if (name.includes(needle)) return 2;
  if (entry.description.toLowerCase().includes(needle)) return 3;
  return null;
}

/** Name-exact, then prefix, then contains, then description. Ties keep input order. */
export function sortMarketplaceEntries(
  entries: MarketplaceEntry[],
  query: string,
): MarketplaceEntry[] {
  const needle = query.trim();
  if (!needle) return entries;
  return entries
    .map((entry, index) => ({ entry, index, rank: rankMatch(entry, needle) }))
    .filter((row): row is { entry: MarketplaceEntry; index: number; rank: number } => row.rank !== null)
    .sort((a, b) => a.rank - b.rank || a.index - b.index)
    .map((row) => row.entry);
}

interface MarketplaceSectionProps {
  sources: MarketplaceSource[];
  sourcesLoading?: boolean;
  /** Called with a notice and the installed plugin ids once a plugin is installed or updated. */
  onDone: (notice: string, pluginIds: string[]) => void;
}

export function MarketplaceSection({
  sources,
  sourcesLoading = false,
  onDone,
}: MarketplaceSectionProps) {
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
  const entryQueries = useMarketplaceEntriesBySource(sources.map((source) => source.id));
  const { entries, states } = entryQueries;

  const sourcesById = useMemo(
    () => new Map(sources.map((source) => [source.id, source])),
    [sources],
  );
  const categories = useMemo(
    () =>
      [...new Set(entries.map((entry) => entry.category).filter((c): c is string => Boolean(c)))].sort(),
    [entries],
  );
  const visible = useMemo(() => {
    const filtered = sortMarketplaceEntries(entries, query).filter(
      (entry) =>
        (sourceId === ALL || entry.sourceId === sourceId) &&
        (category === ALL || entry.category === category),
    );
    return filtered;
  }, [entries, query, sourceId, category]);
  const shown = visible.slice(0, limit);
  const loadingCount = sourcesLoading
    ? Math.max(sources.length, 1)
    : sources.filter((source, index) => {
        const q = states[index];
        return firstFetch.pendingIds.includes(source.id) || Boolean(q?.isLoading);
      }).length;
  const failedSource = sources.find((source, index) => {
    if (firstFetch.pendingIds.includes(source.id)) return false;
    const q = states[index];
    return Boolean(source.lastError) || Boolean(q?.isError);
  });
  const allSettled =
    !sourcesLoading &&
    sources.every((source, index) => {
      const q = states[index];
      return !firstFetch.pendingIds.includes(source.id) && q && !q.isLoading;
    });
  const showSkeletons = !allSettled && loadingCount > 0;

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

  function retrySource(source: MarketplaceSource) {
    void firstFetch.refreshOnce(source, { restart: true }).catch(() => undefined);
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
        <SourceStrip
          sources={sources}
          states={states}
          pendingIds={firstFetch.pendingIds}
          sourcesLoading={sourcesLoading}
          selected={sourceId}
          onSelect={changeSource}
          onRetry={retrySource}
        />
        <p
          className="min-h-[18px] font-body text-sm text-text-secondary"
          role={failedSource || loadingCount > 0 ? 'status' : undefined}
        >
          {failedSource
            ? PLUGIN_COPY.marketplaceSourceFailed(failedSource.name)
            : loadingCount > 0
              ? PLUGIN_COPY.marketplaceFetching(loadingCount)
              : PLUGIN_COPY.marketplaceIntro}
        </p>
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

      {visible.length === 0 && !showSkeletons && allSettled ? (
        <p className="rounded-xl border border-border bg-surface-raised px-4 py-8 text-center font-body text-sm text-text-muted shadow-card">
          {entries.length === 0 ? PLUGIN_COPY.marketplaceNoneYet : PLUGIN_COPY.marketplaceEmpty}
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
          {showSkeletons &&
            Array.from({ length: Math.max(2, 6 - shown.length) }, (_, index) => (
              <li
                key={`skeleton-${index}`}
                data-testid="marketplace-skeleton"
                aria-hidden="true"
                className="flex flex-col gap-3 rounded-xl border border-border bg-surface-raised p-4 shadow-card"
              >
                <div className="h-3 w-[45%] animate-pulse rounded-md bg-border" />
                <div className="h-3 w-[30%] animate-pulse rounded-md bg-border" />
                <div className="h-3 w-full animate-pulse rounded-md bg-border" />
                <div className="h-3 w-[70%] animate-pulse rounded-md bg-border" />
              </li>
            ))}
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

function SourceStrip({
  sources,
  states,
  pendingIds,
  sourcesLoading,
  selected,
  onSelect,
  onRetry,
}: {
  sources: MarketplaceSource[];
  states: ReturnType<typeof useMarketplaceEntriesBySource>['states'];
  pendingIds: string[];
  sourcesLoading: boolean;
  selected: string;
  onSelect: (value: string) => void;
  onRetry: (source: MarketplaceSource) => void;
}) {
  const total = states.reduce((sum, result) => sum + result.count, 0);
  return (
    <div className="flex flex-wrap gap-2" role="group" aria-label="Source">
      <SourceChip
        label="All"
        selected={selected === ALL}
        count={total}
        loading={sourcesLoading || pendingIds.length > 0 || states.some((q) => q.isLoading)}
        onClick={() => onSelect(ALL)}
      />
      {sources.map((source, index) => {
        const q = states[index];
        const loading = pendingIds.includes(source.id) || Boolean(q?.isLoading);
        const failed = !loading && (Boolean(source.lastError) || Boolean(q?.isError));
        return (
          <SourceChip
            key={source.id}
            label={source.name}
            selected={selected === source.id}
            count={q?.count ?? 0}
            loading={loading}
            error={failed}
            onClick={() => onSelect(source.id)}
            onRetry={failed ? () => onRetry(source) : undefined}
          />
        );
      })}
    </div>
  );
}

function SourceChip({
  label,
  selected,
  count,
  loading,
  error,
  onClick,
  onRetry,
}: {
  label: string;
  selected: boolean;
  count: number;
  loading?: boolean;
  error?: boolean;
  onClick: () => void;
  onRetry?: () => void;
}) {
  return (
    <button
      type="button"
      data-testid={`marketplace-source-chip-${label}`}
      aria-pressed={selected}
      onClick={error && onRetry ? onRetry : onClick}
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 font-body text-sm',
        selected && !error && 'border-moss-600 font-medium text-moss-700',
        error && 'border-danger bg-danger-muted text-danger',
        !selected && !error && 'border-border bg-surface-raised text-text-primary',
      )}
    >
      <span>{label}</span>
      {loading ? (
        <Loader2 className="size-3 animate-spin" aria-label={`Loading ${label}`} />
      ) : error ? (
        <span aria-label={`Retry ${label}`}>⚠</span>
      ) : (
        <span className="font-mono text-xs text-text-muted">{count}</span>
      )}
    </button>
  );
}

function entryKey(entry: MarketplaceEntry): string {
  return `${entry.sourceId}/${entry.name}`;
}
