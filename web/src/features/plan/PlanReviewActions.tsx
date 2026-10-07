import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';
import { ticketsQueryKey } from '../board/useTickets';
import { commentsQueryKey, ticketQueryKey } from '../tickets/useTicket';
import { PLAN_COPY } from './copy';

async function postPlan(ticketId: string, path: string, body?: string) {
  const res = await apiFetch(`/api/tickets/${ticketId}/${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify({ comment: body }),
  });
  return res.json();
}

export function PlanReviewActions({
  ticketId,
  boardId,
}: {
  ticketId: string;
  boardId: string;
}) {
  const queryClient = useQueryClient();
  const [comment, setComment] = useState('');
  const [error, setError] = useState<string | null>(null);

  function refresh() {
    void queryClient.invalidateQueries({ queryKey: ticketQueryKey(ticketId) });
    void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });
    void queryClient.invalidateQueries({ queryKey: ticketsQueryKey(boardId) });
  }

  const approve = useMutation({
    mutationFn: () => postPlan(ticketId, 'approve-plan'),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (err: unknown) => setError(parseApiErrorMessage(err)),
  });

  const ask = useMutation({
    mutationFn: () => postPlan(ticketId, 'plan-changes', comment.trim()),
    onSuccess: () => {
      setComment('');
      setError(null);
      refresh();
    },
    onError: (err: unknown) => setError(parseApiErrorMessage(err)),
  });

  const busy = approve.isPending || ask.isPending;

  return (
    <section className="space-y-3 border-t border-border pt-6">
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => approve.mutate()}
          disabled={busy}
          className="rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {approve.isPending ? PLAN_COPY.approving : PLAN_COPY.approvePlan}
        </button>
      </div>
      <form
        className="flex flex-col gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (!comment.trim()) return;
          ask.mutate();
        }}
      >
        <textarea
          value={comment}
          onChange={(event) => setComment(event.target.value)}
          placeholder={PLAN_COPY.askPlaceholder}
          aria-label={PLAN_COPY.askPlaceholder}
          disabled={busy}
          rows={3}
          className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary"
        />
        <button
          type="submit"
          disabled={busy || comment.trim().length === 0}
          className="self-start rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-paper-200 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {ask.isPending ? PLAN_COPY.sending : PLAN_COPY.askForChanges}
        </button>
      </form>
      {error && <p className="font-body text-sm text-danger">{error}</p>}
    </section>
  );
}
