import { useState } from 'react';
import { useConfirm } from '../../components/ui/confirm-dialog';
import { parseApiErrorMessage } from '../../lib/api';
import type { PluginBuild } from '../../lib/schemas/pluginBuild';
import { LiveRunView } from '../runs/LiveRunView';
import { PLAN_COPY } from '../plan/copy';
import { BUILDER_COPY } from './builderCopy';
import {
  useDiscardPluginBuild,
  useStartPluginBuildRun,
  useStartPluginPlanRun,
  useStopPluginBuildRun,
} from './usePluginBuilds';

const secondaryButton =
  'rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-paper-200 disabled:cursor-not-allowed disabled:opacity-50';
const dangerButton =
  'rounded-md border border-danger-muted px-3 py-1.5 font-body text-sm font-medium text-danger transition-colors duration-fast hover:bg-danger-muted/40 disabled:cursor-not-allowed disabled:opacity-50';

type StepState = 'done' | 'now' | 'todo';

function progressSteps(build: PluginBuild): { label: string; state: StepState }[] {
  const checkFailed = (build.checkErrors?.length ?? 0) > 0;
  const writingDone =
    Boolean(build.pluginId) ||
    Boolean(build.testResult) ||
    checkFailed ||
    (!build.activeRunId && build.status === 'building' && checkFailed);
  // While a build run is active without check/install yet → writing now.
  const writingNow = Boolean(build.activeRunId) && !writingDone && !checkFailed;
  const checkingDone = Boolean(build.pluginId) || Boolean(build.testResult);
  const checkingNow =
    !checkingDone && (checkFailed || (writingDone && build.status === 'building'));
  const installingDone = Boolean(build.pluginId);
  const installingNow = !installingDone && writingDone && !checkFailed && !build.activeRunId;
  const testingDone = Boolean(build.testResult);
  const testingNow = installingDone && !testingDone;

  return [
    { label: BUILDER_COPY.stepPlanApproved, state: 'done' },
    {
      label: BUILDER_COPY.stepWriting,
      state: writingDone ? 'done' : writingNow ? 'now' : 'todo',
    },
    {
      label: BUILDER_COPY.stepChecking,
      state: checkingDone ? 'done' : checkingNow ? 'now' : 'todo',
    },
    {
      label: BUILDER_COPY.stepInstalling,
      state: installingDone ? 'done' : installingNow ? 'now' : 'todo',
    },
    {
      label: BUILDER_COPY.stepTesting,
      state: testingDone ? 'done' : testingNow ? 'now' : 'todo',
    },
  ];
}

function stepGlyph(state: StepState): string {
  if (state === 'done') return '✓';
  if (state === 'now') return '●';
  return '○';
}

export function PluginBuildProgress({
  build,
  onDiscarded,
}: {
  build: PluginBuild;
  onDiscarded: () => void;
}) {
  const confirm = useConfirm();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [comment, setComment] = useState('');
  const [error, setError] = useState<string | null>(null);
  const stopRun = useStopPluginBuildRun();
  const tryAgain = useStartPluginBuildRun(build.id);
  const ask = useStartPluginPlanRun(build.id);
  const discard = useDiscardPluginBuild(build.id);

  const name = build.pluginName?.trim() || 'plugin';
  const steps = progressSteps(build);
  const checkFailed =
    (build.checkErrors?.length ?? 0) > 0 &&
    (build.formatRetries ?? 0) >= 2 &&
    !build.activeRunId;
  const runStopped =
    !build.activeRunId &&
    build.status === 'plan_ready' &&
    (build.lastRunStatus === 'cancelled' || build.lastRunStatus === 'failed');
  const showErrorPanel = checkFailed || runStopped || (build.status === 'building' && !build.activeRunId && (build.checkErrors?.length ?? 0) > 0);

  const busy =
    stopRun.isPending || tryAgain.isPending || ask.isPending || discard.isPending;

  async function handleStop() {
    if (!build.activeRunId) return;
    const ok = await confirm({
      title: BUILDER_COPY.stopBuildTitle,
      confirmLabel: BUILDER_COPY.stopBuildConfirm,
      cancelLabel: BUILDER_COPY.stopBuildKeep,
      danger: true,
    });
    if (!ok) return;
    setError(null);
    try {
      await stopRun.mutateAsync(build.activeRunId);
    } catch (err) {
      setError(parseApiErrorMessage(err));
    }
  }

  async function handleDiscard() {
    const ok = await confirm({
      title: BUILDER_COPY.discardTitle,
      confirmLabel: BUILDER_COPY.discardConfirm,
      cancelLabel: BUILDER_COPY.discardKeep,
      danger: true,
    });
    if (!ok) return;
    setError(null);
    try {
      await discard.mutateAsync();
      onDiscarded();
    } catch (err) {
      setError(parseApiErrorMessage(err));
    }
  }

  return (
    <section className="space-y-4" aria-label={BUILDER_COPY.buildingTitle(name)}>
      <h1 className="font-display text-xl font-semibold text-text-primary">
        {BUILDER_COPY.buildingTitle(name)}
      </h1>

      <ol className="space-y-2 font-body text-sm text-text-primary">
        {steps.map((step) => (
          <li
            key={step.label}
            className={
              step.state === 'now' ? 'motion-safe:animate-pulse font-medium' : undefined
            }
          >
            <span className="mr-2 inline-block w-4 text-center" aria-hidden="true">
              {stepGlyph(step.state)}
            </span>
            {step.label}
          </li>
        ))}
      </ol>

      {build.activeRunId && (
        <details
          open={detailsOpen}
          onToggle={(e) => setDetailsOpen((e.target as HTMLDetailsElement).open)}
          className="rounded-md border border-border bg-surface px-3 py-2"
        >
          <summary className="cursor-pointer font-body text-sm text-text-primary">
            {BUILDER_COPY.showDetails}
          </summary>
          <div className="mt-3 min-h-[12rem]">
            <LiveRunView
              console={undefined}
              runId={build.activeRunId}
              runStatus="running"
              startedAt={null}
            />
          </div>
        </details>
      )}

      {showErrorPanel && (
        <div className="space-y-3 rounded-md border border-danger-muted bg-danger-muted/30 px-3 py-3">
          <p className="font-body text-sm text-danger">
            {checkFailed
              ? BUILDER_COPY.checkFailed
              : runStopped
                ? BUILDER_COPY.buildDidNotFinish
                : BUILDER_COPY.checkFailed}
          </p>
          {build.checkErrors && build.checkErrors.length > 0 && (
            <ul className="list-disc space-y-1 pl-5 font-body text-sm text-text-primary">
              {build.checkErrors.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
          )}
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
              onChange={(e) => setComment(e.target.value)}
              placeholder={PLAN_COPY.askPlaceholder}
              aria-label={PLAN_COPY.askPlaceholder}
              disabled={busy}
              rows={2}
              className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary"
            />
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  setError(null);
                  tryAgain.mutate(undefined, {
                    onError: (err: unknown) => setError(parseApiErrorMessage(err)),
                  });
                }}
                className={secondaryButton}
              >
                {BUILDER_COPY.tryAgain}
              </button>
              <button
                type="submit"
                disabled={busy || !comment.trim()}
                className={secondaryButton}
              >
                {BUILDER_COPY.askForChanges}
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => void handleDiscard()}
                className={dangerButton}
              >
                {BUILDER_COPY.discard}
              </button>
            </div>
          </form>
        </div>
      )}

      {build.activeRunId && (
        <button
          type="button"
          disabled={busy}
          onClick={() => void handleStop()}
          className={dangerButton}
        >
          {stopRun.isPending ? 'Stopping…' : BUILDER_COPY.stopBuild}
        </button>
      )}

      {!showErrorPanel && !build.activeRunId && build.status === 'building' && (
        <p className="font-body text-sm text-text-secondary motion-safe:animate-pulse">
          {BUILDER_COPY.stepWriting}
        </p>
      )}

      {error && <p className="font-body text-sm text-danger">{error}</p>}
    </section>
  );
}
