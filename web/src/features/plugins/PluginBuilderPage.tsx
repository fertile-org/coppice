import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';
import { useConfirm } from '../../components/ui/confirm-dialog';
import { parseApiErrorMessage } from '../../lib/api';
import { isDesktopShell } from '../../lib/desktop';
import { useSession } from '../auth/useSession';
import { PLAN_COPY } from '../plan/copy';
import { BUILDER_COPY, TOOL_CHIP_VALUES, type ToolChipValue } from './builderCopy';
import { PluginBuildPlanReview } from './PluginBuildPlanReview';
import { PluginBuildProgress } from './PluginBuildProgress';
import { PluginBuildTryView } from './PluginBuildTryView';
import {
  useCreatePluginBuild,
  usePatchPluginBuild,
  usePluginBuild,
  useStartPluginPlanRun,
} from './usePluginBuilds';
import { missingSettingsNotice } from './copy';
import { usePlugins } from './usePlugins';

const TOTAL_STEPS = 5;

type SecretsChoice = 'none' | 'internet' | 'key';
type WhereChoice = 'local' | 'git';

export type WizardAnswers = {
  purpose: string;
  jobsText: string;
  tools: string[];
  toolsExtra: string;
  secrets: SecretsChoice | null;
  secretService: string;
  where: WhereChoice | null;
  publishTarget: string;
};

const EMPTY_ANSWERS: WizardAnswers = {
  purpose: '',
  jobsText: '',
  tools: [],
  toolsExtra: '',
  secrets: null,
  secretService: '',
  where: 'local',
  publishTarget: '',
};

function jobsFromText(text: string): string[] {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function toolsPayload(answers: WizardAnswers): string[] {
  if (answers.tools.includes(TOOL_CHIP_VALUES.none)) {
    return [TOOL_CHIP_VALUES.none];
  }
  const extra = answers.toolsExtra
    .split(/[\n,]/)
    .map((part) => part.trim())
    .filter(Boolean);
  return [...answers.tools, ...extra];
}

function toApiAnswers(answers: WizardAnswers): Record<string, unknown> {
  const body: Record<string, unknown> = {
    purpose: answers.purpose.trim(),
    jobs: jobsFromText(answers.jobsText),
    tools: toolsPayload(answers),
    secrets: answers.secrets ?? 'none',
    where: answers.where ?? 'local',
  };
  if (answers.secrets && answers.secrets !== 'none' && answers.secretService.trim()) {
    body.secretService = answers.secretService.trim();
  }
  return body;
}

function fromApiAnswers(
  raw: Record<string, unknown>,
  publishTarget: string | null,
): WizardAnswers {
  const toolsRaw = Array.isArray(raw.tools)
    ? raw.tools.filter((t): t is string => typeof t === 'string')
    : [];
  const known = new Set<string>(Object.values(TOOL_CHIP_VALUES));
  const chips = toolsRaw.filter((t) => known.has(t));
  const extra = toolsRaw.filter((t) => !known.has(t));
  const secrets =
    raw.secrets === 'none' || raw.secrets === 'internet' || raw.secrets === 'key'
      ? raw.secrets
      : null;
  const where = raw.where === 'git' || raw.where === 'local' ? raw.where : 'local';
  const jobs = Array.isArray(raw.jobs)
    ? raw.jobs.filter((j): j is string => typeof j === 'string')
    : [];
  return {
    purpose: typeof raw.purpose === 'string' ? raw.purpose : '',
    jobsText: jobs.join('\n'),
    tools: chips.includes(TOOL_CHIP_VALUES.none) ? [TOOL_CHIP_VALUES.none] : chips,
    toolsExtra: extra.join(', '),
    secrets,
    secretService: typeof raw.secretService === 'string' ? raw.secretService : '',
    where,
    publishTarget: publishTarget ?? '',
  };
}

function stepValid(step: number, answers: WizardAnswers): boolean {
  switch (step) {
    case 1:
      return answers.purpose.trim().length > 0;
    case 2:
      return jobsFromText(answers.jobsText).length > 0;
    case 3:
      return toolsPayload(answers).length > 0;
    case 4:
      return answers.secrets != null;
    case 5:
      if (answers.where == null) return false;
      if (answers.where === 'git') return answers.publishTarget.trim().length > 0;
      return true;
    default:
      return false;
  }
}

function hasAnyAnswer(answers: WizardAnswers): boolean {
  return (
    answers.purpose.trim().length > 0 ||
    jobsFromText(answers.jobsText).length > 0 ||
    toolsPayload(answers).length > 0 ||
    (answers.secrets != null && answers.secrets !== 'none') ||
    answers.secretService.trim().length > 0 ||
    answers.where === 'git' ||
    answers.publishTarget.trim().length > 0
  );
}

const chipClass =
  'rounded-md border border-border px-2 py-1 font-body text-xs text-text-primary transition-colors duration-fast hover:bg-paper-200 aria-pressed:bg-paper-200 disabled:opacity-50';
const fieldClass =
  'w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary';
const primaryButton =
  'rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50';
const ghostButton =
  'rounded-md px-3 py-1.5 font-body text-sm font-medium text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:opacity-50';

function toggleTool(current: string[], value: ToolChipValue): string[] {
  if (value === TOOL_CHIP_VALUES.none) {
    return current.includes(TOOL_CHIP_VALUES.none) ? [] : [TOOL_CHIP_VALUES.none];
  }
  const withoutNone = current.filter((t) => t !== TOOL_CHIP_VALUES.none);
  return withoutNone.includes(value)
    ? withoutNone.filter((t) => t !== value)
    : [...withoutNone, value];
}

export function PluginBuilderPage() {
  const { user, loading } = useSession();
  const [desktop] = useState(isDesktopShell);
  const navigate = useNavigate();
  const confirm = useConfirm();
  const [searchParams, setSearchParams] = useSearchParams();
  const buildId = searchParams.get('id');
  const createBuild = useCreatePluginBuild();
  const buildQuery = usePluginBuild(buildId);
  const patch = usePatchPluginBuild(buildId ?? '');
  const planRun = useStartPluginPlanRun(buildId ?? '');

  const [step, setStep] = useState(1);
  const [answers, setAnswers] = useState<WizardAnswers>(EMPTY_ANSWERS);
  const [hydrated, setHydrated] = useState(false);
  const [phase, setPhase] = useState<'wizard' | 'plan' | 'progress' | 'try'>('wizard');
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const pluginsQuery = usePlugins();

  useEffect(() => {
    if (!desktop || loading || user?.role !== 'admin') return;
    if (buildId || creating || createBuild.isPending) return;
    setCreating(true);
    createBuild.mutate(undefined, {
      onSuccess: (build) => {
        setSearchParams({ id: build.id }, { replace: true });
        setCreating(false);
      },
      onError: (err) => {
        setError(parseApiErrorMessage(err));
        setCreating(false);
      },
    });
  }, [
    desktop,
    loading,
    user?.role,
    buildId,
    creating,
    createBuild,
    setSearchParams,
  ]);

  useEffect(() => {
    if (!buildQuery.data || hydrated) return;
    setAnswers(fromApiAnswers(buildQuery.data.answers, buildQuery.data.publishTarget));
    setHydrated(true);
  }, [buildQuery.data, hydrated]);

  useEffect(() => {
    const status = buildQuery.data?.status;
    if (!status) return;
    if (status === 'building') {
      setPhase('progress');
      return;
    }
    if (status === 'ready_to_try') {
      setPhase('try');
      return;
    }
    if (
      status === 'planning' ||
      status === 'plan_ready' ||
      (buildQuery.data?.plans.length ?? 0) > 0
    ) {
      setPhase('plan');
    }
  }, [buildQuery.data?.status, buildQuery.data?.plans.length]);

  const dirty = useMemo(() => hasAnyAnswer(answers), [answers]);
  const canLeaveWithoutPrompt =
    phase === 'plan' ||
    phase === 'progress' ||
    phase === 'try' ||
    buildQuery.data?.status === 'building' ||
    buildQuery.data?.status === 'ready_to_try' ||
    buildQuery.data?.status === 'plan_ready';

  const requestLeave = useCallback(async () => {
    if (!dirty || canLeaveWithoutPrompt) {
      navigate('/settings/plugins');
      return;
    }
    const ok = await confirm({
      title: BUILDER_COPY.leaveTitle,
      confirmLabel: BUILDER_COPY.leaveConfirm,
      cancelLabel: BUILDER_COPY.leaveStay,
      danger: true,
    });
    if (ok) navigate('/settings/plugins');
  }, [dirty, canLeaveWithoutPrompt, confirm, navigate]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      void requestLeave();
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [requestLeave]);

  if (loading) {
    return <p className="font-body text-sm text-text-muted">Loading session…</p>;
  }
  if (user?.role !== 'admin') {
    return <Navigate to="/boards" replace />;
  }
  if (!desktop) {
    return <Navigate to="/settings/plugins" replace />;
  }

  async function persistAndMaybeAdvance(nextStep: number | 'plan') {
    if (!buildId) return;
    setError(null);
    try {
      const body: {
        answers: Record<string, unknown>;
        publishTarget?: string | null;
      } = { answers: toApiAnswers(answers) };
      if (answers.where === 'git') {
        body.publishTarget = answers.publishTarget.trim();
      } else {
        body.publishTarget = null;
      }
      await patch.mutateAsync(body);
      if (nextStep === 'plan') {
        await planRun.mutateAsync(undefined);
        setPhase('plan');
      } else {
        setStep(nextStep);
      }
    } catch (err) {
      setError(parseApiErrorMessage(err));
    }
  }

  const busy = patch.isPending || planRun.isPending;
  const build = buildQuery.data;

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <button type="button" onClick={() => void requestLeave()} className={ghostButton}>
          {BUILDER_COPY.backToPlugins}
        </button>
        {phase === 'wizard' && (
          <p className="font-body text-sm text-text-secondary">
            {BUILDER_COPY.stepOf(step, TOTAL_STEPS)}
          </p>
        )}
      </div>

      {error && <p className="font-body text-sm text-danger">{error}</p>}

      {!buildId || buildQuery.isLoading || !hydrated ? (
        <p className="font-body text-sm text-text-muted motion-safe:animate-pulse">
          {phase === 'plan' ? BUILDER_COPY.writingPlan : 'Loading…'}
        </p>
      ) : phase === 'progress' && build ? (
        <PluginBuildProgress
          build={build}
          onDiscarded={() => navigate('/settings/plugins', { replace: true })}
        />
      ) : phase === 'try' && build ? (
        <PluginBuildTryView
          build={build}
          onDiscarded={() => navigate('/settings/plugins', { replace: true })}
          onAccepted={(pluginId, name, publish, publishTarget) => {
            const missing =
              pluginsQuery.data
                ?.find((p) => p.id === pluginId)
                ?.settings.filter((s) => s.source === 'missing')
                .map((s) => s.key) ?? [];
            const notice =
              missing.length > 0
                ? `${BUILDER_COPY.acceptedNotice(name)} ${missingSettingsNotice(missing)}`
                : BUILDER_COPY.acceptedNotice(name);
            navigate('/settings/plugins', {
              replace: true,
              state: {
                notice,
                highlightIds: pluginId ? [pluginId] : [],
                publish:
                  publish && publish.status !== 'skipped'
                    ? {
                        ...publish,
                        pluginId,
                        publishTarget: publish.url ?? publishTarget,
                      }
                    : undefined,
              },
            });
          }}
        />
      ) : phase === 'plan' && build ? (
        <PluginBuildPlanReview build={build} />
      ) : (
        <WizardStep
          step={step}
          answers={answers}
          setAnswers={setAnswers}
          busy={busy}
          onBack={() => setStep((s) => Math.max(1, s - 1))}
          onNext={() => {
            if (!stepValid(step, answers)) return;
            if (step === TOTAL_STEPS) {
              void persistAndMaybeAdvance('plan');
            } else {
              void persistAndMaybeAdvance(step + 1);
            }
          }}
        />
      )}
    </div>
  );
}

function WizardStep({
  step,
  answers,
  setAnswers,
  busy,
  onBack,
  onNext,
}: {
  step: number;
  answers: WizardAnswers;
  setAnswers: Dispatch<SetStateAction<WizardAnswers>>;
  busy: boolean;
  onBack: () => void;
  onNext: () => void;
}) {
  const valid = stepValid(step, answers);
  const primaryLabel = step === TOTAL_STEPS ? BUILDER_COPY.seeThePlan : BUILDER_COPY.next;

  return (
    <section className="space-y-4" aria-label={BUILDER_COPY.stepOf(step, TOTAL_STEPS)}>
      {step === 1 && (
        <>
          <h1 className="font-display text-xl font-semibold text-text-primary">
            {BUILDER_COPY.q1Title}
          </h1>
          <p className="font-body text-sm text-text-secondary">{BUILDER_COPY.q1Help}</p>
          <textarea
            value={answers.purpose}
            onChange={(e) => setAnswers((a) => ({ ...a, purpose: e.target.value }))}
            placeholder={BUILDER_COPY.q1Placeholder}
            rows={3}
            disabled={busy}
            className={fieldClass}
          />
        </>
      )}

      {step === 2 && (
        <>
          <h1 className="font-display text-xl font-semibold text-text-primary">
            {BUILDER_COPY.q2Title}
          </h1>
          <p className="font-body text-sm text-text-secondary">{BUILDER_COPY.q2Help}</p>
          <textarea
            value={answers.jobsText}
            onChange={(e) => setAnswers((a) => ({ ...a, jobsText: e.target.value }))}
            placeholder={BUILDER_COPY.q2Placeholder}
            rows={4}
            disabled={busy}
            className={fieldClass}
          />
        </>
      )}

      {step === 3 && (
        <>
          <h1 className="font-display text-xl font-semibold text-text-primary">
            {BUILDER_COPY.q3Title}
          </h1>
          <p className="font-body text-sm text-text-secondary">{BUILDER_COPY.q3Help}</p>
          <div className="flex flex-wrap gap-2">
            {(
              [
                [TOOL_CHIP_VALUES.none, BUILDER_COPY.q3None],
                [TOOL_CHIP_VALUES.git, BUILDER_COPY.q3Git],
                [TOOL_CHIP_VALUES.GitHub, BUILDER_COPY.q3GitHub],
                [TOOL_CHIP_VALUES.cli, BUILDER_COPY.q3Cli],
                [TOOL_CHIP_VALUES.api, BUILDER_COPY.q3Api],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                type="button"
                aria-pressed={answers.tools.includes(value)}
                disabled={busy}
                onClick={() =>
                  setAnswers((a) => ({ ...a, tools: toggleTool(a.tools, value) }))
                }
                className={chipClass}
              >
                {label}
              </button>
            ))}
          </div>
          <input
            value={answers.toolsExtra}
            onChange={(e) => setAnswers((a) => ({ ...a, toolsExtra: e.target.value }))}
            placeholder={BUILDER_COPY.q3Placeholder}
            disabled={busy || answers.tools.includes(TOOL_CHIP_VALUES.none)}
            className={fieldClass}
          />
        </>
      )}

      {step === 4 && (
        <>
          <h1 className="font-display text-xl font-semibold text-text-primary">
            {BUILDER_COPY.q4Title}
          </h1>
          <p className="font-body text-sm text-text-secondary">{BUILDER_COPY.q4Help}</p>
          <div className="flex flex-wrap gap-2">
            {(
              [
                ['none', BUILDER_COPY.q4None],
                ['internet', BUILDER_COPY.q4Internet],
                ['key', BUILDER_COPY.q4Key],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                type="button"
                aria-pressed={answers.secrets === value}
                disabled={busy}
                onClick={() => setAnswers((a) => ({ ...a, secrets: value }))}
                className={chipClass}
              >
                {label}
              </button>
            ))}
          </div>
          {answers.secrets && answers.secrets !== 'none' && (
            <div className="flex flex-col gap-2">
              <label htmlFor="builder-secret-service" className="font-body text-sm text-text-primary">
                {BUILDER_COPY.q4ServiceLabel}
              </label>
              <input
                id="builder-secret-service"
                value={answers.secretService}
                onChange={(e) => setAnswers((a) => ({ ...a, secretService: e.target.value }))}
                placeholder={BUILDER_COPY.q4ServicePlaceholder}
                disabled={busy}
                className={fieldClass}
              />
            </div>
          )}
        </>
      )}

      {step === 5 && (
        <>
          <h1 className="font-display text-xl font-semibold text-text-primary">
            {BUILDER_COPY.q5Title}
          </h1>
          <div className="flex flex-col gap-3">
            {(
              [
                ['local', BUILDER_COPY.q5Local, BUILDER_COPY.q5LocalHelp],
                ['git', BUILDER_COPY.q5Git, BUILDER_COPY.q5GitHelp],
              ] as const
            ).map(([value, label, help]) => (
              <button
                key={value}
                type="button"
                aria-pressed={answers.where === value}
                disabled={busy}
                onClick={() => setAnswers((a) => ({ ...a, where: value }))}
                className={`${chipClass} text-left`}
              >
                <span className="font-medium">{label}</span>
                <span className="mt-0.5 block text-text-secondary">{help}</span>
              </button>
            ))}
          </div>
          {answers.where === 'git' && (
            <div className="flex flex-col gap-2">
              <label htmlFor="builder-publish-target" className="font-body text-sm text-text-primary">
                {BUILDER_COPY.q5RepoLabel}
              </label>
              <input
                id="builder-publish-target"
                value={answers.publishTarget}
                onChange={(e) => setAnswers((a) => ({ ...a, publishTarget: e.target.value }))}
                placeholder={BUILDER_COPY.q5RepoPlaceholder}
                disabled={busy}
                className={fieldClass}
              />
              <p className="font-body text-xs text-text-secondary">{BUILDER_COPY.q5RepoHelp}</p>
            </div>
          )}
        </>
      )}

      <div className="flex items-center justify-between pt-2">
        <button
          type="button"
          onClick={onBack}
          disabled={busy || step === 1}
          className={ghostButton}
        >
          {BUILDER_COPY.back}
        </button>
        <button
          type="button"
          onClick={onNext}
          disabled={busy || !valid}
          className={primaryButton}
        >
          {busy && step === TOTAL_STEPS ? PLAN_COPY.writingPlan : primaryLabel}
        </button>
      </div>
    </section>
  );
}
