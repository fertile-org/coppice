import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, Copy } from 'lucide-react';
import { useRegisterShortcut } from '../keyboard/useKeyboard';
import { apiErrorToastMessage, parseApiErrorMessage } from '../../lib/api';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import { useConfirm } from '../../components/ui/confirm-dialog';
import { Popover, PopoverContent, PopoverTrigger } from '../../components/ui/popover';
import { cn } from '../../lib/utils';
import type { Ticket } from '../board/useTickets';
import { usePushDefaultBranch } from '../repos/useRepos';
import {
  ConflictNotice,
  CreatePrButton,
  MergeBranchDialog,
  RebaseBranchDialog,
} from './GitBranchDialogs';
import { selectedConflictAgent, type GitConflict } from './conflictOffer';
import { gitStateCopy, type GitStateTone, type TicketGitState } from './gitStateCopy';
import {
  gitInfoQueryKey,
  ticketQueryKey,
  usePushTicketBranch,
  useRemoveWorktree,
  useResolveConflict,
  useTicketGitInfo,
} from './useTicket';

const pillTone: Record<GitStateTone, string> = {
  neutral: 'bg-surface-raised text-text-secondary border-border',
  warning: 'bg-warning-muted text-warning border-warning',
  danger: 'bg-danger-muted text-danger border-danger',
  success: 'bg-success-muted text-success border-success',
};

const reasonTone: Record<GitStateTone, string> = {
  neutral: 'text-text-secondary',
  warning: 'text-warning',
  danger: 'text-danger',
  success: 'text-text-secondary',
};

type MenuAction =
  | 'merge'
  | 'rebase'
  | 'push-branch'
  | 'compare'
  | 'create-pr'
  | 'remove-worktree';

function isFinalReviewOrDone(status: string): boolean {
  return status === 'wait_for_final_review' || status === 'done';
}

const menuItemClass =
  'flex w-full items-center rounded-sm px-2 py-1.5 text-left font-body text-sm text-text-primary hover:bg-surface disabled:cursor-not-allowed disabled:text-text-muted disabled:hover:bg-transparent';

export function GitStatusRow({
  ticket,
  variant,
  shortcutActive = false,
}: {
  ticket: Ticket;
  variant: 'panel' | 'card';
  /** Card only: true when this card is the board tab stop, so the merge shortcut follows it. */
  shortcutActive?: boolean;
}) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const confirm = useConfirm();
  const isPanel = variant === 'panel';
  const repoId = ticket.repoId ?? '';
  const { data: gitInfo, isLoading } = useTicketGitInfo(ticket.id, Boolean(ticket.repoId));
  const removeWorktree = useRemoveWorktree(ticket.id);
  const pushBranch = usePushTicketBranch(ticket.id);
  const pushMain = usePushDefaultBranch(repoId);
  const resolveConflict = useResolveConflict(ticket.id);
  const [createPrPending, setCreatePrPending] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [detailOpen, setDetailOpen] = useState(false);
  const [mergeOpen, setMergeOpen] = useState(false);
  const [rebaseOpen, setRebaseOpen] = useState(false);
  const [gitError, setGitError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<GitConflict | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const askingRef = useRef(false);
  const [trackedTicketId, setTrackedTicketId] = useState(ticket.id);

  if (trackedTicketId !== ticket.id) {
    setTrackedTicketId(ticket.id);
    setConflict(null);
    setSelectedAgentId(null);
  }

  const review = ticket.humanReview;
  const state: TicketGitState = isPanel
    ? (gitInfo?.gitState ?? review?.gitState ?? 'in_review')
    : (review?.gitState ?? 'in_review');
  const reason = isPanel
    ? (gitInfo?.gitStateReason ?? review?.gitStateReason ?? null)
    : null;
  const copy = gitStateCopy[state];
  const showFinalActions = isFinalReviewOrDone(ticket.status);
  const isArchived = Boolean(ticket.archivedAt);
  const busy =
    removeWorktree.isPending ||
    pushBranch.isPending ||
    pushMain.isPending ||
    createPrPending ||
    asking;
  const blocked = busy || isLoading || isArchived;
  const worktreeExists = Boolean(gitInfo?.worktreeExists);

  const mainIsMerge = state === 'not_merged' || state === 'main_dirty';
  const mergeShortcutEnabled = isPanel
    ? showFinalActions && mainIsMerge && Boolean(gitInfo) && !blocked
    : () =>
        shortcutActive &&
        showFinalActions &&
        mainIsMerge &&
        Boolean(gitInfo) &&
        !blocked &&
        document.querySelector('[data-shortcut-layer="ticket"]') == null;
  useRegisterShortcut('review.merge', () => setMergeOpen(true), mergeShortcutEnabled);

  function rememberConflict(next: GitConflict | null) {
    setConflict(next);
    setSelectedAgentId(next?.defaultAgentId ?? next?.agents[0]?.id ?? null);
  }

  function clearConflictAsk() {
    setConflict(null);
    setSelectedAgentId(null);
    setMergeOpen(false);
    setRebaseOpen(false);
  }

  async function handleAsk() {
    const selected = conflict ? selectedConflictAgent(conflict, selectedAgentId) : null;
    if (!conflict || askingRef.current || !selected || selected.unavailableReason) {
      return;
    }
    askingRef.current = true;
    setAsking(true);
    try {
      await resolveConflict.mutateAsync({
        baseBranch: conflict.baseBranch,
        files: conflict.files,
        operation: conflict.operation,
        agentId: selected.id,
      });
      toast.success(`${selected.name} is resolving the conflict`);
      clearConflictAsk();
    } catch (err) {
      toast.error(apiErrorToastMessage(parseApiErrorMessage(err)));
    } finally {
      askingRef.current = false;
      setAsking(false);
    }
  }

  async function handleRemoveWorktree() {
    if (!gitInfo?.worktreeExists) return;
    const confirmed = await confirm({
      title: 'Remove worktree?',
      description: `${gitInfo.worktreePath}\n\nThe ticket branch and commits are kept in git. This cannot be undone from the UI.`,
      confirmLabel: 'Remove worktree',
      danger: true,
    });
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

  async function handlePushBranch() {
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

  async function handlePushMain() {
    setGitError(null);
    try {
      const result = await pushMain.mutateAsync();
      toast.success(result.message);
      void queryClient.invalidateQueries({ queryKey: ticketQueryKey(ticket.id) });
      void queryClient.invalidateQueries({ queryKey: gitInfoQueryKey(ticket.id) });
      void queryClient.invalidateQueries({ queryKey: ['tickets'] });
    } catch (err) {
      const message = parseApiErrorMessage(err, 'Push failed.');
      setGitError(message);
      toast.error(apiErrorToastMessage(message));
    }
  }

  function runMain() {
    switch (state) {
      case 'in_review':
        setRebaseOpen(true);
        break;
      case 'not_merged':
      case 'main_dirty':
        setMergeOpen(true);
        break;
      case 'conflict':
        if (conflict) {
          void handleAsk();
        } else {
          // The conflict details come from a merge attempt; retry to get them.
          setMergeOpen(true);
        }
        break;
      case 'not_pushed':
        void handlePushMain();
        break;
      case 'worktree_kept':
        void handleRemoveWorktree();
        break;
      default:
        break;
    }
  }

  const mainDisabled =
    blocked ||
    (state === 'in_review' && !worktreeExists) ||
    (state === 'worktree_kept' && (!worktreeExists || Boolean(ticket.hasActiveRun))) ||
    (state !== 'in_review' && state !== 'not_pushed' && state !== 'worktree_kept' && !gitInfo);
  const mainBusyLabel =
    state === 'not_pushed' && pushMain.isPending
      ? 'Pushing…'
      : state === 'worktree_kept' && removeWorktree.isPending
        ? 'Removing…'
        : copy.primary;

  // The main button already covers one of these; the menu lists the rest.
  const mainAction: MenuAction | null =
    state === 'in_review'
      ? 'rebase'
      : state === 'worktree_kept'
        ? 'remove-worktree'
        : null;
  const showMergeItem = showFinalActions && state !== 'in_review' && state !== 'merged';

  function pick(run: () => void) {
    setMenuOpen(false);
    run();
  }

  const menuItems: { key: MenuAction; node: React.ReactNode }[] = [];
  if (showMergeItem) {
    menuItems.push({
      key: 'merge',
      node: (
        <button
          type="button"
         
          className={menuItemClass}
          disabled={blocked || !gitInfo}
          onClick={() => pick(() => setMergeOpen(true))}
        >
          Merge into…
        </button>
      ),
    });
  }
  menuItems.push({
    key: 'rebase',
    node: (
      <button
        type="button"
       
        className={menuItemClass}
        disabled={blocked || !worktreeExists}
        title={worktreeExists ? undefined : 'Rebase requires an existing worktree'}
        onClick={() => pick(() => setRebaseOpen(true))}
      >
        Rebase…
      </button>
    ),
  });
  if (showFinalActions) {
    menuItems.push({
      key: 'push-branch',
      node: (
        <button
          type="button"
         
          className={menuItemClass}
          disabled={blocked || !gitInfo?.canPush}
          title={
            gitInfo?.canPush
              ? 'Push ticket branch to origin using host git credentials'
              : (gitInfo?.pushDisabledReason ?? 'Push unavailable')
          }
          onClick={() => pick(() => void handlePushBranch())}
        >
          Push branch
        </button>
      ),
    });
    if (gitInfo?.prCreateUrl) {
      const url = gitInfo.prCreateUrl;
      menuItems.push({
        key: 'compare',
        node: (
          <button
            type="button"
           
            className={menuItemClass}
            disabled={blocked}
            title="Open compare / new PR page on the git host (push the branch first)"
            onClick={() =>
              pick(() => {
                window.open(url, '_blank', 'noopener,noreferrer');
              })
            }
          >
            Open compare URL
          </button>
        ),
      });
    }
    if (gitInfo?.canCreatePr) {
      menuItems.push({
        key: 'create-pr',
        node: (
          <CreatePrButton
            ticketId={ticket.id}
            disabled={blocked}
            menuItem
            className={menuItemClass}
            onPendingChange={setCreatePrPending}
            onError={setGitError}
          />
        ),
      });
    }
    menuItems.push({
      key: 'remove-worktree',
      node: (
        <button
          type="button"
         
          className={menuItemClass}
          disabled={blocked || Boolean(ticket.hasActiveRun) || !worktreeExists}
          onClick={() => pick(() => void handleRemoveWorktree())}
        >
          {removeWorktree.isPending ? 'Removing…' : 'Remove worktree'}
        </button>
      ),
    });
  }
  const visibleItems = menuItems.filter((item) => item.key !== mainAction);

  const shortSha = review?.mergedSha ? review.mergedSha.slice(0, 7) : null;
  const showMenu = state !== 'merged' && visibleItems.length > 0;
  const buttonSize = isPanel ? 'default' : 'sm';

  async function copyWorktreePath() {
    if (!gitInfo) return;
    try {
      await navigator.clipboard.writeText(gitInfo.worktreePath);
      toast.success('Worktree path copied');
    } catch {
      toast.error('Unable to copy the path');
    }
  }

  const row = (
    <div className="flex items-center gap-2">
      <span
        className={cn(
          'inline-flex shrink-0 items-center rounded-full border px-2.5 py-0.5 font-body text-xs font-medium',
          pillTone[copy.tone],
        )}
      >
        {copy.label}
      </span>
      {state === 'merged' && shortSha && (
        <span className="ml-auto font-mono text-xs text-text-secondary">{shortSha}</span>
      )}
      {state !== 'merged' && (copy.primary || showMenu) && (
        <div className="ml-auto flex">
          {copy.primary && (
            <Button
              type="button"
              variant={mainIsMerge ? 'default' : 'secondary'}
              size={buttonSize}
              disabled={mainDisabled}
              className={showMenu ? 'rounded-r-none' : undefined}
              onClick={runMain}
            >
              {mainBusyLabel}
            </Button>
          )}
          {showMenu && (
            <Popover open={menuOpen} onOpenChange={setMenuOpen}>
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant={mainIsMerge ? 'default' : 'secondary'}
                  size={buttonSize}
                  aria-label="More git actions"
                  disabled={isArchived}
                  className={cn('px-2', copy.primary && 'rounded-l-none border-l-0')}
                >
                  <ChevronDown className="h-3.5 w-3.5" aria-hidden="true" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="end" role="group" aria-label="Git actions" className="w-52 p-1">
                {visibleItems.map((item) => (
                  <div key={item.key}>{item.node}</div>
                ))}
              </PopoverContent>
            </Popover>
          )}
        </div>
      )}
    </div>
  );

  const dialogs = gitInfo && (
    <>
      <RebaseBranchDialog
        open={rebaseOpen}
        onClose={() => setRebaseOpen(false)}
        ticketId={ticket.id}
        defaultBranch={gitInfo.defaultBranch}
        branches={gitInfo.branches}
        ticketBranch={gitInfo.ticketBranch}
        conflict={conflict}
        selectedAgentId={selectedAgentId}
        onSelectAgent={setSelectedAgentId}
        asking={asking}
        onConflict={rememberConflict}
        onAsk={() => void handleAsk()}
      />
      {showFinalActions && (
        <MergeBranchDialog
          open={mergeOpen}
          onClose={() => setMergeOpen(false)}
          ticketId={ticket.id}
          defaultBranch={gitInfo.defaultBranch}
          branches={gitInfo.branches}
          ticketBranch={gitInfo.ticketBranch}
          conflict={conflict}
          selectedAgentId={selectedAgentId}
          onSelectAgent={setSelectedAgentId}
          asking={asking}
          onConflict={rememberConflict}
          onAsk={() => void handleAsk()}
        />
      )}
    </>
  );

  if (!isPanel) {
    return (
      <div
        className="mt-2 space-y-1.5"
        data-testid="git-status-row"
        onPointerDown={(event) => event.stopPropagation()}
        onClick={(event) => event.stopPropagation()}
      >
        {row}
        {dialogs}
      </div>
    );
  }

  return (
    <div className="space-y-2" data-testid="git-status-row">
      {row}
      {reason && (
        <p
          data-testid="git-state-reason"
          className={cn('font-body text-xs', reasonTone[copy.tone])}
        >
          {reason}
        </p>
      )}
      {state === 'conflict' && !conflict && (
        <p className="font-body text-xs text-text-secondary">
          Retry the merge to load the conflicting files.
        </p>
      )}
      {isLoading && <p className="font-body text-xs text-text-muted">Loading git info…</p>}
      {gitError && (
        <p className="whitespace-pre-wrap rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
          {gitError}
        </p>
      )}
      {conflict && (
        <div className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2">
          <ConflictNotice
            conflict={conflict}
            ticketId={ticket.id}
            selectedAgentId={selectedAgentId}
            onSelectAgent={setSelectedAgentId}
            showAsk
            asking={asking}
            onAsk={() => void handleAsk()}
            idSuffix="drawer"
          />
        </div>
      )}
      {gitInfo && (
        <div>
          <button
            type="button"
            aria-expanded={detailOpen}
            onClick={() => setDetailOpen((open) => !open)}
            className="flex max-w-full items-center gap-1 font-mono text-xs text-text-secondary hover:text-text-primary"
          >
            <ChevronRight
              className={cn('h-3.5 w-3.5 shrink-0 transition-transform', detailOpen && 'rotate-90')}
              aria-hidden="true"
            />
            <span className="truncate" title={gitInfo.ticketBranch}>
              {gitInfo.ticketBranch}
            </span>
          </button>
          {detailOpen && (
            <dl className="mt-2 space-y-2 pl-4">
              <div className="space-y-1">
                <dt className="font-body text-xs font-medium text-text-muted">Worktree</dt>
                <dd className="flex items-center gap-1">
                  <span
                    className="min-w-0 truncate font-mono text-xs text-text-primary"
                    title={gitInfo.worktreePath}
                  >
                    {gitInfo.worktreeExists ? gitInfo.worktreePath : '(removed)'}
                  </span>
                  {gitInfo.worktreeExists && (
                    <button
                      type="button"
                      aria-label="Copy worktree path"
                      className="shrink-0 rounded-sm p-1 text-text-muted hover:text-text-primary"
                      onClick={() => void copyWorktreePath()}
                    >
                      <Copy className="h-3.5 w-3.5" aria-hidden="true" />
                    </button>
                  )}
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
        </div>
      )}
      {dialogs}
    </div>
  );
}
