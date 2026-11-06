import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { useConfirm } from '../../components/ui/confirm-dialog';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';
import {
  ticketsQueryKey,
  type SplitChildProgress,
  type SplitProgress,
  type Ticket,
} from '../board/useTickets';
import { codeReviewHref } from '../code/codeReviewHref';
import { openCodeReview } from '../code/openCodeReview';
import { TicketStatusBadge } from '../tickets/TicketStatusBadge';
import { commentsQueryKey, ticketQueryKey, useTicketGitInfo } from '../tickets/useTicket';
import { PLAN_COPY } from './copy';

async function postSplit(ticketId: string, action: string): Promise<Ticket> {
  const res = await apiFetch(`/api/tickets/${ticketId}/split/${action}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
  });
  return res.json() as Promise<Ticket>;
}

function reviewChild(progress: SplitProgress): SplitChildProgress | undefined {
  const waiting = progress.children.filter(
    (child) => !child.archived && child.status === 'wait_for_final_review',
  );
  return waiting.find((child) => child.id === progress.activeChildId) ?? waiting[0];
}

/** Stepper and controls for a ticket that runs its sub-tickets in order. */
export function SplitParentTracker({
  ticketId,
  boardId,
  repoId,
  progress,
}: {
  ticketId: string;
  boardId: string;
  repoId?: string;
  progress: SplitProgress;
}) {
  const navigate = useNavigate();
  const confirm = useConfirm();
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const activeId = progress.activeChildId ?? undefined;
  const gitInfo = useTicketGitInfo(activeId ?? ticketId, Boolean(activeId));
  const waiting = reviewChild(progress);

  function refresh(ticket: Ticket) {
    queryClient.setQueryData(ticketQueryKey(ticketId), ticket);
    void queryClient.invalidateQueries({ queryKey: ticketsQueryKey(boardId) });
    void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });
  }

  const control = useMutation({
    mutationFn: (action: string) => postSplit(ticketId, action),
    onSuccess: (ticket) => {
      setError(null);
      refresh(ticket);
    },
    onError: (err: unknown) => setError(parseApiErrorMessage(err)),
  });

  async function markMergedElsewhere() {
    const branch = gitInfo.data?.ticketBranch?.trim() || PLAN_COPY.splitUnknownBranch;
    const pr = gitInfo.data?.prUrl?.trim() || PLAN_COPY.splitNoPullRequest;
    const ok = await confirm({
      title: PLAN_COPY.splitMergedElsewhereTitle,
      description: PLAN_COPY.splitMergedElsewhereBody(branch, pr),
      confirmLabel: PLAN_COPY.splitRecordMerge,
    });
    if (!ok) return;
    control.mutate('merged-elsewhere');
  }

  const reviewHref =
    waiting && repoId ? codeReviewHref(waiting.id, repoId, null) : null;

  return (
    <section className="space-y-3" aria-label={PLAN_COPY.subTickets}>
      {waiting && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-md border border-column-wait-final-border bg-column-wait-final-bg px-3 py-3">
          <p className="font-body text-sm font-medium text-text-primary">
            {PLAN_COPY.yourTurnReview(waiting.position)}
          </p>
          {reviewHref && (
            <a
              href={reviewHref}
              onClick={(event) => {
                event.preventDefault();
                openCodeReview(reviewHref, navigate);
              }}
              className="rounded-md bg-btn-primary px-3 py-1.5 font-body text-sm font-medium text-btn-primary-fg hover:bg-btn-primary-hover"
            >
              {PLAN_COPY.reviewCode}
            </a>
          )}
        </div>
      )}
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="font-display text-base font-semibold tracking-tight text-text-primary">
          {PLAN_COPY.subTickets}
        </h3>
        <p className="font-body text-xs text-text-secondary">
          {PLAN_COPY.mergedCount(progress.mergedCount, progress.count)}
        </p>
      </div>
      <ol className="space-y-2">
        {progress.children.map((child) => (
          <li
            key={child.id}
            className="flex items-center gap-3 rounded-md border border-border bg-surface px-3 py-2"
          >
            <span className="w-5 shrink-0 font-body text-sm font-medium text-text-secondary">
              {child.position}
            </span>
            <span
              className="min-w-0 flex-1 truncate font-body text-sm font-medium text-text-primary"
              title={child.title}
            >
              {child.title}
            </span>
            {child.archived ? (
              <span className="font-body text-xs text-text-muted">Archived</span>
            ) : (
              <TicketStatusBadge status={child.status} />
            )}
          </li>
        ))}
      </ol>
      <p className="font-body text-xs text-text-secondary">{PLAN_COPY.splitParentDoesNotRun}</p>
      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="secondary"
          disabled={control.isPending}
          onClick={() => control.mutate(progress.autoStart ? 'pause' : 'resume')}
        >
          {progress.autoStart ? PLAN_COPY.splitPause : PLAN_COPY.splitResume}
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={control.isPending}
          onClick={() => control.mutate('start-next')}
        >
          {PLAN_COPY.splitStartNext}
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={control.isPending}
          onClick={() => void markMergedElsewhere()}
        >
          {PLAN_COPY.splitMergedElsewhere}
        </Button>
      </div>
      {error && <p className="font-body text-sm text-danger">{error}</p>}
    </section>
  );
}
