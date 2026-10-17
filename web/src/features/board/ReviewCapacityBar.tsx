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

export function ReviewCapacityBar({ boardId }: { boardId: string }) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: queryKey(boardId),
    queryFn: () => fetchReviewCapacity(boardId),
  });
  const [draft, setDraft] = useState<{
    cap: string;
    countInReview: boolean;
  } | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
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
      saveReviewCapacity(boardId, body),
    onSuccess: (next) => {
      queryClient.setQueryData(queryKey(boardId), next);
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

  if (!saved || !draft) {
    return null;
  }

  function commit(next: { cap: string; countInReview: boolean }) {
    if (!saved) return;
    const reviewCap = parseLimit(next.cap);
    if (reviewCap === undefined) {
      setError(saved.reviewCapInvalid);
      return;
    }
    if (
      reviewCap === saved.reviewCap &&
      next.countInReview === saved.countInReview
    ) {
      setError(null);
      return;
    }
    setError(null);
    save.mutate({
      reviewCap,
      countInReview: next.countInReview,
    });
  }

  const capId = `${boardId}-review-cap`;

  return (
    <div className="mt-3">
      {saved.waitingOnReview && (
        <p className="mb-2">
          <span
            data-testid="review-waiting-chip"
            className="inline-flex rounded-full border border-border bg-paper-200 px-2.5 py-0.5 font-body text-xs text-text-secondary"
          >
            {saved.waitingOnReview}
          </span>
        </p>
      )}
      <button
        type="button"
        aria-expanded={settingsOpen}
        onClick={() => setSettingsOpen((open) => !open)}
        className="font-body text-sm text-text-secondary underline-offset-2 hover:underline"
      >
        {saved.boardSettingsLabel}
      </button>
      {settingsOpen && (
        <div className="mt-3 flex flex-wrap items-end gap-x-4 gap-y-2">
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
              min={1}
              step={1}
              value={draft.cap}
              disabled={save.isPending}
              onChange={(event) => setDraft({ ...draft, cap: event.target.value })}
              onBlur={() => commit(draft)}
              aria-describedby={`${capId}-hint`}
              className="field-control w-24 px-2 py-1 font-body text-sm"
            />
            <p
              id={`${capId}-hint`}
              className="mt-1 font-body text-xs text-text-muted"
            >
              {saved.reviewCapHint}
            </p>
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
          {error && (
            <p className="w-full font-body text-sm text-danger" role="alert">
              {error}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
