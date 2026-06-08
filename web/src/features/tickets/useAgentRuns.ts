import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  agentRunSchema,
  type AgentRun,
  type RunStatus,
} from '../../lib/schemas/agentRun';
import { commentsQueryKey, ticketQueryKey } from './useTicket';
import { z } from 'zod';

export function agentRunsQueryKey(ticketId: string) {
  return ['agent-runs', ticketId] as const;
}

const runsListSchema = z.object({
  runs: z.array(agentRunSchema),
});

const singleRunSchema = z.object({
  run: agentRunSchema,
});

async function fetchAgentRuns(ticketId: string): Promise<AgentRun[]> {
  const res = await apiFetch(`/api/tickets/${ticketId}/runs`);
  const data = runsListSchema.parse(await res.json());
  return data.runs;
}

async function postRunAgent(ticketId: string): Promise<AgentRun> {
  const res = await apiFetch(`/api/tickets/${ticketId}/run-agent`, {
    method: 'POST',
  });
  const data = singleRunSchema.parse(await res.json());
  return data.run;
}

async function postStopRun(runId: string): Promise<AgentRun> {
  const res = await apiFetch(`/api/agent-runs/${runId}/stop`, {
    method: 'POST',
  });
  const data = singleRunSchema.parse(await res.json());
  return data.run;
}

async function postRetryRun(runId: string): Promise<AgentRun> {
  const res = await apiFetch(`/api/agent-runs/${runId}/retry`, {
    method: 'POST',
  });
  const data = singleRunSchema.parse(await res.json());
  return data.run;
}

export function isActiveRunStatus(status: RunStatus): boolean {
  return status === 'queued' || status === 'running';
}

function invalidateRunRelated(queryClient: QueryClient, ticketId: string) {
  void queryClient.invalidateQueries({ queryKey: agentRunsQueryKey(ticketId) });
  void queryClient.invalidateQueries({ queryKey: ticketQueryKey(ticketId) });
  void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });
}

export function useAgentRuns(ticketId: string | undefined) {
  return useQuery({
    queryKey: agentRunsQueryKey(ticketId ?? ''),
    queryFn: () => fetchAgentRuns(ticketId!),
    enabled: Boolean(ticketId),
    refetchInterval: (query) => {
      const runs = query.state.data;
      if (runs?.some((run) => isActiveRunStatus(run.status))) {
        return 3000;
      }
      return false;
    },
  });
}

export function useRunAgent(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => postRunAgent(ticketId),
    onSuccess: () => invalidateRunRelated(queryClient, ticketId),
  });
}

export function useStopRun(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (runId: string) => postStopRun(runId),
    onSuccess: () => invalidateRunRelated(queryClient, ticketId),
  });
}

export function useRetryRun(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (runId: string) => postRetryRun(runId),
    onSuccess: () => invalidateRunRelated(queryClient, ticketId),
  });
}
