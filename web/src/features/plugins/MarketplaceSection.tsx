import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { Badge } from '../../components/ui/badge';
import { Button } from '../../components/ui/button';
import { useConfirm } from '../../components/ui/confirm-dialog';
import { Combobox } from '../../components/ui/combobox';
import { parseApiErrorMessage } from '../../lib/api';
import type { MarketplaceEntry, MarketplaceSource } from '../../lib/schemas/marketplace';
import { PLUGIN_COPY } from './copy';
import { MarketplacePluginIcon } from './MarketplacePluginIcon';
import {
  useAddMarketplaceSource,
  useInstallMarketplaceEntry,
  useMarketplaceFirstFetch,
  useMarketplaceJob,
  useRefreshMarketplaceSource,
  useRemoveMarketplaceSource,
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

function isCoppiceSource(source: MarketplaceSource | undefined): boolean {
  return source?.kind === 'builtin';
}

function trustLabel(source: MarketplaceSource | undefined): string {
  return isCoppiceSource(source) ? PLUGIN_COPY.trustCoppice : PLUGIN_COPY.trustCommunity;
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
        <p role="alert" className={ERROR_CLASS}>
          Some sources couldn’t load. Open Manage sources to see the error and retry.
        </p>
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

      <MarketplaceSources sources={sources} />
    </div>
  );
}

function entryKey(entry: MarketplaceEntry): string {
  return `${entry.sourceId}/${entry.name}`;
}

function MarketplaceSources({ sources }: { sources: MarketplaceSource[] }) {
  const [gitUrl, setGitUrl] = useState('');
  const [name, setName] = useState('');
  const [ref, setRef] = useState('');
  const [error, setError] = useState<string | null>(null);
  const addSource = useAddMarketplaceSource();
  const refreshSource = useRefreshMarketplaceSource();
  const removeSource = useRemoveMarketplaceSource();
  const confirm = useConfirm();
  const busy = refreshSource.isPending || removeSource.isPending;

  async function handleAdd(e: FormEvent) {
    e.preventDefault();
    const trimmedName = name.trim();
    const trimmedUrl = gitUrl.trim();
    if (!trimmedName || !trimmedUrl) {
      setError('Name and git URL are required.');
      return;
    }
    setError(null);
    try {
      await addSource.mutateAsync({
        name: trimmedName,
        gitUrl: trimmedUrl,
        ref: ref.trim() || undefined,
      });
      setName('');
      setGitUrl('');
      setRef('');
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to add marketplace.'));
    }
  }

  async function handleRefresh(source: MarketplaceSource) {
    setError(null);
    try {
      await refreshSource.mutateAsync(source.id);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to refresh marketplace.'));
    }
  }

  async function handleRemove(source: MarketplaceSource) {
    if (
      !(await confirm({
        title: `Remove marketplace "${source.name}"?`,
        description: 'Plugins installed from it stay installed, but lose their Update button.',
        danger: true,
      }))
    ) {
      return;
    }
    setError(null);
    try {
      await removeSource.mutateAsync(source.id);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to remove marketplace.'));
    }
  }

  return (
    <section className="space-y-3">
      <div>
        <h3 className="font-display text-base font-semibold text-bark-900">
          {PLUGIN_COPY.sourcesTitle}
        </h3>
        <p className="mt-1 font-body text-sm text-text-secondary">{PLUGIN_COPY.sourcesIntro}</p>
      </div>

      <ol className="overflow-hidden rounded-xl border border-border bg-surface-raised shadow-card">
        {sources.map((source) => {
          const coppice = isCoppiceSource(source);
          return (
            <li
              key={source.id}
              data-testid={`marketplace-source-${source.id}`}
              className="flex flex-wrap items-center gap-3 border-b border-border px-4 py-3 last:border-b-0"
            >
              <span className="font-body text-sm font-medium text-text-primary">{source.name}</span>
              <Badge variant="outline">{trustLabel(source)}</Badge>
              <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-secondary">
                {source.gitUrl ?? ''}
              </span>
              <span className="font-body text-xs text-text-muted">
                {source.lastError
                  ? `Last refresh failed: ${source.lastError}`
                  : source.refreshedAt
                    ? `Refreshed ${new Date(source.refreshedAt).toLocaleString()}`
                    : 'Not fetched yet'}
              </span>
              {!coppice && (
                <div className="flex items-center gap-1">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    aria-label={`Refresh ${source.name}`}
                    disabled={busy}
                    onClick={() => void handleRefresh(source)}
                  >
                    Refresh
                  </Button>
                  <Button
                    type="button"
                    variant="destructive"
                    size="sm"
                    aria-label={`Remove ${source.name}`}
                    disabled={busy}
                    onClick={() => void handleRemove(source)}
                  >
                    Remove
                  </Button>
                </div>
              )}
            </li>
          );
        })}
      </ol>

      <form onSubmit={(e) => void handleAdd(e)} className="grid gap-3 sm:grid-cols-[1fr_2fr_1fr_auto] sm:items-end">
        <div>
          <label
            htmlFor="marketplace-source-name"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Name
          </label>
          <input
            id="marketplace-source-name"
            type="text"
            autoComplete="off"
            placeholder="acme-plugins"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="field-control w-full px-3 py-2 font-body text-sm"
          />
        </div>
        <div>
          <label
            htmlFor="marketplace-source-url"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Git URL
          </label>
          <input
            id="marketplace-source-url"
            type="text"
            autoComplete="off"
            spellCheck={false}
            placeholder="https://github.com/org/plugins.git"
            value={gitUrl}
            onChange={(e) => setGitUrl(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>
        <div>
          <label
            htmlFor="marketplace-source-ref"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Ref <span className="font-normal text-text-muted">(optional)</span>
          </label>
          <input
            id="marketplace-source-ref"
            type="text"
            autoComplete="off"
            spellCheck={false}
            placeholder="main"
            value={ref}
            onChange={(e) => setRef(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>
        <Button type="submit" loading={addSource.isPending}>
          Add marketplace
        </Button>
      </form>

      {error && (
        <p role="alert" className={ERROR_CLASS}>
          {error}
        </p>
      )}
    </section>
  );
}
