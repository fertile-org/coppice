import { useState } from 'react';

export interface QuestionItem {
  question: string;
  choices?: string[];
}

export interface QuestionsFormCopy {
  title: string;
  answerPlaceholder: string;
  sendAnswers: string;
  sending: string;
}

export function QuestionsForm({
  questions,
  copy,
  onSubmit,
  disabled = false,
  readOnly = false,
  statusLabel,
}: {
  questions: QuestionItem[];
  copy: QuestionsFormCopy;
  onSubmit?: (answers: string[]) => void | Promise<void>;
  disabled?: boolean;
  /** Show the card without inputs (older / settled cards). */
  readOnly?: boolean;
  /** Optional status line under the title when read-only (Answered / Skipped). */
  statusLabel?: string;
}) {
  const [answers, setAnswers] = useState<string[]>(() => questions.map(() => ''));
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const hasAnswer = answers.some((answer) => answer.trim().length > 0);
  const locked = disabled || pending || readOnly;

  function setAnswer(index: number, value: string) {
    setAnswers((current) => current.map((answer, i) => (i === index ? value : answer)));
  }

  if (readOnly) {
    return (
      <section
        className="space-y-2 rounded-lg border border-border bg-paper-50 px-3 py-2"
        aria-label={copy.title}
        data-testid="questions-form"
        data-readonly="true"
      >
        <h3 className="font-display text-sm font-semibold text-text-primary">{copy.title}</h3>
        {statusLabel ? (
          <p className="font-body text-xs text-text-secondary">{statusLabel}</p>
        ) : null}
        <ul className="space-y-1 font-body text-sm text-text-secondary">
          {questions.map((question, index) => (
            <li key={`ro-${index}`}>{question.question}</li>
          ))}
        </ul>
      </section>
    );
  }

  return (
    <section
      className="space-y-3 rounded-lg border border-border bg-paper-50 p-3"
      aria-label={copy.title}
      data-testid="questions-form"
    >
      <h3 className="font-display text-sm font-semibold text-text-primary">{copy.title}</h3>
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          if (!hasAnswer || !onSubmit || locked) return;
          setError(null);
          setPending(true);
          Promise.resolve(onSubmit(answers))
            .catch((err: unknown) => {
              setError(err instanceof Error ? err.message : String(err));
            })
            .finally(() => setPending(false));
        }}
      >
        {questions.map((question, index) => {
          const id = `question-${index}`;
          return (
            <div key={id} className="flex flex-col gap-2">
              <label htmlFor={id} className="font-body text-sm text-text-primary">
                {question.question}
              </label>
              {question.choices && question.choices.length > 0 ? (
                <div className="flex flex-wrap gap-2">
                  {question.choices.map((choice) => (
                    <button
                      key={choice}
                      type="button"
                      aria-pressed={answers[index] === choice}
                      disabled={locked}
                      onClick={() => setAnswer(index, choice)}
                      className="rounded-md border border-border px-2 py-1 font-body text-xs text-text-primary transition-colors duration-fast hover:bg-paper-200 aria-pressed:bg-paper-200 disabled:opacity-50"
                    >
                      {choice}
                    </button>
                  ))}
                </div>
              ) : null}
              <input
                id={id}
                value={answers[index] ?? ''}
                onChange={(event) => setAnswer(index, event.target.value)}
                placeholder={copy.answerPlaceholder}
                disabled={locked}
                className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary"
              />
            </div>
          );
        })}
        <button
          type="submit"
          disabled={locked || !hasAnswer}
          className="self-start rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {pending ? copy.sending : copy.sendAnswers}
        </button>
      </form>
      {error ? <p className="font-body text-sm text-danger">{error}</p> : null}
    </section>
  );
}
