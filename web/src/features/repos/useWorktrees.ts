import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  removeResponseSchema,
  worktreeListSchema,
  type RemoveResult,
  type WorktreeAdminEntry,
} from '../../lib/schemas/worktree';

export const WORKTREES_QUERY_KEY = ['worktrees'] as const;

async function fetchWorktrees(): Promise<WorktreeAdminEntry[]> {
  const res = await apiFetch('/api/worktrees');
  const body = worktreeListSchema.parse(await res.json());
  return body.worktrees;
}

async function removeWorktrees(folderNames: string[]): Promise<RemoveResult[]> {
  const res = await apiFetch('/api/worktrees/remove', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ folderNames }),
  });
  const body = removeResponseSchema.parse(await res.json());
  return body.results;
}

export function useWorktrees() {
  return useQuery({
    queryKey: WORKTREES_QUERY_KEY,
    queryFn: fetchWorktrees,
    refetchInterval: (query) => {
      const rows = query.state.data;
      if (!rows) return false;
      return rows.some((row) => row.sizeBytes == null) ? 3000 : false;
    },
  });
}

export function useRemoveWorktrees() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeWorktrees,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: WORKTREES_QUERY_KEY });
    },
  });
}
