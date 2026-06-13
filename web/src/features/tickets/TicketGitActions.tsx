import { useEffect, useState, type FormEvent } from 'react';
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
  useMergeTicketBranch,
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
    } catch {
      setError('Merge failed. Check that the base branch is clean and the ticket branch exists.');
      toast.error('Merge failed');
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
            <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
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
    } catch {
      setGitError('Unable to remove worktree.');
      toast.error('Unable to remove worktree');
    }
  }

  const busy = removeWorktree.isPending;

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
        </dl>
      )}

      <div className="flex flex-col gap-2">
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
        <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
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
