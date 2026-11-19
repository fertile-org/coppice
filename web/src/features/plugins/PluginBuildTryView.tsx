import { useMemo, useState } from 'react';
import { useConfirm } from '../../components/ui/confirm-dialog';
import { parseApiErrorMessage } from '../../lib/api';
import type { PluginBuild } from '../../lib/schemas/pluginBuild';
import { pluginBuildTestPassed } from '../../lib/schemas/pluginBuild';
import type { PluginTestResult } from '../../lib/schemas/plugin';
import { PLAN_COPY } from '../plan/copy';
import { AcceptPluginDialog } from './AcceptPluginDialog';
import { AdvancedDetails } from './PluginBuildPlanReview';
import { BUILDER_COPY } from './builderCopy';
import { confirmStart } from './pluginConsent';
import { PluginSettingsForm } from './PluginSettingsForm';
import { PluginTestResults } from './PluginTestResults';
import {
  useAcceptPluginBuild,
  useDiscardPluginBuild,
  useStartPluginPlanRun,
} from './usePluginBuilds';
import { usePlugins, useTestPlugin } from './usePlugins';

const primaryButton =
  'rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50';
const secondaryButton =
  'rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-paper-200 disabled:cursor-not-allowed disabled:opacity-50';
const dangerButton =
  'rounded-md border border-danger-muted px-3 py-1.5 font-body text-sm font-medium text-danger transition-colors duration-fast hover:bg-danger-muted/40 disabled:cursor-not-allowed disabled:opacity-50';

export function PluginBuildTryView({
  build,
  onAccepted,
  onDiscarded,
}: {
  build: PluginBuild;
  onAccepted: (pluginId: string | null, name: string) => void;
  onDiscarded: () => void;
}) {
  const confirm = useConfirm();
  const pluginsQuery = usePlugins();
  const plugin = pluginsQuery.data?.find((p) => p.id === build.pluginId) ?? null;
  const testPlugin = useTestPlugin(build.pluginId ?? '');
  const ask = useStartPluginPlanRun(build.id);
  const discard = useDiscardPluginBuild(build.id);
  const accept = useAcceptPluginBuild(build.id);

  const [comment, setComment] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [acceptOpen, setAcceptOpen] = useState(false);
  const [localTest, setLocalTest] = useState<PluginTestResult | null>(null);

  const storedTest = build.testResult ?? null;
  const latestTest = localTest ?? storedTest;
  const passed = pluginBuildTestPassed(latestTest as PluginBuild['testResult']);
  const name = build.pluginName?.trim() || plugin?.name || 'plugin';
  const latestPlan = build.plans.length > 0 ? build.plans[build.plans.length - 1] : null;

  const missingSettings = useMemo(
    () => plugin?.settings.filter((s) => s.source === 'missing') ?? [],
    [plugin],
  );

  const busy =
    ask.isPending || discard.isPending || accept.isPending || testPlugin.isPending;

  async function handleTestAgain() {
    if (!plugin) return;
    if (!(await confirmStart(plugin, confirm))) return;
    setError(null);
    try {
      const result = await testPlugin.mutateAsync();
      setLocalTest(result);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Test failed.'));
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

  async function handleAcceptConfirm(opts: { enable: boolean; publish: boolean }) {
    if (!plugin && opts.enable) {
      setError('Plugin is not installed yet.');
      return;
    }
    setError(null);
    try {
      if (opts.enable && plugin) {
        if (!(await confirmStart(plugin, confirm))) return;
      }
      await accept.mutateAsync({ enable: opts.enable, publish: opts.publish });
      setAcceptOpen(false);
      onAccepted(build.pluginId ?? null, name);
    } catch (err) {
      setError(parseApiErrorMessage(err));
    }
  }

  return (
    <section className="space-y-5" aria-label={BUILDER_COPY.readyTitle(name)}>
      <div>
        <h1 className="font-display text-xl font-semibold text-text-primary">
          {BUILDER_COPY.readyTitle(name)}
        </h1>
        <p className="mt-1 font-body text-sm text-text-secondary">{BUILDER_COPY.readySub}</p>
      </div>

      <div className="space-y-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <h2 className="font-display text-sm font-semibold text-text-primary">Test</h2>
            <span
              className={
                passed
                  ? 'font-body text-sm text-success'
                  : 'font-body text-sm text-danger'
              }
            >
              {passed ? BUILDER_COPY.testPassed : BUILDER_COPY.testFailed}
            </span>
          </div>
          <button
            type="button"
            disabled={busy || !plugin}
            onClick={() => void handleTestAgain()}
            className={secondaryButton}
          >
            {testPlugin.isPending ? 'Testing…' : BUILDER_COPY.testAgain}
          </button>
        </div>
        {latestTest && <PluginTestResults result={latestTest} />}
        {plugin?.skills && plugin.skills.length > 0 && (
          <ul className="list-disc space-y-1 pl-5 font-body text-sm text-text-primary">
            {plugin.skills.map((skill) => (
              <li key={skill.name}>Skill: {skill.name}</li>
            ))}
          </ul>
        )}
      </div>

      {plugin && missingSettings.length > 0 && (
        <p className="font-body text-sm text-warning">
          Add {missingSettings.map((s) => s.key).join(', ')} before you test it.
        </p>
      )}
      {plugin && plugin.settings.length > 0 && (
        <PluginSettingsForm pluginId={plugin.id} settings={plugin.settings} />
      )}

      {latestPlan?.advanced && <AdvancedDetails advanced={latestPlan.advanced} />}

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
          rows={3}
          className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary"
        />
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            disabled={busy}
            onClick={() => void handleDiscard()}
            className={dangerButton}
          >
            {BUILDER_COPY.discard}
          </button>
          <button
            type="submit"
            disabled={busy || !comment.trim()}
            className={secondaryButton}
          >
            {ask.isPending ? PLAN_COPY.sending : BUILDER_COPY.askForChanges}
          </button>
          <button
            type="button"
            disabled={busy || !passed}
            title={!passed ? BUILDER_COPY.acceptDisabledTitle : undefined}
            onClick={() => setAcceptOpen(true)}
            className={`ml-auto ${passed ? primaryButton : secondaryButton}`}
          >
            {BUILDER_COPY.accept}
          </button>
        </div>
      </form>

      {error && <p className="font-body text-sm text-danger">{error}</p>}

      <AcceptPluginDialog
        open={acceptOpen}
        publishTarget={build.publishTarget}
        busy={accept.isPending}
        onClose={() => setAcceptOpen(false)}
        onConfirm={(opts) => void handleAcceptConfirm(opts)}
      />
    </section>
  );
}