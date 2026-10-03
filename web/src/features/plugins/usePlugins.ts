import { useEffect } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { apiFetch } from '../../lib/api';
import {
  pluginDirSchema,
  pluginInstallSchema,
  pluginSchema,
  pluginTestResultSchema,
  type InstallPluginInput,
  type Plugin,
  type PluginDir,
  type PluginInstall,
  type PluginTestResult,
} from '../../lib/schemas/plugin';

export const PLUGIN_DIRS_QUERY_KEY = ['plugin-dirs'] as const;
export const PLUGINS_QUERY_KEY = ['plugins'] as const;

export function pluginInstallQueryKey(id: string) {
  return ['plugin-installs', id] as const;
}

const INSTALL_POLL_MS = 2_000;

const pluginDirListSchema = z.array(pluginDirSchema);
const pluginListSchema = z.array(pluginSchema);

const JSON_HEADERS = { 'Content-Type': 'application/json' };

async function fetchPluginDirs(): Promise<PluginDir[]> {
  const res = await apiFetch('/api/plugin-dirs');
  return pluginDirListSchema.parse(await res.json());
}

async function fetchPlugins(): Promise<Plugin[]> {
  const res = await apiFetch('/api/plugins');
  return pluginListSchema.parse(await res.json());
}

async function fetchPluginInstall(id: string): Promise<PluginInstall> {
  const res = await apiFetch(`/api/plugin-installs/${id}`);
  return pluginInstallSchema.parse(await res.json());
}

async function addPluginDir(path: string): Promise<PluginDir> {
  const res = await apiFetch('/api/plugin-dirs', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ path }),
  });
  return pluginDirSchema.parse(await res.json());
}

async function movePluginDir({
  id,
  position,
}: {
  id: string;
  position: number;
}): Promise<PluginDir[]> {
  const res = await apiFetch(`/api/plugin-dirs/${id}`, {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify({ position }),
  });
  return pluginDirListSchema.parse(await res.json());
}

async function removePluginDir(id: string): Promise<void> {
  await apiFetch(`/api/plugin-dirs/${id}`, { method: 'DELETE' });
}

async function rescanPlugins(): Promise<Plugin[]> {
  const res = await apiFetch('/api/plugins/rescan', { method: 'POST' });
  return pluginListSchema.parse(await res.json());
}

async function setPluginEnabled({
  id,
  enabled,
}: {
  id: string;
  enabled: boolean;
}): Promise<Plugin> {
  const res = await apiFetch(`/api/plugins/${id}`, {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify({ enabled }),
  });
  return pluginSchema.parse(await res.json());
}

async function setSkillEnabled(
  pluginId: string,
  skill: string,
  enabled: boolean,
): Promise<Plugin> {
  const res = await apiFetch(
    `/api/plugins/${pluginId}/skills/${encodeURIComponent(skill)}`,
    {
      method: 'PUT',
      headers: JSON_HEADERS,
      body: JSON.stringify({ enabled }),
    },
  );
  return pluginSchema.parse(await res.json());
}

async function setPluginSettings(
  id: string,
  values: Record<string, string>,
): Promise<Plugin> {
  const res = await apiFetch(`/api/plugins/${id}/settings`, {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify({ values }),
  });
  return pluginSchema.parse(await res.json());
}

async function testPlugin(id: string): Promise<PluginTestResult> {
  const res = await apiFetch(`/api/plugins/${id}/test`, { method: 'POST' });
  return pluginTestResultSchema.parse(await res.json());
}

async function installPlugin(body: InstallPluginInput): Promise<PluginInstall> {
  const res = await apiFetch('/api/plugins/install', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
  return pluginInstallSchema.parse(await res.json());
}

async function updatePlugin(id: string): Promise<PluginInstall> {
  const res = await apiFetch(`/api/plugins/${id}/update`, { method: 'POST' });
  return pluginInstallSchema.parse(await res.json());
}

export function usePluginDirs() {
  return useQuery({
    queryKey: PLUGIN_DIRS_QUERY_KEY,
    queryFn: fetchPluginDirs,
  });
}

export function usePlugins() {
  return useQuery({
    queryKey: PLUGINS_QUERY_KEY,
    queryFn: fetchPlugins,
  });
}

/** Polls while the install is running; refreshes plugins once it settles. */
export function usePluginInstall(id: string | null) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: pluginInstallQueryKey(id ?? ''),
    queryFn: () => fetchPluginInstall(id!),
    enabled: Boolean(id),
    refetchInterval: (current) =>
      current.state.data?.status === 'running' ? INSTALL_POLL_MS : false,
  });
  const status = query.data?.status;
  useEffect(() => {
    if (status && status !== 'running') {
      void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
    }
  }, [status, queryClient]);
  return query;
}

function invalidatePluginsAndDirs(queryClient: ReturnType<typeof useQueryClient>) {
  void queryClient.invalidateQueries({ queryKey: PLUGIN_DIRS_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
}

export function useAddPluginDir() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: addPluginDir,
    onSuccess: () => invalidatePluginsAndDirs(queryClient),
  });
}

export function useMovePluginDir() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: movePluginDir,
    onSuccess: (list) => {
      queryClient.setQueryData(PLUGIN_DIRS_QUERY_KEY, list);
      void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
    },
  });
}

export function useRemovePluginDir() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removePluginDir,
    onSuccess: () => invalidatePluginsAndDirs(queryClient),
  });
}

export function useRescanPlugins() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: rescanPlugins,
    onSuccess: (list) => {
      queryClient.setQueryData(PLUGINS_QUERY_KEY, list);
    },
  });
}

export function useSetPluginEnabled() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: setPluginEnabled,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
    },
  });
}

export function useSetSkillEnabled(pluginId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ skill, enabled }: { skill: string; enabled: boolean }) =>
      setSkillEnabled(pluginId, skill, enabled),
    onSuccess: (updated) => {
      queryClient.setQueryData<Plugin[]>(PLUGINS_QUERY_KEY, (list) =>
        list?.map((p) => (p.id === updated.id ? updated : p)),
      );
    },
  });
}

export function useSetPluginSettings(pluginId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (values: Record<string, string>) => setPluginSettings(pluginId, values),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
    },
  });
}

export function useTestPlugin(pluginId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => testPlugin(pluginId),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
    },
  });
}

export function useInstallPlugin() {
  return useMutation({ mutationFn: installPlugin });
}

export function useUpdatePlugin() {
  return useMutation({ mutationFn: updatePlugin });
}
