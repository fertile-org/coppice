import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { apiFetch } from '../../lib/api';

const uncoveredModelsSchema = z.object({
  modelIds: z.array(z.string()),
});

export function uncoveredModelsQueryKey(connectorId: string) {
  return ['uncovered-models', connectorId] as const;
}

function uncoveredModelsPath(connectorId: string): string {
  return `/api/tools/connectors/${encodeURIComponent(connectorId)}/uncovered-models`;
}

export function useUncoveredModels(connectorId: string) {
  return useQuery({
    queryKey: uncoveredModelsQueryKey(connectorId),
    queryFn: async () => {
      const res = await apiFetch(uncoveredModelsPath(connectorId));
      return uncoveredModelsSchema.parse(await res.json()).modelIds;
    },
  });
}

export function useReplaceUncoveredModels(connectorId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (modelIds: string[]) => {
      const res = await apiFetch(uncoveredModelsPath(connectorId), {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ modelIds }),
      });
      return uncoveredModelsSchema.parse(await res.json()).modelIds;
    },
    onSuccess: (modelIds) => {
      queryClient.setQueryData(uncoveredModelsQueryKey(connectorId), modelIds);
    },
  });
}
