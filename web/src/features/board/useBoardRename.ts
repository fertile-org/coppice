import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { z } from 'zod';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';
import { BOARDS_QUERY_KEY } from '../boards/useBoards';
import {
  reviewCapacityQueryKey,
  type ReviewCapacity,
} from './useReviewCapacity';

const renameResponseSchema = z.object({
  name: z.string(),
});

export interface BoardRenameState {
  draft: string;
  setDraft: (name: string) => void;
  error: string | null;
  isPending: boolean;
  commit: (name: string) => Promise<void>;
}

async function renameBoard(boardId: string, name: string): Promise<string> {
  const response = await apiFetch(`/api/boards/${boardId}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  return renameResponseSchema.parse(await response.json()).name;
}

export function useBoardRename(
  boardId: string,
  savedName: string | undefined,
): BoardRenameState {
  const queryClient = useQueryClient();
  const [draftState, setDraftState] = useState<{
    boardId: string;
    name: string;
  } | null>(null);
  const [errorState, setErrorState] = useState<{
    boardId: string;
    message: string;
  } | null>(null);
  const inFlight = useRef<{ boardId: string; promise: Promise<void> } | null>(
    null,
  );

  const draft =
    draftState?.boardId === boardId ? draftState.name : (savedName ?? '');
  const error =
    errorState?.boardId === boardId ? errorState.message : null;

  const rename = useMutation({
    mutationFn: (name: string) => renameBoard(boardId, name),
    onSuccess: (name) => {
      queryClient.setQueryData(
        reviewCapacityQueryKey(boardId),
        (prev: ReviewCapacity | undefined) =>
          prev ? { ...prev, name } : prev,
      );
      void queryClient.invalidateQueries({ queryKey: BOARDS_QUERY_KEY });
      setDraftState({ boardId, name });
      setErrorState(null);
    },
    onError: (err: unknown) => {
      setErrorState({ boardId, message: parseApiErrorMessage(err) });
    },
  });

  async function commit(name: string): Promise<void> {
    if (inFlight.current?.boardId === boardId) {
      return inFlight.current.promise;
    }
    const trimmed = name.trim();
    if (trimmed === (savedName ?? '')) {
      setErrorState(null);
      return;
    }
    const pending = rename.mutateAsync(trimmed).finally(() => {
      if (inFlight.current?.boardId === boardId) {
        inFlight.current = null;
      }
    });
    inFlight.current = { boardId, promise: pending };
    return pending;
  }

  return {
    draft,
    setDraft: (name: string) => setDraftState({ boardId, name }),
    error,
    isPending: rename.isPending,
    commit,
  };
}
