import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { z } from 'zod';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';

const reviewCapacitySchema = z.object({
  reviewCap: z.number().int().nullable(),
  countInReview: z.boolean(),
  diffBudget: z.number().int().nullable(),
  waitingOnReview: z.string().nullable(),
  reviewCapLabel: z.string(),
  countInReviewLabel: z.string(),
  diffBudgetLabel: z.string(),
  diffBudgetHint: z.string(),
});

type ReviewCapacity = z.infer<typeof reviewCapacitySchema>;

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
    diffBudget: number | null;
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
  const value = Number(trimmed);
  if (!Number.isInteger(value)) return undefined;
  return value;
}

export function ReviewCapacityBar({ boardId }: { boardId: string }) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: queryKey(boardId),
    queryFn: () => fetchReviewCapacity(boardId),
  });
  const [draft, setDraft] = useState<{
    cap: string;
    budget: string;
    countInReview: boolean;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const saved = query.data;
  if (saved && draft === null) {
    setDraft({
      cap: limitText(saved.reviewCap),
      budget: limitText(saved.diffBudget),
      countInReview: saved.countInReview,
    });
  }

  const save = useMutation({
    mutationFn: (body: {
      reviewCap: number | null;
      countInReview: boolean;
      diffBudget: number | null;
    }) => saveReviewCapacity(boardId, body),
    onSuccess: (next) => {
      queryClient.setQueryData(queryKey(boardId), next);
      setDraft({
        cap: limitText(next.reviewCap),
        budget: limitText(next.diffBudget),
        countInReview: next.countInReview,
      });
      setError(null);
    },
    onError: (err: unknown) => {
      setError(parseApiErrorMessage(err));
    },
  });

  if (!saved || !draft) {
    return null;
  }

  function commit(next: {
    cap: string;
    budget: string;
    countInReview: boolean;
  }) {
    if (!saved) return;
    const reviewCap = parseLimit(next.cap);
    const diffBudget = parseLimit(next.budget);
    if (reviewCap === undefined || diffBudget === undefined) {
      return;
    }
    if (
      reviewCap === saved.reviewCap &&
      diffBudget === saved.diffBudget &&
      next.countInReview === saved.countInReview
    ) {
      return;
    }
    save.mutate({
      reviewCap,
      countInReview: next.countInReview,
      diffBudget,
    });
  }

  const capId = `${boardId}-review-cap`;
  const budgetId = `${boardId}-diff-budget`;

  return (
    <div className="mt-3 flex flex-wrap items-end gap-x-4 gap-y-2">
      {saved.waitingOnReview && (
        <p className="w-full font-body text-sm text-text-secondary">
          {saved.waitingOnReview}
        </p>
      )}
      <div>
        <label
          htmlFor={capId}
          className="mb-1 block font-body text-sm text-text-secondary"
        >
          {saved.reviewCapLabel}
        </label>
        <input
          id={capId}
          type="number"
          min={0}
          step={1}
          value={draft.cap}
          disabled={save.isPending}
          onChange={(event) =>
            setDraft({ ...draft, cap: event.target.value })
          }
          onBlur={() => commit(draft)}
          className="field-control w-24 px-2 py-1 font-body text-sm"
        />
      </div>
      <label className="flex items-center gap-2 pb-1 font-body text-sm text-text-secondary">
        <input
          type="checkbox"
          checked={draft.countInReview}
          disabled={save.isPending}
          onChange={(event) => {
            const next = { ...draft, countInReview: event.target.checked };
            setDraft(next);
            commit(next);
          }}
        />
        {saved.countInReviewLabel}
      </label>
      <div>
        <label
          htmlFor={budgetId}
          className="mb-1 block font-body text-sm text-text-secondary"
        >
          {saved.diffBudgetLabel}
        </label>
        <input
          id={budgetId}
          type="number"
          min={0}
          step={1}
          value={draft.budget}
          disabled={save.isPending}
          onChange={(event) =>
            setDraft({ ...draft, budget: event.target.value })
          }
          onBlur={() => commit(draft)}
          aria-describedby={`${budgetId}-hint`}
          className="field-control w-24 px-2 py-1 font-body text-sm"
        />
        <p
          id={`${budgetId}-hint`}
          className="mt-1 font-body text-xs text-text-muted"
        >
          {saved.diffBudgetHint}
        </p>
      </div>
      {error && (
        <p className="w-full font-body text-sm text-danger" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
