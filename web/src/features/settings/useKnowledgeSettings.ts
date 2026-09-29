import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  knowledgeSettingsSchema,
  type KnowledgeSettings,
} from '../../lib/schemas/settings';
import { COMPACTION_STATUS_QUERY_KEY } from '../knowledge/useCompaction';

export const KNOWLEDGE_SETTINGS_QUERY_KEY = ['settings', 'knowledge'] as const;

async function fetchKnowledgeSettings(): Promise<KnowledgeSettings> {
  const response = await apiFetch('/api/settings/knowledge');
  return knowledgeSettingsSchema.parse(await response.json());
}

async function putKnowledgeSettings(
  compactionAgentId: string | null,
): Promise<KnowledgeSettings> {
  const response = await apiFetch('/api/settings/knowledge', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ compactionAgentId }),
  });
  return knowledgeSettingsSchema.parse(await response.json());
}

export function useKnowledgeSettings() {
  return useQuery({
    queryKey: KNOWLEDGE_SETTINGS_QUERY_KEY,
    queryFn: fetchKnowledgeSettings,
  });
}

export function useUpdateKnowledgeSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: putKnowledgeSettings,
    onSuccess: (settings) => {
      queryClient.setQueryData(KNOWLEDGE_SETTINGS_QUERY_KEY, settings);
      void queryClient.invalidateQueries({ queryKey: COMPACTION_STATUS_QUERY_KEY });
    },
  });
}
