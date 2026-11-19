import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { TicketMarkdown } from '../../components/TicketMarkdown';
import { parseApiErrorMessage } from '../../lib/api';
import type {
  PluginBuild,
  PluginBuildAdvanced,
  PluginBuildPlanQuestion,
} from '../../lib/schemas/pluginBuild';
import { PLAN_COPY } from '../plan/copy';
import { BUILDER_COPY } from './builderCopy';
import {
  usePatchPluginBuild,
  useStartPluginBuildRun,
  useStartPluginPlanRun,
} from './usePluginBuilds';

const primaryButton =
  'rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50';
const secondaryButton =
  'rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-paper-200 disabled:cursor-not-allowed disabled:opacity-50';

function AdvancedDetails({ advanced }: { advanced: PluginBuildAdvanced }) {
  return (
    <details className="rounded-md border border-border bg-surface px-3 py-2">
      <summary className="cursor-pointer font-body text-sm text-text-primary">
        {BUILDER_COPY.advanced}
      </summary>
      <div className="mt-3 space-y-3 font-body text-sm text-text-primary">
        {advanced.pluginName && (
          <p>
            <span className="text-text-secondary">Name · </span>
            {advanced.pluginName}
          </p>
        )}
        {advanced.version && (
          <p>
            <span className="text-text-secondary">Version · </span>
            {advanced.version}
          </p>
        )}
        {advanced.description && (
          <p>
            <span className="text-text-secondary">Description · </span>
            {advanced.description}
          </p>
        )}
        {advanced.files && advanced.files.length > 0 && (
          <div>
            <p className="text-text-secondary">Files</p>
            <ul className="mt-1 list-disc space-y-1 pl-5">
              {advanced.files.map((file) => (
                <li key={file}>
                  <code className="font-mono text-xs">{file}</code>
                </li>
              ))}
            </ul>
          </div>
        )}
        {advanced.skills && advanced.skills.length > 0 && (
          <div>
            <p className="text-text-secondary">Skills</p>
            <ul className="mt-1 list-disc space-y-1 pl-5">
              {advanced.skills.map((skill) => (
                <li key={skill.name}>
                  {skill.name}
                  {skill.description ? ` — ${skill.description}` : ''}
                </li>
              ))}
            </ul>
          </div>
        )}
        {advanced.servers && advanced.servers.length > 0 && (
          <div>
            <p className="text-text-secondary">Servers</p>
            <pre className="mt-1 overflow-x-auto rounded-md bg-paper-200 p-2 font-mono text-xs">
              {JSON.stringify(advanced.servers, null, 2)}
            </pre>
          </div>
        )}
        {advanced.settings && advanced.settings.length > 0 && (
          <div>
            <p className="text-text-secondary">Settings</p>
            <pre className="mt-1 overflow-x-auto rounded-md bg-paper-200 p-2 font-mono text-xs">
              {JSON.stringify(advanced.settings, null, 2)}
            </pre>
          </div>
        )}
        {advanced.permissions && advanced.permissions.length > 0 && (
          <div>
            <p className="text-text-secondary">Permissions</p>
            <ul className="mt-1 list-disc space-y-1 pl-5">
              {advanced.permissions.map((permission) => (
                <li key={permission}>{permission}</li>
              ))}
            </ul>
          </div>
        )}
        {advanced.manifestPreview && (
          <div>
            <p className="text-text-secondary">Manifest preview</p>
            <pre className="mt-1 overflow-x-auto rounded-md bg-paper-200 p-2 font-mono text-xs">
              {advanced.manifestPreview}
            </pre>
          </div>
        )}
      </div>
    </details>
  );
}

function ClarifyingQuestions({
  buildId,
  questions,
  existingAnswers,
}: {
  buildId: string;
  questions: PluginBuildPlanQuestion[];
  existingAnswers: Record<string, unknown>;
}) {
  const [answers, setAnswers] = useState<string[]>(() => questions.map(() => ''));
  const [error, setError] = useState<string | null>(null);
  const patch = usePatchPluginBuild(buildId);
  const planRun = useStartPluginPlanRun(buildId);
  const busy = patch.isPending || planRun.isPending;
  const hasAnswer = answers.some((answer) => answer.trim().length > 0);

  function setAnswer(index: number, value: string) {
    setAnswers((current) => current.map((answer, i) => (i === index ? value : answer)));
  }

  async function submit() {
    if (!hasAnswer) return;
    setError(null);
    try {
      await patch.mutateAsync({
        answers: { ...existingAnswers, clarifying: answers },
      });
      await planRun.mutateAsync(undefined);
    } catch (err) {
      setError(parseApiErrorMessage(err));
    }
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
          void submit();
        }}
      >
        {questions.map((question, index) => {
          const id = `plugin-plan-question-${index}`;
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
                      disabled={busy}
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
                disabled={busy}
                className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary"
              />
            </div>
          );
        })}
        <button
          type="submit"
          disabled={busy || !hasAnswer}
          className="self-start rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy ? PLAN_COPY.sending : PLAN_COPY.sendAnswers}
        </button>
      </form>
      {error && <p className="font-body text-sm text-danger">{error}</p>}
    </section>
  );
}

export function PluginBuildPlanReview({ build }: { build: PluginBuild }) {
  const navigate = useNavigate();
  const [comment, setComment] = useState('');
  const [error, setError] = useState<string | null>(null);
  const ask = useStartPluginPlanRun(build.id);
  const approve = useStartPluginBuildRun(build.id);

  const latest = build.plans.length > 0 ? build.plans[build.plans.length - 1] : null;
  const changeRequests = build.plans
    .map((plan) => plan.changeRequest)
    .filter((request): request is string => Boolean(request?.trim()));
  const questions = build.planQuestions ?? [];
  const waitingOnQuestions = build.status === 'planning' && questions.length > 0;
  const planRunActive = build.status === 'planning' && questions.length === 0;
  const hasPlan = Boolean(latest?.body?.trim()) && build.status === 'plan_ready';

  const busy = ask.isPending || approve.isPending;
  const notePending = comment.trim().length > 0;

  if (waitingOnQuestions) {
    return (
      <ClarifyingQuestions
        buildId={build.id}
        questions={questions}
        existingAnswers={build.answers}
      />
    );
  }

  if (planRunActive || (!hasPlan && build.status === 'planning')) {
    return (
      <p
        role="status"
        className="font-body text-sm text-text-secondary motion-safe:animate-pulse"
      >
        {BUILDER_COPY.writingPlan}
      </p>
    );
  }

  if (!hasPlan || !latest) {
    return null;
  }

  return (
    <section className="space-y-3 border-t border-border pt-6" aria-label={PLAN_COPY.proposedPlan}>
      <div className="flex flex-wrap items-baseline gap-2">
        <h3 className="font-display text-base font-semibold tracking-tight text-text-primary">
          {PLAN_COPY.proposedPlan}
        </h3>
        <p className="font-body text-xs text-text-secondary">
          {PLAN_COPY.revision(latest.revision)}
        </p>
      </div>
      <div
        data-testid="pending-plan"
        className="rounded-md border border-accent/40 bg-surface px-4 py-3"
      >
        <TicketMarkdown>{latest.body}</TicketMarkdown>
      </div>
      {latest.advanced && <AdvancedDetails advanced={latest.advanced} />}
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
      <p className="font-body text-xs text-text-secondary">{BUILDER_COPY.approveStartsBuild}</p>
      <form
        className="flex flex-col gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (!comment.trim()) return;
          setError(null);
          ask.mutate(comment.trim(), {
            onSuccess: () => setComment(''),
            onError: (err: unknown) => setError(parseApiErrorMessage(err)),
          });
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
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => {
                setError(null);
                approve.mutate(undefined, {
                  onSuccess: () => navigate('/settings/plugins', { replace: true }),
                  onError: (err: unknown) => setError(parseApiErrorMessage(err)),
                });
              }}
              disabled={busy || planRunActive || notePending}
              title={planRunActive ? PLAN_COPY.approveWhileRewriting : undefined}
              className={notePending ? secondaryButton : primaryButton}
            >
              {approve.isPending ? PLAN_COPY.approving : PLAN_COPY.approvePlan}
            </button>
          </div>
        </div>
      </form>
      {error && <p className="font-body text-sm text-danger">{error}</p>}
    </section>
  );
}
