import { useRef, useState } from 'react';
import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  connectorModelsPath,
  ModelsRequestError,
  modelsRequestError,
} from './modelList';
import type { CreateAgentInput, UpdateAgentInput } from '../../lib/schemas/agent';
import { connectorListSchema, type Connector } from '../../lib/schemas/connector';
import { TOOL_CONNECTORS_QUERY_KEY } from '../tools/useConnectorDiagnostics';

export const AGENTS_QUERY_KEY = ['agents'] as const;
export const AGENT_PRESETS_QUERY_KEY = ['agent-presets'] as const;
export const CONNECTORS_QUERY_KEY = ['connectors'] as const;

/** Refresh models must ignore the shared 30s staleTime and hit the server. */
export const MODELS_REFRESH_STALE_TIME = 0;

export interface AgentPreset {
  id: string;
  key: string;
  role: string;
  responsibilities: string[];
  systemPromptTemplate: string;
}

export interface Agent {
  id: string;
  name: string;
  role: string;
  responsibilities: string[];
  systemPrompt: string;
  connector: string;
  modelProvider?: string | null;
  model?: string | null;
  health: 'unknown' | 'healthy' | 'missing_config' | 'unhealthy';
  healthDetail?: string | null;
  enabled: boolean;
  presetSource?: string;
  createdAt: string;
  updatedAt: string;
  connectorTurnedOn?: { id: string; displayName: string } | null;
}

export interface ConnectorOption {
  id: string;
  displayName?: string;
  readiness?: 'ready' | 'found_not_signed_in' | 'not_on_path' | 'found' | null;
}

export interface ModelProviderOption {
  id: string;
}

export interface ModelOption {
  id: string;
  name: string;
}

export type AgentSummary = Pick<Agent, 'id' | 'name' | 'role' | 'enabled'>;

async function fetchPresets(): Promise<AgentPreset[]> {
  const res = await apiFetch('/api/agent-presets');
  const data = (await res.json()) as { items: AgentPreset[] };
  return data.items;
}

async function fetchAgents(): Promise<Agent[]> {
  const res = await apiFetch('/api/agents');
  const data = (await res.json()) as { items: Agent[] };
  return data.items;
}

async function createAgent(body: CreateAgentInput): Promise<Agent> {
  const res = await apiFetch('/api/agents', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<Agent>;
}

async function updateAgent(
  agentId: string,
  body: UpdateAgentInput,
): Promise<Agent> {
  const res = await apiFetch(`/api/agents/${agentId}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<Agent>;
}

export function useAgentPresets() {
  return useQuery({
    queryKey: AGENT_PRESETS_QUERY_KEY,
    queryFn: fetchPresets,
  });
}

export function useAgents(options?: { enabled?: boolean }) {
  const enabled = options?.enabled ?? true;
  return useQuery({
    queryKey: AGENTS_QUERY_KEY,
    queryFn: fetchAgents,
    enabled,
    refetchInterval: enabled ? 30_000 : false,
  });
}

export async function fetchConnectors(): Promise<Connector[]> {
  const res = await apiFetch('/api/connectors');
  return connectorListSchema.parse(await res.json()).items;
}

export function useConnectors(options?: { enabled?: boolean }) {
  const enabled = options?.enabled ?? true;
  return useQuery({
    queryKey: CONNECTORS_QUERY_KEY,
    queryFn: fetchConnectors,
    enabled,
    refetchInterval: enabled
      ? (query) =>
          query.state.data?.some((item) => item.id !== 'mock' && item.readiness == null)
            ? 2_000
            : false
      : false,
  });
}

function invalidateIfConnectorTurnedOn(
  queryClient: ReturnType<typeof useQueryClient>,
  agent: Agent,
) {
  if (!agent.connectorTurnedOn) return;
  void queryClient.invalidateQueries({ queryKey: CONNECTORS_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: TOOL_CONNECTORS_QUERY_KEY });
}

async function fetchModelProviders(
  connectorId: string,
  refresh = false,
): Promise<ModelProviderOption[]> {
  const path = `/api/connectors/${encodeURIComponent(connectorId)}/model-providers`;
  const res = await apiFetch(refresh ? `${path}?refresh=true` : path);
  const data = (await res.json()) as { items: ModelProviderOption[] };
  return data.items;
}

export function useModelProviders(connectorId: string | undefined) {
  return useQuery({
    queryKey: ['model-providers', connectorId],
    enabled: !!connectorId && connectorId !== 'mock',
    queryFn: () => fetchModelProviders(connectorId!),
  });
}

export function useModels(
  connectorId: string | undefined,
  modelProviderId: string | undefined,
) {
  return useQuery({
    queryKey: ['models', connectorId, modelProviderId],
    enabled:
      !!connectorId && !!modelProviderId && connectorId !== 'mock',
    queryFn: () => fetchProviderModels(connectorId!, modelProviderId!, false),
  });
}

export interface ModelGroup {
  provider: string;
  models: ModelOption[];
}

async function fetchProviderModels(
  connectorId: string,
  providerId: string,
  refresh: boolean,
): Promise<ModelOption[]> {
  try {
    const res = await apiFetch(connectorModelsPath(connectorId, providerId, refresh));
    const data = (await res.json()) as { items?: ModelOption[] };
    if (!Array.isArray(data.items) || data.items.length === 0) {
      throw new ModelsRequestError('empty model list', null);
    }
    return data.items;
  } catch (err) {
    throw modelsRequestError(err);
  }
}

function modelsQueryKey(connectorId: string, providerId: string) {
  return ['models', connectorId, providerId] as const;
}

/**
 * Models for every provider of the connector, in provider order.
 * Cursor's provider id is `cursor`, so the request is
 * `/api/connectors/cursor/model-providers/cursor/models`.
 */
export function useConnectorModels(connectorId: string | undefined) {
  const queryClient = useQueryClient();
  const providers = useModelProviders(connectorId);
  const providerIds = (providers.data ?? []).map((provider) => provider.id);
  const enabled = !!connectorId && connectorId !== 'mock' && providers.isSuccess;
  const queries = useQueries({
    queries: providerIds.map((providerId) => ({
      queryKey: modelsQueryKey(connectorId ?? '', providerId),
      enabled,
      queryFn: () => fetchProviderModels(connectorId!, providerId, false),
    })),
  });

  const groups: ModelGroup[] = providerIds.map((provider, index) => ({
    provider,
    models: queries[index]?.data ?? [],
  }));
  const [refreshing, setRefreshing] = useState(false);
  const refreshInFlight = useRef(false);

  const refresh = async () => {
    if (!connectorId || connectorId === 'mock' || refreshInFlight.current) return;
    refreshInFlight.current = true;
    setRefreshing(true);
    try {
      const next = await fetchModelProviders(connectorId, true);
      const shown = queryClient.getQueryData<unknown[]>(['model-providers', connectorId]) ?? [];
      if (next.length === 0 && shown.length > 0) {
        // An empty discovery is more likely a hiccup than every provider
        // being gone; keep the list already on screen.
        return;
      }
      await Promise.all(
        next.map((provider) =>
          queryClient
            .fetchQuery({
              queryKey: modelsQueryKey(connectorId, provider.id),
              queryFn: () => fetchProviderModels(connectorId, provider.id, true),
              staleTime: MODELS_REFRESH_STALE_TIME,
            })
            .catch(() => undefined),
        ),
      );
      queryClient.setQueryData(['model-providers', connectorId], next);
    } catch {
      // A failed provider list leaves the list already on screen.
    } finally {
      refreshInFlight.current = false;
      setRefreshing(false);
    }
  };

  const providersEmpty =
    !!connectorId &&
    connectorId !== 'mock' &&
    providers.isSuccess &&
    providerIds.length === 0;
  const isLoading =
    (!!connectorId && connectorId !== 'mock' && providers.isLoading) ||
    queries.some((query) => query.isLoading);
  const isFetching = refreshing || queries.some((query) => query.isFetching);
  const isError =
    providersEmpty || providers.isError || queries.some((query) => query.isError);
  const detail =
    queries
      .map((query) => query.error)
      .find((error): error is ModelsRequestError => error instanceof ModelsRequestError)?.detail ??
    null;
  const errorMessage = [providers.error, ...queries.map((query) => query.error)]
    .map((error) => (error ? modelsRequestError(error).message.trim() : ''))
    .find((message) => message.length > 0) ?? null;

  return { groups, isLoading, isFetching, isError, detail, errorMessage, refresh };
}

export function useCreateAgent() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: createAgent,
    onSuccess: (agent) => {
      void queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
      invalidateIfConnectorTurnedOn(queryClient, agent);
    },
  });
}

export function useUpdateAgent(agentId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: UpdateAgentInput) => updateAgent(agentId, body),
    onSuccess: (agent) => {
      void queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
      invalidateIfConnectorTurnedOn(queryClient, agent);
    },
  });
}

export function agentPluginsQueryKey(agentId: string) {
  return ['agents', agentId, 'plugins'] as const;
}

async function fetchAgentPlugins(agentId: string): Promise<string[]> {
  const res = await apiFetch(`/api/agents/${agentId}/plugins`);
  const data = (await res.json()) as { pluginIds: string[] };
  return data.pluginIds;
}

async function setAgentPlugins({
  agentId,
  pluginIds,
}: {
  agentId: string;
  pluginIds: string[];
}): Promise<string[]> {
  const res = await apiFetch(`/api/agents/${agentId}/plugins`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ pluginIds }),
  });
  const data = (await res.json()) as { pluginIds: string[] };
  return data.pluginIds;
}

export function useAgentPlugins(agentId: string | undefined) {
  return useQuery({
    queryKey: agentPluginsQueryKey(agentId ?? ''),
    queryFn: () => fetchAgentPlugins(agentId!),
    enabled: Boolean(agentId),
  });
}

export function useSetAgentPlugins() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: setAgentPlugins,
    onSuccess: (pluginIds, { agentId }) => {
      queryClient.setQueryData(agentPluginsQueryKey(agentId), pluginIds);
    },
  });
}

export { fetchAgentPlugins };

export function useUpdateAgentMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      agentId,
      body,
    }: {
      agentId: string;
      body: UpdateAgentInput;
    }) => updateAgent(agentId, body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
    },
  });
}
