import { useState } from 'react';
import { Button } from '../../components/ui/button';
import { apiErrorToastMessage, parseApiErrorMessage } from '../../lib/api';
import { useToast } from '../../components/ToastProvider';
import type { Repo } from '../../lib/schemas/repo';
import {
  type DefaultBranchSyncStatus,
  useDefaultBranchSync,
  useFetchDefaultBranch,
  usePushDefaultBranch,
} from './useRepos';

function shortSha(sha: string | null): string {
  if (!sha) return '—';
  return sha.slice(0, 7);
}

function syncSummary(status: DefaultBranchSyncStatus): string {
  const parts: string[] = [];
  if (!status.workingTreeClean) {
    parts.push('working tree dirty');
  }
  if (status.remoteSha == null) {
    parts.push(status.pushDisabledReason ?? 'Fetch remote first');
  } else if (status.aheadCount != null && status.behindCount != null) {
    if (status.aheadCount === 0 && status.behindCount === 0) {
      parts.push('in sync with remote');
    } else {
      parts.push(
        `ahead ${status.aheadCount}, behind ${status.behindCount}`,
      );
    }
  }
  return parts.join(' · ') || `${status.defaultBranch} sync`;
}

export interface DefaultBranchSyncControlsProps {
  repo: Repo;
  /** Test override — when set, hooks are not used. */
  statusOverride?: DefaultBranchSyncStatus;
  onFetch?: () => void | Promise<void>;
  onPush?: () => void | Promise<void>;
  fetchPending?: boolean;
  pushPending?: boolean;
}

/** Sync status + Fetch / Push for a ready repo's default branch (admin UI). */
export function DefaultBranchSyncControls({
  repo,
  statusOverride,
  onFetch,
  onPush,
  fetchPending: fetchPendingOverride,
  pushPending: pushPendingOverride,
}: DefaultBranchSyncControlsProps) {
  const toast = useToast();
  const enabled = statusOverride == null && repo.verificationStatus === 'ready';
  const { data: fetched, isLoading } = useDefaultBranchSync(repo.id, enabled);
  const fetchMutation = useFetchDefaultBranch(repo.id);
  const pushMutation = usePushDefaultBranch(repo.id);
  const [error, setError] = useState<string | null>(null);

  const status = statusOverride ?? fetched;
  const fetchPending = fetchPendingOverride ?? fetchMutation.isPending;
  const pushPending = pushPendingOverride ?? pushMutation.isPending;
  const busy = fetchPending || pushPending;

  if (repo.verificationStatus !== 'ready' && statusOverride == null) {
    return null;
  }

  async function handleFetch() {
    setError(null);
    try {
      if (onFetch) {
        await onFetch();
      } else {
        await fetchMutation.mutateAsync();
      }
      toast.success('Remote refs updated');
    } catch (err) {
      const message = parseApiErrorMessage(err, 'Fetch failed.');
      setError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  async function handlePush() {
    if (
      !window.confirm(
        `Push local \`${repo.defaultBranch}\` to the remote default branch?\n\nThis updates the remote to match your local checkout (no force).`,
      )
    ) {
      return;
    }
    setError(null);
    try {
      if (onPush) {
        await onPush();
      } else {
        const result = await pushMutation.mutateAsync();
        toast.success(result.message);
      }
    } catch (err) {
      const message = parseApiErrorMessage(err, 'Push failed.');
      setError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  return (
    <div
      className="mt-2 w-full max-w-xs space-y-2 rounded-md border border-border bg-surface px-2 py-2 text-left"
      data-testid="default-branch-sync"
    >
      <p className="font-body text-xs font-medium text-text-muted">
        Default branch sync
      </p>
      {isLoading && statusOverride == null && (
        <p className="font-body text-xs text-text-muted">Loading sync status…</p>
      )}
      {status && (
        <>
          <p className="font-body text-xs text-text-secondary">
            {syncSummary(status)}
          </p>
          <p className="font-mono text-[11px] text-text-muted">
            local {shortSha(status.localSha)} · remote {shortSha(status.remoteSha)}
          </p>
          <div className="flex flex-wrap gap-1">
            <Button
              type="button"
              variant="secondary"
              disabled={busy || !status.canFetch}
              title={
                status.canFetch
                  ? 'Update remote-tracking ref for the default branch'
                  : (status.fetchDisabledReason ?? 'Fetch unavailable')
              }
              onClick={() => void handleFetch()}
            >
              {fetchPending ? 'Fetching…' : 'Fetch'}
            </Button>
            <Button
              type="button"
              variant="secondary"
              disabled={busy || !status.canPush}
              title={
                status.canPush
                  ? `Push local ${status.defaultBranch} to remote`
                  : (status.pushDisabledReason ?? 'Push unavailable')
              }
              onClick={() => void handlePush()}
            >
              {pushPending ? 'Pushing…' : 'Push to remote'}
            </Button>
          </div>
          {!status.canPush && status.pushDisabledReason && (
            <p className="font-body text-xs text-text-muted">
              {status.pushDisabledReason}
            </p>
          )}
          {!status.canFetch && status.fetchDisabledReason && (
            <p className="font-body text-xs text-text-muted">
              {status.fetchDisabledReason}
            </p>
          )}
        </>
      )}
      {error && <p className="font-body text-xs text-danger">{error}</p>}
    </div>
  );
}
