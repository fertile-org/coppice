import { useCallback, useEffect, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
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

async function fetchEntries(): Promise<MarketplaceEntry[]> {
  const res = await apiFetch('/api/marketplace/entries');
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

export function useMarketplaceEntries() {
  return useQuery({
    queryKey: MARKETPLACE_ENTRIES_QUERY_KEY,
    queryFn: fetchEntries,
  });
}

export type RefreshMarketplaceOnce = (
  source: MarketplaceSource,
) => Promise<MarketplaceSource>;

/**
 * Refreshes each never-fetched source once per mount. Kept promises deduplicate
 * the request across the invalidation that follows success or failure.
 */
export function useMarketplaceFirstFetch(sources: MarketplaceSource[]) {
  const queryClient = useQueryClient();
  const attempts = useRef(new Map<string, Promise<MarketplaceSource>>());
  const [pendingIds, setPendingIds] = useState<string[]>([]);

  const refreshOnce = useCallback<RefreshMarketplaceOnce>(
    (source) => {
      const existing = attempts.current.get(source.id);
      if (existing) return existing;

      const request = refreshSource(source.id)
        .then((refreshed) => {
          invalidateMarketplace(queryClient);
          return refreshed;
        })
        .catch(async (error: unknown) => {
          await queryClient.invalidateQueries({ queryKey: MARKETPLACE_SOURCES_QUERY_KEY });
          throw error;
        })
        .finally(() => {
          setPendingIds((ids) => ids.filter((id) => id !== source.id));
        });
      attempts.current.set(source.id, request);
      setPendingIds((ids) => [...ids, source.id]);
      return request;
    },
    [queryClient],
  );

  useEffect(() => {
    for (const source of sources) {
      if (source.refreshedAt !== null || attempts.current.has(source.id)) continue;
      void refreshOnce(source).catch(() => undefined);
    }
  }, [sources, refreshOnce]);

  return { refreshOnce, pendingCount: pendingIds.length };
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
    onSuccess: () => invalidateMarketplace(queryClient),
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
