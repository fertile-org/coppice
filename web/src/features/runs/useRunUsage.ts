import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import { runUsageDetailSchema, type RunUsageDetail } from '../../lib/schemas/usage';

export function runUsageQueryKey(runId: string) {
  return ['agent-run-usage', runId] as const;
}

async function fetchRunUsage(runId: string): Promise<RunUsageDetail> {
  const response = await apiFetch(`/api/agent-runs/${runId}/usage`);
  return runUsageDetailSchema.parse(await response.json());
}

export function useRunUsage(runId: string, enabled: boolean, refetchInterval: number | false = false) {
  return useQuery({
    queryKey: runUsageQueryKey(runId),
    queryFn: () => fetchRunUsage(runId),
    enabled,
    refetchInterval,
  });
}
