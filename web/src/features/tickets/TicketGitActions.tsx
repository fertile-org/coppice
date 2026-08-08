import { useEffect, useState, type FormEvent } from 'react';
import { apiErrorToastMessage, parseApiErrorMessage } from '../../lib/api';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import { Label } from '../../components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../components/ui/select';
import type { Ticket } from '../board/useTickets';
import {
  useCreateTicketPr,
  useMergeTicketBranch,
  usePushTicketBranch,
  useRemoveWorktree,
  useTicketGitInfo,
} from './useTicket';

interface TicketGitActionsProps {
  ticket: Ticket;
}

function MergeBranchDialog({
  open,
  onClose,
  ticketId,
  defaultBranch,
  branches,
  ticketBranch,
}: {
  open: boolean;
  onClose: () => void;
  ticketId: string;
  defaultBranch: string;
  branches: string[];
  ticketBranch: string;
}) {
  const toast = useToast();
  const mergeBranch = useMergeTicketBranch(ticketId);
  const [baseBranch, setBaseBranch] = useState(defaultBranch);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setBaseBranch(defaultBranch);
      setError(null);
    }
  }, [open, defaultBranch]);

  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!baseBranch) {
      setError('Select a base branch.');
      return;
    }
    setError(null);
    try {
      const result = await mergeBranch.mutateAsync(baseBranch);
      toast.success(result.merge.message);
      onClose();
    } catch (err) {
      const message = parseApiErrorMessage(
        err,
        'Merge failed. Check that the base branch is clean and the ticket branch exists.',
      );
      setError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  const options = branches.length > 0 ? branches : [defaultBranch];

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-bark-950/40 px-4"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="merge-branch-title"
        className="w-full max-w-md rounded-xl border border-border bg-paper-50 p-6 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <h2
          id="merge-branch-title"
          className="font-display text-xl font-semibold text-bark-900"
        >
          Merge ticket branch
        </h2>
        <p className="mt-1 font-body text-sm text-text-secondary">
          Merge <span className="font-mono text-xs">{ticketBranch}</span> into a
          base branch. You can run this multiple times.
        </p>

        <form onSubmit={(e) => void handleSubmit(e)} className="mt-5 space-y-4">
          <div className="space-y-2">
            <Label htmlFor="merge-base-branch">Base branch</Label>
            <Select value={baseBranch} onValueChange={setBaseBranch}>
              <SelectTrigger id="merge-base-branch">
                <SelectValue placeholder="Select branch…" />
              </SelectTrigger>
              <SelectContent>
                {options.map((branch) => (
                  <SelectItem key={branch} value={branch} textValue={branch}>
                    {branch}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {error && (
            <p className="whitespace-pre-wrap rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
              {error}
            </p>
          )}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" loading={mergeBranch.isPending}>
              {mergeBranch.isPending ? 'Merging…' : 'Merge'}
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}

export function TicketGitActions({ ticket }: TicketGitActionsProps) {
  const toast = useToast();
  const showActions =
    ticket.status === 'wait_for_final_review' || ticket.status === 'done';
  const { data: gitInfo, isLoading } = useTicketGitInfo(
    ticket.id,
    showActions && Boolean(ticket.repoId),
  );
  const removeWorktree = useRemoveWorktree(ticket.id);
  const pushBranch = usePushTicketBranch(ticket.id);
  const createPr = useCreateTicketPr(ticket.id);
  const [mergeOpen, setMergeOpen] = useState(false);
  const [gitError, setGitError] = useState<string | null>(null);

  if (!showActions || !ticket.repoId) {
    return null;
  }

  async function handleRemoveWorktree() {
    if (!gitInfo?.worktreeExists) return;
    const confirmed = window.confirm(
      `Remove worktree at:\n${gitInfo.worktreePath}\n\nThe ticket branch and commits are kept in git. This cannot be undone from the UI.`,
    );
    if (!confirmed) return;

    setGitError(null);
    try {
      await removeWorktree.mutateAsync();
      toast.success('Worktree removed');
    } catch (err) {
      const message = parseApiErrorMessage(err, 'Unable to remove worktree.');
      setGitError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  async function handlePush() {
    setGitError(null);
    try {
      const result = await pushBranch.mutateAsync();
      toast.success(result.push.message);
    } catch (err) {
      const message = parseApiErrorMessage(err, 'Push failed.');
      setGitError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  async function handleCreatePr() {
    setGitError(null);
    try {
      const result = await createPr.mutateAsync({});
      toast.success(`PR #${result.pullRequest.number} created`);
      window.open(result.pullRequest.prUrl, '_blank', 'noopener,noreferrer');
    } catch (err) {
      const message = parseApiErrorMessage(err, 'Create PR failed.');
      setGitError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  const busy =
    removeWorktree.isPending || pushBranch.isPending || createPr.isPending;

  return (
    <div className="space-y-3 rounded-md border border-border bg-surface px-3 py-3">
      <p className="font-body text-xs font-medium text-text-muted">Git actions</p>

      {isLoading && (
        <p className="font-body text-xs text-text-muted">Loading git info…</p>
      )}

      {gitInfo && (
        <dl className="space-y-2">
          <div className="space-y-1">
            <dt className="font-body text-xs font-medium text-text-muted">Ticket branch</dt>
            <dd className="truncate font-mono text-xs text-text-primary" title={gitInfo.ticketBranch}>
              {gitInfo.ticketBranch}
            </dd>
          </div>
          <div className="space-y-1">
            <dt className="font-body text-xs font-medium text-text-muted">Worktree</dt>
            <dd
              className="truncate font-mono text-xs text-text-primary"
              title={gitInfo.worktreePath}
            >
              {gitInfo.worktreeExists ? gitInfo.worktreePath : '(removed)'}
            </dd>
          </div>
          {gitInfo.prUrl && (
            <div className="space-y-1">
              <dt className="font-body text-xs font-medium text-text-muted">Pull request</dt>
              <dd>
                <a
                  href={gitInfo.prUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="font-body text-xs text-moss-700 underline-offset-2 hover:underline"
                >
                  {gitInfo.prUrl}
                </a>
              </dd>
            </div>
          )}
        </dl>
      )}

      <div className="flex flex-col gap-2">
        <Button
          type="button"
          variant="secondary"
          disabled={busy || isLoading || !gitInfo?.canPush}
          title={
            gitInfo?.canPush
              ? 'Push ticket branch to origin using the repo forge token'
              : (gitInfo?.pushDisabledReason ?? 'Push unavailable')
          }
          onClick={() => void handlePush()}
          className="w-full"
        >
          {pushBranch.isPending ? 'Pushing…' : 'Push branch'}
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={busy || isLoading || !gitInfo?.canCreatePr}
          title={
            gitInfo?.canCreatePr
              ? 'Create a GitHub pull request via API'
              : (gitInfo?.createPrDisabledReason ??
                gitInfo?.prCreateUrl
                  ? 'API create unavailable — use compare link if the branch is already pushed'
                  : 'Create PR unavailable')
          }
          onClick={() => void handleCreatePr()}
          className="w-full"
        >
          {createPr.isPending ? 'Creating…' : 'Create PR'}
        </Button>
        {gitInfo?.prCreateUrl && !gitInfo.canCreatePr && (
          <Button
            type="button"
            variant="secondary"
            disabled={busy || isLoading}
            title="Open compare URL on the git host (branch must already be pushed)"
            onClick={() => {
              window.open(gitInfo.prCreateUrl!, '_blank', 'noopener,noreferrer');
            }}
            className="w-full"
          >
            Open compare URL
          </Button>
        )}
        <Button
          type="button"
          variant="secondary"
          disabled={busy || isLoading || !gitInfo}
          onClick={() => setMergeOpen(true)}
          className="w-full"
        >
          Merge…
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={busy || isLoading || !gitInfo?.worktreeExists}
          onClick={() => void handleRemoveWorktree()}
          className="w-full"
        >
          {removeWorktree.isPending ? 'Removing…' : 'Remove worktree'}
        </Button>
      </div>

      {gitError && (
        <p className="whitespace-pre-wrap rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
          {gitError}
        </p>
      )}

      {gitInfo && (
        <MergeBranchDialog
          open={mergeOpen}
          onClose={() => setMergeOpen(false)}
          ticketId={ticket.id}
          defaultBranch={gitInfo.defaultBranch}
          branches={gitInfo.branches}
          ticketBranch={gitInfo.ticketBranch}
        />
      )}
    </div>
  );
}
