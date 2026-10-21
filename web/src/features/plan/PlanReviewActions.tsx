import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { TicketMarkdown } from '../../components/TicketMarkdown';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';
import { ticketsQueryKey, type PlanQuestions } from '../board/useTickets';
import { commentsQueryKey, ticketQueryKey } from '../tickets/useTicket';
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

export function PlanReviewActions({
  ticketId,
  boardId,
  hasPlan = true,
  plan,
  planQuestions,
}: {
  ticketId: string;
  boardId: string;
  /** False while there is no plan yet: Approve and Ask for changes stay hidden. */
  hasPlan?: boolean;
  /** The plan to review, shown right above Approve and Ask for changes. */
  plan?: string | null;
  planQuestions?: PlanQuestions | null;
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

  const waiting = Boolean(planQuestions?.waiting);

  useRegisterShortcut(
    'review.approvePlan',
    () => {
      approve.mutate();
    },
    !busy && hasPlan && !waiting,
  );

  if (waiting && planQuestions) {
    return <PlanQuestionsForm ticketId={ticketId} questions={planQuestions} onSent={refresh} />;
  }
  if (!hasPlan) {
    return null;
  }

  return (
    <section className="space-y-3 border-t border-border pt-6" aria-label={PLAN_COPY.proposedPlan}>
      {plan?.trim() && (
        <>
          <h3 className="font-display text-base font-semibold tracking-tight text-text-primary">
            {PLAN_COPY.proposedPlan}
          </h3>
          <div
            data-testid="pending-plan"
            className="rounded-md border border-accent/40 bg-surface px-4 py-3"
          >
            <TicketMarkdown>{plan}</TicketMarkdown>
          </div>
          <p className="font-body text-xs text-text-secondary">
            {PLAN_COPY.approveAddsToDescription}
          </p>
        </>
      )}
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
