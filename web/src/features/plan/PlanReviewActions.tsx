import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { TicketMarkdown } from '../../components/TicketMarkdown';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';
import { ticketsQueryKey, type PlanQuestions } from '../board/useTickets';
import type { SplitTicketSpec } from '../../lib/schemas/ticket';
import { agentRunsQueryKey } from '../tickets/useAgentRuns';
import { commentsQueryKey, ticketQueryKey } from '../tickets/useTicket';
import { DESIGN_COPY } from '../designs/designCopy';
import { useRegisterShortcut } from '../keyboard/useKeyboard';
import { approveTicketPlan } from './approvePlan';
import { PLAN_COPY } from './copy';

async function postPlan(ticketId: string, path: string, body?: string) {
  const res = await apiFetch(`/api/tickets/${ticketId}/${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify({ comment: body }),
  });
  return res.json();
}

async function postAnswers(ticketId: string, answers: string[]) {
  const res = await apiFetch(`/api/tickets/${ticketId}/plan-answers`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ answers }),
  });
  return res.json();
}

function PlanQuestionsForm({
  ticketId,
  questions,
  onSent,
}: {
  ticketId: string;
  questions: PlanQuestions;
  onSent: () => void;
}) {
  const [answers, setAnswers] = useState<string[]>(() => questions.questions.map(() => ''));
  const [error, setError] = useState<string | null>(null);
  const send = useMutation({
    mutationFn: () => postAnswers(ticketId, answers),
    onSuccess: () => {
      setError(null);
      onSent();
    },
    onError: (err: unknown) => setError(parseApiErrorMessage(err)),
  });
  const hasAnswer = answers.some((answer) => answer.trim().length > 0);

  function setAnswer(index: number, value: string) {
    setAnswers((current) => current.map((answer, i) => (i === index ? value : answer)));
  }

  return (
    <section className="space-y-3 border-t border-border pt-6" aria-label={PLAN_COPY.questionsTitle}>
      <h3 className="font-display text-sm font-semibold text-text-primary">
        {PLAN_COPY.questionsTitle}
      </h3>
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          if (!hasAnswer) return;
          send.mutate();
        }}
      >
        {questions.questions.map((question, index) => {
          const id = `plan-question-${index}`;
          return (
            <div key={id} className="flex flex-col gap-2">
              <label htmlFor={id} className="font-body text-sm text-text-primary">
                {question.question}
              </label>
              {question.choices && question.choices.length > 0 && (
                <div className="flex flex-wrap gap-2">
                  {question.choices.map((choice) => (
                    <button
                      key={choice}
                      type="button"
                      aria-pressed={answers[index] === choice}
                      disabled={send.isPending}
                      onClick={() => setAnswer(index, choice)}
                      className="rounded-md border border-border px-2 py-1 font-body text-xs text-text-primary transition-colors duration-fast hover:bg-paper-200 aria-pressed:bg-paper-200 disabled:opacity-50"
                    >
                      {choice}
                    </button>
                  ))}
                </div>
              )}
              <input
                id={id}
                value={answers[index] ?? ''}
                onChange={(event) => setAnswer(index, event.target.value)}
                placeholder={PLAN_COPY.answerPlaceholder}
                disabled={send.isPending}
                className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary"
              />
            </div>
          );
        })}
        <button
          type="submit"
          disabled={send.isPending || !hasAnswer}
          className="self-start rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {send.isPending ? PLAN_COPY.sending : PLAN_COPY.sendAnswers}
        </button>
      </form>
      {error && <p className="font-body text-sm text-danger">{error}</p>}
    </section>
  );
}

function listLines(text: string | undefined): string[] {
  if (!text) return [];
  return text
    .split('\n')
    .map((line) => line.replace(/^\s*[-*]\s+/, '').trim())
    .filter((line) => line.length > 0);
}

function ProposedSubTickets({ splits }: { splits: SplitTicketSpec[] }) {
  return (
    <div className="mt-4 border-t border-border pt-3">
      <h4 className="font-display text-sm font-semibold text-text-primary">
        {PLAN_COPY.proposedSubTickets}
      </h4>
      <ol aria-label={PLAN_COPY.proposedSubTickets} className="mt-2 space-y-3">
        {splits.map((split, index) => {
          const steps = split.steps ?? [];
          const criteria = listLines(split.acceptanceCriteria);
          return (
            <li key={`${split.title}-${index}`} className="font-body text-sm text-text-primary">
              <p className="font-medium">
                <span className="mr-2 text-text-secondary">{index + 1}</span>
                {split.title}
                {split.assignTo && (
                  <span className="ml-2 rounded-full border border-border px-2 py-0.5 text-xs font-normal text-text-secondary">
                    {split.assignTo}
                  </span>
                )}
              </p>
              <p className="mt-2 text-xs font-medium text-text-secondary">{PLAN_COPY.steps}</p>
              {steps.length > 0 ? (
                <ul className="mt-1 list-disc space-y-1 pl-5">
                  {steps.map((step, stepIndex) => (
                    <li key={`${stepIndex}-${step}`}>{step}</li>
                  ))}
                </ul>
              ) : (
                <p className="mt-1 text-xs text-text-muted">{PLAN_COPY.noSteps}</p>
              )}
              <p className="mt-2 text-xs font-medium text-text-secondary">{PLAN_COPY.criteria}</p>
              {criteria.length > 0 ? (
                <ul className="mt-1 list-disc space-y-1 pl-5">
                  {criteria.map((criterion, criterionIndex) => (
                    <li key={`${criterionIndex}-${criterion}`}>{criterion}</li>
                  ))}
                </ul>
              ) : (
                <p className="mt-1 text-xs text-text-muted">{PLAN_COPY.noCriteria}</p>
              )}
            </li>
          );
        })}
      </ol>
      <p className="mt-3 text-xs text-text-secondary">{PLAN_COPY.proposedSubTicketNote}</p>
    </div>
  );
}

const primaryButton =
  'rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50';
const secondaryButton =
  'rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-paper-200 disabled:cursor-not-allowed disabled:opacity-50';

export function PlanReviewActions({
  ticketId,
  boardId,
  hasPlan = true,
  plan,
  proposedSplits = [],
  revision,
  changeRequests = [],
  planQuestions,
  planRunActive = false,
  designOutOfDate = false,
}: {
  ticketId: string;
  boardId: string;
  /** False while there is no plan yet: Approve and Ask for changes stay hidden. */
  hasPlan?: boolean;
  /** The plan to review, shown above the feedback box. */
  plan?: string | null;
  /** Sub-tickets this approval would create and start, in order. */
  proposedSplits?: SplitTicketSpec[];
  revision?: number;
  changeRequests?: string[];
  planQuestions?: PlanQuestions | null;
  /** A plan run is queued or running: the plan on screen is about to change. */
  planRunActive?: boolean;
  /** The chosen design isn't the one the plan reflects. Approve stays available. */
  designOutOfDate?: boolean;
}) {
  const queryClient = useQueryClient();
  const [comment, setComment] = useState('');
  const [error, setError] = useState<string | null>(null);

  function refresh() {
    void queryClient.invalidateQueries({ queryKey: ticketQueryKey(ticketId) });
    void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });
    void queryClient.invalidateQueries({ queryKey: ticketsQueryKey(boardId) });
    // Approve and plan changes can queue the Tech Lead or the planner.
    void queryClient.invalidateQueries({ queryKey: agentRunsQueryKey(ticketId) });
  }

  const approve = useMutation({
    mutationFn: () => approveTicketPlan(ticketId),
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
  const notePending = comment.trim().length > 0;
  const splitCount = proposedSplits.length;
  const approveLabel = approve.isPending
    ? PLAN_COPY.approving
    : splitCount > 0
      ? PLAN_COPY.approvePlanAndStart(splitCount)
      : PLAN_COPY.approvePlan;

  const waiting = Boolean(planQuestions?.waiting);

  useRegisterShortcut(
    'review.approvePlan',
    () => {
      approve.mutate();
    },
    !busy && hasPlan && !waiting && !planRunActive && !notePending,
  );

  if (waiting && planQuestions) {
    return <PlanQuestionsForm ticketId={ticketId} questions={planQuestions} onSent={refresh} />;
  }
  if (!hasPlan) {
    return null;
  }

  return (
    <section className="space-y-3 border-t border-border pt-6" aria-label={PLAN_COPY.proposedPlan}>
      {(plan?.trim() || splitCount > 0) && (
        <>
          <div className="flex flex-wrap items-baseline gap-2">
            <h3 className="font-display text-base font-semibold tracking-tight text-text-primary">
              {PLAN_COPY.proposedPlan}
            </h3>
            {revision != null && (
              <p className="font-body text-xs text-text-secondary">{PLAN_COPY.revision(revision)}</p>
            )}
          </div>
          <div
            data-testid="pending-plan"
            className="rounded-md border border-accent/40 bg-surface px-4 py-3"
          >
            {plan?.trim() && <TicketMarkdown>{plan}</TicketMarkdown>}
            {splitCount > 0 && <ProposedSubTickets splits={proposedSplits} />}
          </div>
          {changeRequests.length > 0 && (
            <details className="rounded-md border border-border bg-surface px-3 py-2">
              <summary className="cursor-pointer font-body text-sm text-text-primary">
                {PLAN_COPY.requestedChanges}
              </summary>
              <ol className="mt-2 list-decimal space-y-2 pl-5 font-body text-sm text-text-primary">
                {changeRequests.map((request, index) => (
                  <li key={`${index}-${request.slice(0, 24)}`}>{request}</li>
                ))}
              </ol>
            </details>
          )}
          <p className="font-body text-xs text-text-secondary">
            {PLAN_COPY.approveAddsToDescription}
          </p>
        </>
      )}
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
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="submit"
            disabled={busy || !notePending}
            className={notePending ? primaryButton : secondaryButton}
          >
            {ask.isPending ? PLAN_COPY.sending : PLAN_COPY.askForChanges}
          </button>
          {notePending && (
            <p className="font-body text-xs text-text-secondary">
              {PLAN_COPY.approveNeedsEmptyNote}
            </p>
          )}
          <div className="ml-auto flex items-center gap-2">
            {designOutOfDate && (
              <p className="font-body text-xs text-danger">{DESIGN_COPY.planNotUpdated}</p>
            )}
            <button
              type="button"
              onClick={() => approve.mutate()}
              disabled={busy || planRunActive || notePending}
              title={planRunActive ? PLAN_COPY.approveWhileRewriting : undefined}
              className={notePending ? secondaryButton : primaryButton}
            >
              {approveLabel}
            </button>
          </div>
        </div>
      </form>
      {error && <p className="font-body text-sm text-danger">{error}</p>}
    </section>
  );
}
