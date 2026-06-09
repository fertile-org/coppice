import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import type { CreateAgentInput, UpdateAgentInput } from '../../lib/schemas/agent';

export const AGENTS_QUERY_KEY = ['agents'] as const;
export const AGENT_PRESETS_QUERY_KEY = ['agent-presets'] as const;
export const CONNECTORS_QUERY_KEY = ['connectors'] as const;

export interface AgentPreset {
  id: string;
  key: string;
  role: string;
  skills: string[];
  responsibilities: string[];
  systemPromptTemplate: string;
}

export interface Agent {
  id: string;
  name: string;
  role: string;
  skills: string[];
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
}

export interface ConnectorOption {
  id: string;
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

export function useAgents() {
  return useQuery({
    queryKey: AGENTS_QUERY_KEY,
    queryFn: fetchAgents,
    refetchInterval: 30_000,
  });
}

export function useConnectors() {
  return useQuery({
    queryKey: CONNECTORS_QUERY_KEY,
    queryFn: async () => {
      const res = await apiFetch('/api/connectors');
      const data = (await res.json()) as { items: ConnectorOption[] };
      return data.items;
    },
  });
}

export function useModelProviders(connectorId: string | undefined) {
  return useQuery({
    queryKey: ['model-providers', connectorId],
    enabled: !!connectorId && connectorId !== 'mock',
    queryFn: async () => {
      const res = await apiFetch(
        `/api/connectors/${connectorId}/model-providers`,
      );
      const data = (await res.json()) as { items: ModelProviderOption[] };
      return data.items;
    },
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
    queryFn: async () => {
      const res = await apiFetch(
        `/api/connectors/${connectorId}/model-providers/${modelProviderId}/models`,
      );
      const data = (await res.json()) as { items: ModelOption[] };
      return data.items;
    },
  });
}

export function useCreateAgent() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: createAgent,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
    },
  });
}

export function useUpdateAgent(agentId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: UpdateAgentInput) => updateAgent(agentId, body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
    },
  });
}

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
