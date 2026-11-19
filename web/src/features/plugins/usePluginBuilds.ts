import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  parsePluginBuild,
  pluginBuildRunStartSchema,
  type PluginBuild,
  type PluginBuildRunStart,
} from '../../lib/schemas/pluginBuild';

export const PLUGIN_BUILDS_QUERY_KEY = ['plugin-builds'] as const;

export function pluginBuildQueryKey(id: string) {
  return ['plugin-builds', id] as const;
}

const JSON_HEADERS = { 'Content-Type': 'application/json' };
const PLANNING_POLL_MS = 2_000;

async function createPluginBuild(): Promise<PluginBuild> {
  const res = await apiFetch('/api/plugin-builds', { method: 'POST' });
  return parsePluginBuild(await res.json());
}

async function fetchPluginBuild(id: string): Promise<PluginBuild> {
  const res = await apiFetch(`/api/plugin-builds/${id}`);
  return parsePluginBuild(await res.json());
}

export type PluginBuildPatch = {
  answers?: Record<string, unknown>;
  publishTarget?: string | null;
  pluginName?: string | null;
};

async function patchPluginBuild(
  id: string,
  body: PluginBuildPatch,
): Promise<PluginBuild> {
  const res = await apiFetch(`/api/plugin-builds/${id}`, {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
  return parsePluginBuild(await res.json());
}

async function startPlanRun(
  id: string,
  changeRequest?: string,
): Promise<PluginBuildRunStart> {
  const res = await apiFetch(`/api/plugin-builds/${id}/plan-runs`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(changeRequest ? { changeRequest } : {}),
  });
  return pluginBuildRunStartSchema.parse(await res.json());
}

async function startBuildRun(id: string): Promise<PluginBuildRunStart> {
  const res = await apiFetch(`/api/plugin-builds/${id}/build-runs`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({}),
  });
  return pluginBuildRunStartSchema.parse(await res.json());
}

export function useCreatePluginBuild() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createPluginBuild,
    onSuccess: (build) => {
      queryClient.setQueryData(pluginBuildQueryKey(build.id), build);
      void queryClient.invalidateQueries({ queryKey: PLUGIN_BUILDS_QUERY_KEY });
    },
  });
}

export function usePluginBuild(id: string | null, opts?: { pollWhilePlanning?: boolean }) {
  const poll = opts?.pollWhilePlanning ?? true;
  return useQuery({
    queryKey: id ? pluginBuildQueryKey(id) : ['plugin-builds', 'missing'],
    queryFn: () => fetchPluginBuild(id!),
    enabled: Boolean(id),
    refetchInterval: (query) => {
      if (!poll) return false;
      const status = query.state.data?.status;
      return status === 'planning' ? PLANNING_POLL_MS : false;
    },
  });
}

export function usePatchPluginBuild(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: PluginBuildPatch) => patchPluginBuild(id, body),
    onSuccess: (build) => {
      queryClient.setQueryData(pluginBuildQueryKey(build.id), build);
    },
  });
}

export function useStartPluginPlanRun(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (changeRequest?: string) => startPlanRun(id, changeRequest),
    onSuccess: (result) => {
      queryClient.setQueryData(pluginBuildQueryKey(id), result.build);
    },
  });
}

export function useStartPluginBuildRun(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => startBuildRun(id),
    onSuccess: (result) => {
      queryClient.setQueryData(pluginBuildQueryKey(id), result.build);
      void queryClient.invalidateQueries({ queryKey: PLUGIN_BUILDS_QUERY_KEY });
    },
  });
}
