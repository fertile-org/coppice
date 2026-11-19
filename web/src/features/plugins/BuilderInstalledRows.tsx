import { useNavigate } from 'react-router-dom';
import { parseApiErrorMessage } from '../../lib/api';
import type { PluginBuild } from '../../lib/schemas/pluginBuild';
import { BUILDER_COPY } from './builderCopy';
import {
  useDiscardPluginBuild,
  usePluginBuilds,
  useStartPluginBuildRun,
} from './usePluginBuilds';

const secondaryButton =
  'rounded-md border border-border px-2.5 py-1 font-body text-xs font-medium text-text-primary transition-colors duration-fast hover:bg-paper-200 disabled:opacity-50';
const dangerButton =
  'rounded-md border border-danger-muted px-2.5 py-1 font-body text-xs font-medium text-danger transition-colors duration-fast hover:bg-danger-muted/40 disabled:opacity-50';

function isInterrupted(build: PluginBuild): boolean {
  if (build.activeRunId) return false;
  if (build.status !== 'plan_ready' && build.status !== 'building') return false;
  return (
    build.lastRunStatus === 'cancelled' ||
    build.lastRunStatus === 'failed' ||
    (build.lastRunStatus?.startsWith('interrupted') ?? false) ||
    build.lastRunStatus === 'interrupted'
  );
}

function BuilderRow({ build }: { build: PluginBuild }) {
  const navigate = useNavigate();
  const tryAgain = useStartPluginBuildRun(build.id);
  const discard = useDiscardPluginBuild(build.id);
  const name = build.pluginName?.trim() || 'plugin';
  const interrupted = isInterrupted(build);
  const building = build.status === 'building' || Boolean(build.activeRunId);
  const ready = build.status === 'ready_to_try';

  if (!building && !ready && !interrupted) return null;

  return (
    <li className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-border bg-surface-raised px-4 py-3 shadow-card">
      <div className="min-w-0 font-body text-sm text-text-primary">
        {interrupted ? (
          <>
            <span className="font-medium">{name}</span>
            <span className="mt-0.5 block text-text-secondary">
              {BUILDER_COPY.buildStoppedClosed}
            </span>
          </>
        ) : ready ? (
          <span className={ready ? undefined : 'motion-safe:animate-pulse'}>
            {BUILDER_COPY.installedReady(name)}
          </span>
        ) : (
          <span className="motion-safe:animate-pulse">
            {BUILDER_COPY.installedBuilding(name)}
          </span>
        )}
      </div>
      <div className="flex flex-wrap gap-2">
        {interrupted ? (
          <>
            <button
              type="button"
              disabled={tryAgain.isPending || discard.isPending}
              onClick={() =>
                tryAgain.mutate(undefined, {
                  onError: (err: unknown) => {
                    console.error(parseApiErrorMessage(err));
                  },
                })
              }
              className={secondaryButton}
            >
              {BUILDER_COPY.tryAgain}
            </button>
            <button
              type="button"
              disabled={tryAgain.isPending || discard.isPending}
              onClick={() =>
                discard.mutate(undefined, {
                  onError: (err: unknown) => {
                    console.error(parseApiErrorMessage(err));
                  },
                })
              }
              className={dangerButton}
            >
              {BUILDER_COPY.discard}
            </button>
          </>
        ) : (
          <button
            type="button"
            onClick={() => navigate(`/settings/plugins/build?id=${build.id}`)}
            className={secondaryButton}
          >
            {ready ? BUILDER_COPY.review : BUILDER_COPY.open}
          </button>
        )}
      </div>
    </li>
  );
}

/** Non-terminal builder progress above the normal Installed list. */
export function BuilderInstalledRows() {
  const buildsQuery = usePluginBuilds();
  const builds = (buildsQuery.data ?? []).filter((build) => {
    if (['accepted', 'discarded', 'cancelled', 'drafting'].includes(build.status)) {
      return false;
    }
    if (build.status === 'ready_to_try') return true;
    if (build.status === 'building') return true;
    return isInterrupted(build);
  });

  if (builds.length === 0) return null;

  return (
    <ul className="space-y-2" aria-label="Plugin builds in progress">
      {builds.map((build) => (
        <BuilderRow key={build.id} build={build} />
      ))}
    </ul>
  );
}
