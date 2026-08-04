import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import { ApiError, apiFetch } from '../../lib/api';
import {
  knowledgeItemSchema,
  knowledgePageSchema,
  knowledgeUsageListSchema,
  type KnowledgeConfidence,
  type KnowledgeItem,
  type KnowledgePage,
  type KnowledgeScope,
  type KnowledgeSourceType,
  type KnowledgeStatus,
  type KnowledgeType,
  type KnowledgeUsage,
} from '../../lib/schemas/knowledge';

export const KNOWLEDGE_QUERY_KEY = ['knowledge'] as const;

export interface KnowledgeRevisionInput {
  scope: KnowledgeScope;
  projectId: string | null;
  agentId: string | null;
  knowledgeType: KnowledgeType;
  title: string;
  content: string;
  sourceType: KnowledgeSourceType;
  sourceId: string | null;
  sourceRunId: string | null;
  confidence: KnowledgeConfidence;
}

export interface KnowledgeListFilter {
  status: KnowledgeStatus;
  projectId?: string;
  knowledgeType?: KnowledgeType;
}

async function fetchKnowledgePage(
  filter: KnowledgeListFilter,
  cursor: string | null,
): Promise<KnowledgePage> {
  const params = new URLSearchParams({ status: filter.status, limit: '24' });
  if (filter.projectId) params.set('projectId', filter.projectId);
  if (filter.knowledgeType) params.set('knowledgeType', filter.knowledgeType);
  if (cursor) params.set('cursor', cursor);
  const response = await apiFetch(`/api/knowledge?${params.toString()}`);
  return knowledgePageSchema.parse(await response.json());
}

async function createKnowledge(
  input: KnowledgeRevisionInput,
): Promise<KnowledgeItem> {
  const response = await apiFetch('/api/knowledge', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  return knowledgeItemSchema.parse(await response.json());
}

async function mutateKnowledge(
  itemId: string,
  action: string,
  body: unknown,
): Promise<KnowledgeItem> {
  const response = await apiFetch(`/api/knowledge/${itemId}${action}`, {
    method: action === '' ? 'PATCH' : 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return knowledgeItemSchema.parse(await response.json());
}

export function useKnowledge(filter: KnowledgeListFilter) {
  return useInfiniteQuery({
    queryKey: [...KNOWLEDGE_QUERY_KEY, filter],
    queryFn: ({ pageParam }) => fetchKnowledgePage(filter, pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });
}

function useKnowledgeMutation<TVariables>(
  mutationFn: (variables: TVariables) => Promise<KnowledgeItem>,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: KNOWLEDGE_QUERY_KEY });
    },
    onError: (error) => {
      if (error instanceof ApiError && error.status === 409) {
        void queryClient.invalidateQueries({ queryKey: KNOWLEDGE_QUERY_KEY });
      }
    },
  });
}

export function useCreateKnowledge() {
  return useKnowledgeMutation(createKnowledge);
}

export function useApproveKnowledge() {
  return useKnowledgeMutation(
    ({ id, expectedVersion }: { id: string; expectedVersion: number }) =>
      mutateKnowledge(id, '/approve', { expectedVersion }),
  );
}

export function useRejectKnowledge() {
  return useKnowledgeMutation(
    ({
      id,
      expectedVersion,
      reason,
    }: {
      id: string;
      expectedVersion: number;
      reason: string | null;
    }) => mutateKnowledge(id, '/reject', { expectedVersion, reason }),
  );
}

export function useMarkKnowledgeStale() {
  return useKnowledgeMutation(
    ({ id, expectedVersion }: { id: string; expectedVersion: number }) =>
      mutateKnowledge(id, '/mark-stale', { expectedVersion }),
  );
}

export function useExpireKnowledge() {
  return useKnowledgeMutation(
    ({ id, expectedVersion }: { id: string; expectedVersion: number }) =>
      mutateKnowledge(id, '/expire', {
        expectedVersion,
        expiresAt: new Date().toISOString(),
      }),
  );
}

export function useEditKnowledge() {
  return useKnowledgeMutation(
    ({
      id,
      expectedVersion,
      patch,
    }: {
      id: string;
      expectedVersion: number;
      patch: Partial<
        Pick<
          KnowledgeRevisionInput,
          | 'scope'
          | 'projectId'
          | 'agentId'
          | 'knowledgeType'
          | 'title'
          | 'content'
          | 'confidence'
        >
      >;
    }) => mutateKnowledge(id, '', { expectedVersion, ...patch }),
  );
}

export function useSupersedeKnowledge() {
  return useKnowledgeMutation(
    ({
      id,
      expectedVersion,
      replacement,
    }: {
      id: string;
      expectedVersion: number;
      replacement: KnowledgeRevisionInput;
    }) => mutateKnowledge(id, '/supersede', { expectedVersion, replacement }),
  );
}

export function knowledgeUsageQueryKey(runId: string) {
  return ['agent-run-knowledge-used', runId] as const;
}

async function fetchKnowledgeUsed(runId: string): Promise<KnowledgeUsage[]> {
  const response = await apiFetch(`/api/agent-runs/${runId}/knowledge-used`);
  return knowledgeUsageListSchema.parse(await response.json()).items;
}

export function useKnowledgeUsed(runId: string, enabled: boolean) {
  return useQuery({
    queryKey: knowledgeUsageQueryKey(runId),
    queryFn: () => fetchKnowledgeUsed(runId),
    enabled,
  });
}
