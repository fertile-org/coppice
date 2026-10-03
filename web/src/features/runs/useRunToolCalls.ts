import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import { runToolCallsSchema, type RunToolCalls } from '../../lib/schemas/toolCall';

export function runToolCallsQueryKey(runId: string) {
  return ['agent-run-tool-calls', runId] as const;
}

async function fetchRunToolCalls(runId: string): Promise<RunToolCalls> {
  const response = await apiFetch(`/api/agent-runs/${runId}/tool-calls`);
  return runToolCallsSchema.parse(await response.json());
}

export function useRunToolCalls(runId: string, enabled: boolean) {
  return useQuery({
    queryKey: runToolCallsQueryKey(runId),
    queryFn: () => fetchRunToolCalls(runId),
    enabled,
  });
}
