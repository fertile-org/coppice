import { useCallback, useEffect, useRef, useState } from 'react';
import {
  keepPreviousData,
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import { z } from 'zod';
import { apiFetch } from '../../lib/api';
import {
  marketplaceEntrySchema,
  marketplaceSourceSchema,
  type MarketplaceEntry,
  type MarketplaceSource,
} from '../../lib/schemas/marketplace';
import {
  pluginInstallSchema,
  pluginSchema,
  type Plugin,
  type PluginInstall,
} from '../../lib/schemas/plugin';
import { PLUGINS_QUERY_KEY, pluginInstallQueryKey } from './usePlugins';

export const MARKETPLACE_SOURCES_QUERY_KEY = ['marketplace-sources'] as const;
export const MARKETPLACE_ENTRIES_QUERY_KEY = ['marketplace-entries'] as const;

const JSON_HEADERS = { 'Content-Type': 'application/json' };
const INSTALL_POLL_MS = 2_000;

const sourceListSchema = z.array(marketplaceSourceSchema);
const entryListSchema = z.array(marketplaceEntrySchema);

/** Path entries finish inline (plugin); object entries start a background job. */
export type MarketplaceStart =
  | { type: 'plugin'; plugin: Plugin }
  | { type: 'job'; install: PluginInstall };

async function parseStart(res: Response): Promise<MarketplaceStart> {
  const body: unknown = await res.json();
  if (res.status === 202) {
    return { type: 'job', install: pluginInstallSchema.parse(body) };
  }
  return { type: 'plugin', plugin: pluginSchema.parse(body) };
}

async function fetchSources(): Promise<MarketplaceSource[]> {
  const res = await apiFetch('/api/marketplace/sources');
  return sourceListSchema.parse(await res.json());
}

async function fetchEntries(sourceId: string): Promise<MarketplaceEntry[]> {
  const res = await apiFetch(`/api/marketplace/entries?source=${encodeURIComponent(sourceId)}`);
  return entryListSchema.parse(await res.json());
}

async function addSource(input: {
  name: string;
  gitUrl: string;
  ref?: string;
}): Promise<MarketplaceSource> {
  const res = await apiFetch('/api/marketplace/sources', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(input),
  });
  return marketplaceSourceSchema.parse(await res.json());
}

async function removeSource(id: string): Promise<void> {
  await apiFetch(`/api/marketplace/sources/${id}`, { method: 'DELETE' });
}

async function refreshSource(id: string): Promise<MarketplaceSource> {
  const res = await apiFetch(`/api/marketplace/sources/${id}/refresh`, {
    method: 'POST',
  });
  return marketplaceSourceSchema.parse(await res.json());
}

async function installEntry(input: {
  sourceId: string;
  entry: string;
}): Promise<MarketplaceStart> {
  const res = await apiFetch('/api/marketplace/install', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(input),
  });
  return parseStart(res);
}

async function updateEntry(pluginId: string): Promise<MarketplaceStart> {
  const res = await apiFetch(`/api/marketplace/plugins/${pluginId}/update`, {
    method: 'POST',
  });
  return parseStart(res);
}

export function useMarketplaceSources() {
  return useQuery({
    queryKey: MARKETPLACE_SOURCES_QUERY_KEY,
    queryFn: fetchSources,
  });
}

export function useMarketplaceEntriesBySource(sourceIds: string[]) {
  return useQueries({
    queries: sourceIds.map((sourceId) => ({
      queryKey: [...MARKETPLACE_ENTRIES_QUERY_KEY, sourceId],
      queryFn: () => fetchEntries(sourceId),
      placeholderData: keepPreviousData,
    })),
    combine: (results) => ({
      entries: results.flatMap((result) => result.data ?? []),
      states: results.map((result) => ({
        isLoading: result.isLoading,
        isError: result.isError,
        count: result.data?.length ?? 0,
      })),
    }),
  });
}

export type RefreshMarketplaceOnce = (
  source: MarketplaceSource,
  options?: { restart?: boolean },
) => Promise<MarketplaceSource>;

const STALE_MS = 24 * 60 * 60 * 1000;

function isStaleSource(source: MarketplaceSource): boolean {
  if (source.refreshedAt === null) return true;
  const at = Date.parse(source.refreshedAt);
  return Number.isNaN(at) || Date.now() - at > STALE_MS;
}

/**
 * Refreshes never-fetched or 24h-stale sources once per mount. `restart` drops
 * a settled attempt so chip and dialog retries issue a new request.
 */
export function useMarketplaceFirstFetch(sources: MarketplaceSource[]) {
  const queryClient = useQueryClient();
  const attempts = useRef(new Map<string, Promise<MarketplaceSource>>());
  const settled = useRef(new Set<string>());
  const [pendingIds, setPendingIds] = useState<string[]>([]);

  const refreshOnce = useCallback<RefreshMarketplaceOnce>(
    (source, options) => {
      if (options?.restart && settled.current.has(source.id)) {
        settled.current.delete(source.id);
        attempts.current.delete(source.id);
      }
      const existing = attempts.current.get(source.id);
      if (existing) return existing;

      const request = refreshSource(source.id)
        .then(async (refreshed) => {
          await queryClient.invalidateQueries({
            queryKey: [...MARKETPLACE_ENTRIES_QUERY_KEY, source.id],
          });
          await queryClient.invalidateQueries({ queryKey: MARKETPLACE_SOURCES_QUERY_KEY });
          return refreshed;
        })
        .catch(async (error: unknown) => {
          await queryClient.invalidateQueries({ queryKey: MARKETPLACE_SOURCES_QUERY_KEY });
          throw error;
        })
        .finally(() => {
          settled.current.add(source.id);
          setPendingIds((ids) => ids.filter((id) => id !== source.id));
        });
      attempts.current.set(source.id, request);
      setPendingIds((ids) => (ids.includes(source.id) ? ids : [...ids, source.id]));
      return request;
    },
    [queryClient],
  );

  useEffect(() => {
    for (const source of sources) {
      if (!isStaleSource(source) || attempts.current.has(source.id)) continue;
      void refreshOnce(source).catch(() => undefined);
    }
  }, [sources, refreshOnce]);

  return { refreshOnce, pendingIds, pendingCount: pendingIds.length };
}

function invalidateMarketplace(queryClient: ReturnType<typeof useQueryClient>) {
  void queryClient.invalidateQueries({ queryKey: MARKETPLACE_ENTRIES_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: MARKETPLACE_SOURCES_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
}

export function useAddMarketplaceSource() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: addSource,
    onSuccess: () => invalidateMarketplace(queryClient),
  });
}

export function useRemoveMarketplaceSource() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeSource,
    onSuccess: () => invalidateMarketplace(queryClient),
  });
}

export function useRefreshMarketplaceSource() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: refreshSource,
    onSuccess: (source) => {
      void queryClient.invalidateQueries({
        queryKey: [...MARKETPLACE_ENTRIES_QUERY_KEY, source.id],
      });
      void queryClient.invalidateQueries({ queryKey: MARKETPLACE_SOURCES_QUERY_KEY });
    },
  });
}

export function useInstallMarketplaceEntry() {
  return useMutation({ mutationFn: installEntry });
}

export function useUpdateMarketplacePlugin() {
  return useMutation({ mutationFn: updateEntry });
}

/** Polls a marketplace job until it settles, then refreshes the marketplace and plugins. */
export function useMarketplaceJob(id: string | null) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: pluginInstallQueryKey(id ?? ''),
    queryFn: async () => {
      const res = await apiFetch(`/api/plugin-installs/${id}`);
      return pluginInstallSchema.parse(await res.json());
    },
    enabled: Boolean(id),
    refetchInterval: (current) =>
      current.state.data?.status === 'running' ? INSTALL_POLL_MS : false,
  });
  const status = query.data?.status;
  useEffect(() => {
    if (status && status !== 'running') {
      invalidateMarketplace(queryClient);
    }
  }, [status, queryClient]);
  return query;
}
