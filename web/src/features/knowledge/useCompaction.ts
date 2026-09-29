import { useEffect, useRef } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  compactionBatchSchema,
  compactionStatusSchema,
  type CompactionBatch,
  type CompactionStatus,
} from '../../lib/schemas/knowledge';
import { KNOWLEDGE_QUERY_KEY } from './useKnowledge';

export const COMPACTION_STATUS_QUERY_KEY = [
  ...KNOWLEDGE_QUERY_KEY,
  'compaction',
] as const;

const ACTIVE_POLL_MS = 5_000;
const IDLE_POLL_MS = 30_000;

async function fetchCompactionStatus(): Promise<CompactionStatus> {
  const response = await apiFetch('/api/knowledge/compaction');
  return compactionStatusSchema.parse(await response.json());
}

async function postCompaction(action: 'run' | 'retry' | 'cancel'): Promise<CompactionBatch> {
  const response = await apiFetch(`/api/knowledge/compaction/${action}`, {
    method: 'POST',
  });
  return compactionBatchSchema.parse(await response.json());
}

/**
 * Compaction status, polled quickly while a batch is active. When a batch
 * finishes, knowledge lists refetch so new Pending candidates appear.
 */
export function useCompactionStatus() {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: COMPACTION_STATUS_QUERY_KEY,
    queryFn: fetchCompactionStatus,
    refetchInterval: (current) =>
      current.state.data?.activeBatch ? ACTIVE_POLL_MS : IDLE_POLL_MS,
  });
  const activeBatchId = query.data?.activeBatch?.id ?? null;
  const previousActive = useRef<string | null>(null);
  useEffect(() => {
    if (previousActive.current && previousActive.current !== activeBatchId) {
      void queryClient.invalidateQueries({
        queryKey: KNOWLEDGE_QUERY_KEY,
        predicate: (candidate) => candidate.queryKey[1] !== 'compaction',
      });
    }
    previousActive.current = activeBatchId;
  }, [activeBatchId, queryClient]);
  return query;
}

function useCompactionMutation(action: 'run' | 'retry' | 'cancel') {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => postCompaction(action),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: COMPACTION_STATUS_QUERY_KEY });
    },
  });
}

export function useCompactNow() {
  return useCompactionMutation('run');
}

export function useRetryCompaction() {
  return useCompactionMutation('retry');
}

export function useCancelCompaction() {
  return useCompactionMutation('cancel');
}
