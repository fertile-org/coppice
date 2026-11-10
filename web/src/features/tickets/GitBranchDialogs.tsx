import { useEffect, useState, type FormEvent } from 'react';
import { apiErrorToastMessage, parseApiErrorMessage } from '../../lib/api';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { Label } from '../../components/ui/label';
import {
  useCreateTicketPr,
  useMergeTicketBranch,
  useRebaseTicketBranch,
} from './useTicket';
import {
  parseGitConflict,
  selectedConflictAgent,
  type GitConflict,
} from './conflictOffer';

function selectedReason(conflict: GitConflict, selectedAgentId: string | null): string | null {
  const selected = selectedConflictAgent(conflict, selectedAgentId);
  if (selected?.unavailableReason) {
    return selected.unavailableReason;
  }
  if (conflict.agents.length === 0) {
    return conflict.unavailableReason;
  }
  return null;
}

function ConflictPicker({
  conflict,
  ticketId,
  selectedAgentId,
  onSelectAgent,
  idSuffix,
}: {
  conflict: GitConflict;
  ticketId: string;
  selectedAgentId: string | null;
  onSelectAgent: (id: string) => void;
  idSuffix: string;
}) {
  if (conflict.agents.length === 0) {
    return null;
  }
  const selectId = `conflict-resolver-${ticketId}-${idSuffix}`;
  const selected = selectedConflictAgent(conflict, selectedAgentId);
  return (
    <div className="space-y-2">
      <Label htmlFor={selectId}>Who should resolve it?</Label>
      <select
        id={selectId}
        aria-label="Who should resolve it?"
        className="h-9 w-full rounded-md border border-border bg-surface-raised px-2 font-body text-sm text-text-primary"
        value={selected?.id ?? ''}
        onChange={(event) => onSelectAgent(event.target.value)}
      >
        {conflict.agents.map((agent) => (
          <option key={agent.id} value={agent.id}>
            {agent.name}
          </option>
        ))}
      </select>
    </div>
  );
}

export function ConflictNotice({
  conflict,
  ticketId,
  selectedAgentId,
  onSelectAgent,
  showAsk,
  asking,
  onAsk,
  idSuffix,
}: {
  conflict: GitConflict;
  ticketId: string;
  selectedAgentId: string | null;
  onSelectAgent: (id: string) => void;
  showAsk: boolean;
  asking: boolean;
  onAsk: () => void;
  idSuffix: string;
}) {
  const selected = selectedConflictAgent(conflict, selectedAgentId);
  const reason = selectedReason(conflict, selectedAgentId);
  const canAsk = Boolean(selected && !selected.unavailableReason);
  return (
    <div data-testid="git-conflict" className="space-y-2">
      <p className="whitespace-pre-wrap font-body text-sm text-danger">{conflict.message}</p>
      <p className="font-body text-sm text-text-secondary">{conflict.rereviewNote}</p>
      <ConflictPicker
        conflict={conflict}
        ticketId={ticketId}
        selectedAgentId={selectedAgentId}
        onSelectAgent={onSelectAgent}
        idSuffix={idSuffix}
      />
      {reason && (
        <p className="font-body text-sm text-text-secondary">{reason}</p>
      )}
      {showAsk && selected && (
        <Button type="button" disabled={asking || !canAsk} onClick={onAsk}>
          {selected.askLabel}
        </Button>
      )}
    </div>
  );
}


export function CreatePrButton({
  ticketId,
  disabled,
  className,
  size,
  menuItem,
  onPendingChange,
  onError,
}: {
  ticketId: string;
  disabled?: boolean;
  className?: string;
  size?: 'default' | 'sm';
  /** Render as a plain menu item (dropdown) instead of a button. */
  menuItem?: boolean;
  onPendingChange?: (pending: boolean) => void;
  onError?: (message: string | null) => void;
}) {
  const toast = useToast();
  const createPr = useCreateTicketPr(ticketId);

  useEffect(() => {
    onPendingChange?.(createPr.isPending);
  }, [createPr.isPending, onPendingChange]);

  // The button can unmount (menu closed) mid-request; never leave the parent busy.
  useEffect(() => () => onPendingChange?.(false), [onPendingChange]);

  async function handleCreatePr() {
    onError?.(null);
    try {
      const result = await createPr.mutateAsync({});
      toast.success(`PR #${result.pullRequest.number} created`);
      window.open(result.pullRequest.prUrl, '_blank', 'noopener,noreferrer');
    } catch (err) {
      const message = parseApiErrorMessage(err, 'Create PR failed.');
      onError?.(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  if (menuItem) {
    return (
      <button
        type="button"
        disabled={disabled || createPr.isPending}
        title="Push the ticket branch (if needed) and create a GitHub pull request via API"
        onClick={() => void handleCreatePr()}
        className={className}
      >
        {createPr.isPending ? 'Creating…' : 'Create PR via API'}
      </button>
    );
  }

  return (
    <Button
      type="button"
      variant="secondary"
      size={size}
      disabled={disabled || createPr.isPending}
      title="Push the ticket branch (if needed) and create a GitHub pull request via API"
      onClick={() => void handleCreatePr()}
      className={className}
    >
      {createPr.isPending ? 'Creating…' : 'Create PR via API'}
    </Button>
  );
}

export function MergeBranchDialog({
  open,
  onClose,
  ticketId,
  defaultBranch,
  branches,
  ticketBranch,
  conflict,
  selectedAgentId,
  onSelectAgent,
  asking,
  onConflict,
  onAsk,
  onMerged,
}: {
  open: boolean;
  onClose: () => void;
  ticketId: string;
  defaultBranch: string;
  branches: string[];
  ticketBranch: string;
  conflict: GitConflict | null;
  selectedAgentId: string | null;
  onSelectAgent: (id: string) => void;
  asking: boolean;
  onConflict: (conflict: GitConflict | null) => void;
  onAsk: () => void;
  onMerged?: () => void;
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
      onConflict(null);
      onMerged?.();
      onClose();
    } catch (err) {
      const gitConflict = parseGitConflict(err);
      if (gitConflict) {
        onConflict(gitConflict);
        setError(null);
        toast.error(apiErrorToastMessage(gitConflict.message));
        return;
      }
      onConflict(null);
      const message = parseApiErrorMessage(
        err,
        'Merge failed. Check that the base branch is clean and the ticket branch exists.',
      );
      setError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  const options = branches.length > 0 ? branches : [defaultBranch];
  const selected = conflict ? selectedConflictAgent(conflict, selectedAgentId) : null;
  const canAsk = Boolean(selected && !selected.unavailableReason);

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="z-[60] max-w-md" overlayClassName="z-[60]">
        <DialogTitle>Merge ticket branch</DialogTitle>
        <DialogDescription className="mt-1">
          Merge <span className="font-mono text-xs">{ticketBranch}</span> into a
          base branch. You can run this multiple times.
        </DialogDescription>

        <form onSubmit={(e) => void handleSubmit(e)} className="mt-5 space-y-4">
          <div className="space-y-2">
            <Label htmlFor={`merge-base-branch-${ticketId}`}>Base branch</Label>
            <Combobox
              id={`merge-base-branch-${ticketId}`}
              value={baseBranch}
              onValueChange={setBaseBranch}
              placeholder="Select branch…"
              searchPlaceholder="Search branches…"
              options={options.map((branch) => ({ value: branch, label: branch }))}
            />
          </div>

          {conflict && (
            <div className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2">
              <ConflictNotice
                conflict={conflict}
                ticketId={ticketId}
                selectedAgentId={selectedAgentId}
                onSelectAgent={onSelectAgent}
                showAsk={false}
                asking={asking}
                onAsk={onAsk}
                idSuffix="merge"
              />
            </div>
          )}

          {error && (
            <p className="whitespace-pre-wrap rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
              {error}
            </p>
          )}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" variant="secondary" loading={mergeBranch.isPending}>
              {mergeBranch.isPending ? 'Merging…' : conflict ? 'Merge again' : 'Merge'}
            </Button>
            {selected && (
              <Button type="button" disabled={asking || !canAsk} onClick={onAsk}>
                {selected.askLabel}
              </Button>
            )}
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function RebaseBranchDialog({
  open,
  onClose,
  ticketId,
  defaultBranch,
  branches,
  ticketBranch,
  conflict,
  selectedAgentId,
  onSelectAgent,
  asking,
  onConflict,
  onAsk,
}: {
  open: boolean;
  onClose: () => void;
  ticketId: string;
  defaultBranch: string;
  branches: string[];
  ticketBranch: string;
  conflict: GitConflict | null;
  selectedAgentId: string | null;
  onSelectAgent: (id: string) => void;
  asking: boolean;
  onConflict: (conflict: GitConflict | null) => void;
  onAsk: () => void;
}) {
  const toast = useToast();
  const rebaseBranch = useRebaseTicketBranch(ticketId);
  const [baseBranch, setBaseBranch] = useState(defaultBranch);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setBaseBranch(defaultBranch);
      setError(null);
    }
  }, [open, defaultBranch]);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!baseBranch) {
      setError('Select a base branch.');
      return;
    }
    setError(null);
    try {
      const result = await rebaseBranch.mutateAsync(baseBranch);
      toast.success(
        `${result.rebase.message} — if already pushed, force-with-lease push is not available yet`,
      );
      onConflict(null);
      onClose();
    } catch (err) {
      const gitConflict = parseGitConflict(err);
      if (gitConflict) {
        onConflict(gitConflict);
        setError(null);
        toast.error(apiErrorToastMessage(gitConflict.message));
        return;
      }
      onConflict(null);
      const message = parseApiErrorMessage(
        err,
        'Rebase failed. Worktree must be clean; conflicts are aborted automatically.',
      );
      setError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  const options = branches.length > 0 ? branches : [defaultBranch];
  const selected = conflict ? selectedConflictAgent(conflict, selectedAgentId) : null;
  const canAsk = Boolean(selected && !selected.unavailableReason);

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="z-[60] max-w-md" overlayClassName="z-[60]">
        <DialogTitle>Rebase ticket branch</DialogTitle>
        <DialogDescription className="mt-1">
          Rebase <span className="font-mono text-xs">{ticketBranch}</span> onto a
          base branch in the ticket worktree. The worktree must be clean.
        </DialogDescription>

        <form onSubmit={(e) => void handleSubmit(e)} className="mt-5 space-y-4">
          <div className="space-y-2">
            <Label htmlFor="rebase-base-branch">Base branch</Label>
            <Combobox
              id="rebase-base-branch"
              value={baseBranch}
              onValueChange={setBaseBranch}
              placeholder="Select branch…"
              searchPlaceholder="Search branches…"
              options={options.map((branch) => ({ value: branch, label: branch }))}
            />
          </div>

          {conflict && (
            <div className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2">
              <ConflictNotice
                conflict={conflict}
                ticketId={ticketId}
                selectedAgentId={selectedAgentId}
                onSelectAgent={onSelectAgent}
                showAsk={false}
                asking={asking}
                onAsk={onAsk}
                idSuffix="rebase"
              />
            </div>
          )}

          {error && (
            <p
              data-testid="rebase-inline-error"
              className="whitespace-pre-wrap rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger"
            >
              {error}
            </p>
          )}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" variant="secondary" loading={rebaseBranch.isPending}>
              {rebaseBranch.isPending ? 'Rebasing…' : conflict ? 'Rebase again' : 'Rebase'}
            </Button>
            {selected && (
              <Button type="button" disabled={asking || !canAsk} onClick={onAsk}>
                {selected.askLabel}
              </Button>
            )}
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

