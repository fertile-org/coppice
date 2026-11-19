import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  parsePluginBuild,
  type PluginBuild,
  type PluginBuildRunStart,
} from '../../lib/schemas/pluginBuild';
import { PLUGINS_QUERY_KEY } from './usePlugins';

export const PLUGIN_BUILDS_QUERY_KEY = ['plugin-builds'] as const;

export function pluginBuildQueryKey(id: string) {
  return ['plugin-builds', id] as const;
}

const JSON_HEADERS = { 'Content-Type': 'application/json' };
const ACTIVE_POLL_MS = 2_000;

async function createPluginBuild(): Promise<PluginBuild> {
  const res = await apiFetch('/api/plugin-builds', { method: 'POST' });
  return parsePluginBuild(await res.json());
}

async function fetchPluginBuild(id: string): Promise<PluginBuild> {
  const res = await apiFetch(`/api/plugin-builds/${id}`);
  return parsePluginBuild(await res.json());
}

async function fetchPluginBuilds(): Promise<PluginBuild[]> {
  const res = await apiFetch('/api/plugin-builds');
  const data = (await res.json()) as unknown[];
  return data.map(parsePluginBuild);
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

async function parseRunStart(res: Response): Promise<PluginBuildRunStart> {
  const raw = (await res.json()) as { build: unknown; runId: string };
  return {
    build: parsePluginBuild(raw.build),
    runId: raw.runId,
  };
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
  return parseRunStart(res);
}

async function startBuildRun(id: string): Promise<PluginBuildRunStart> {
  const res = await apiFetch(`/api/plugin-builds/${id}/build-runs`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({}),
  });
  return parseRunStart(res);
}

async function discardPluginBuild(id: string): Promise<PluginBuild> {
  const res = await apiFetch(`/api/plugin-builds/${id}/discard`, { method: 'POST' });
  return parsePluginBuild(await res.json());
}

async function acceptPluginBuild(
  id: string,
  body: { enable?: boolean; publish?: boolean },
): Promise<PluginBuild> {
  const res = await apiFetch(`/api/plugin-builds/${id}/accept`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
  return parsePluginBuild(await res.json());
}

async function stopAgentRun(runId: string): Promise<void> {
  await apiFetch(`/api/agent-runs/${runId}/stop`, { method: 'POST' });
}

function shouldPollBuild(status: string | undefined): boolean {
  return status === 'planning' || status === 'building';
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

export function usePluginBuilds(opts?: { poll?: boolean }) {
  const poll = opts?.poll ?? true;
  return useQuery({
    queryKey: PLUGIN_BUILDS_QUERY_KEY,
    queryFn: fetchPluginBuilds,
    refetchInterval: (query) => {
      if (!poll) return false;
      const builds = query.state.data;
      return builds?.some((b) => shouldPollBuild(b.status) || Boolean(b.activeRunId))
        ? ACTIVE_POLL_MS
        : false;
    },
  });
}

export function usePluginBuild(id: string | null, opts?: { pollWhileActive?: boolean }) {
  const poll = opts?.pollWhileActive ?? true;
  return useQuery({
    queryKey: id ? pluginBuildQueryKey(id) : ['plugin-builds', 'missing'],
    queryFn: () => fetchPluginBuild(id!),
    enabled: Boolean(id),
    refetchInterval: (query) => {
      if (!poll) return false;
      const data = query.state.data;
      return shouldPollBuild(data?.status) || Boolean(data?.activeRunId)
        ? ACTIVE_POLL_MS
        : false;
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
      void queryClient.invalidateQueries({ queryKey: PLUGIN_BUILDS_QUERY_KEY });
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

export function useDiscardPluginBuild(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => discardPluginBuild(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLUGIN_BUILDS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
      queryClient.removeQueries({ queryKey: pluginBuildQueryKey(id) });
    },
  });
}

export function useAcceptPluginBuild(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: { enable?: boolean; publish?: boolean }) =>
      acceptPluginBuild(id, body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLUGIN_BUILDS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: PLUGINS_QUERY_KEY });
      queryClient.removeQueries({ queryKey: pluginBuildQueryKey(id) });
    },
  });
}

export function useStopPluginBuildRun() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (runId: string) => stopAgentRun(runId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLUGIN_BUILDS_QUERY_KEY });
    },
  });
}
