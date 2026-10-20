import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { z } from 'zod';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';

const reviewCapacitySchema = z.object({
  reviewCap: z.number().int().nullable(),
  countInReview: z.boolean(),
  waitingOnReview: z.string().nullable(),
  boardSettingsLabel: z.string(),
  reviewCapLabel: z.string(),
  reviewCapHint: z.string(),
  countInReviewLabel: z.string(),
  reviewCapInvalid: z.string(),
});

export type ReviewCapacity = z.infer<typeof reviewCapacitySchema>;

export interface ReviewCapacityDraft {
  cap: string;
  countInReview: boolean;
}

export interface ReviewCapacityState {
  saved: ReviewCapacity | undefined;
  draft: ReviewCapacityDraft | null;
  setDraft: (draft: ReviewCapacityDraft) => void;
  error: string | null;
  /** Saves `next` when it differs from `saved`. Returns false when the cap is invalid. */
  commit: (next: ReviewCapacityDraft) => boolean;
  isPending: boolean;
}

function queryKey(boardId: string) {
  return ['board-review-capacity', boardId] as const;
}

async function fetchReviewCapacity(boardId: string): Promise<ReviewCapacity> {
  const response = await apiFetch(`/api/boards/${boardId}`);
  return reviewCapacitySchema.parse(await response.json());
}

async function saveReviewCapacity(
  boardId: string,
  body: {
    reviewCap: number | null;
    countInReview: boolean;
  },
): Promise<ReviewCapacity> {
  const response = await apiFetch(`/api/boards/${boardId}/review-capacity`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return reviewCapacitySchema.parse(await response.json());
}

function limitText(value: number | null): string {
  return value == null ? '' : String(value);
}

function parseLimit(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (!trimmed) return null;
  if (!/^\d+$/.test(trimmed)) return undefined;
  const value = Number(trimmed);
  if (!Number.isInteger(value) || value < 1) return undefined;
  return value;
}

/** Board review cap: one draft per board page, shared by the chip, button and drawer. */
export function useReviewCapacity(
  boardId: string | undefined,
): ReviewCapacityState {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: queryKey(boardId ?? ''),
    queryFn: () => fetchReviewCapacity(boardId!),
    enabled: Boolean(boardId),
  });
  const [draft, setDraft] = useState<ReviewCapacityDraft | null>(null);
  const [error, setError] = useState<string | null>(null);

  const saved = query.data;
  if (saved && draft === null) {
    setDraft({
      cap: limitText(saved.reviewCap),
      countInReview: saved.countInReview,
    });
  }

  const save = useMutation({
    mutationFn: (body: { reviewCap: number | null; countInReview: boolean }) =>
      saveReviewCapacity(boardId!, body),
    onSuccess: (next) => {
      queryClient.setQueryData(queryKey(boardId!), next);
      setDraft({
        cap: limitText(next.reviewCap),
        countInReview: next.countInReview,
      });
      setError(null);
    },
    onError: (err: unknown) => {
      setError(parseApiErrorMessage(err));
    },
  });

  function commit(next: ReviewCapacityDraft): boolean {
    if (!saved) return true;
    const reviewCap = parseLimit(next.cap);
    if (reviewCap === undefined) {
      setError(saved.reviewCapInvalid);
      return false;
    }
    if (
      reviewCap === saved.reviewCap &&
      next.countInReview === saved.countInReview
    ) {
      setError(null);
      return true;
    }
    setError(null);
    save.mutate({
      reviewCap,
      countInReview: next.countInReview,
    });
    return true;
  }

  return {
    saved,
    draft,
    setDraft,
    error,
    commit,
    isPending: save.isPending,
  };
}
