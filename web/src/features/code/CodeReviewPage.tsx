import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import { useRepos } from '../repos/useRepos';
import { useRegisterShortcut } from '../keyboard/useKeyboard';
import { PlanChecklist } from '../plan/PlanChecklist';
import { useTicket } from '../tickets/useTicket';
import { ChangedFilesPanel } from './ChangedFilesPanel';
import { CodeReviewBackButton } from './CodeReviewBackButton';
import { DiffViewer, type InlineCommentDraft } from './DiffViewer';
import { FileReviewHeader } from './FileReviewHeader';
import { nextUnviewed, stepFile, viewedStorageKey } from './reviewNavigation';
import { useViewedFiles } from './useViewedFiles';
import { briefMatchesReviewedCommit } from './briefCommit';
import { ReviewerBriefBar } from './ReviewerBriefBar';
import { SubmitReviewDialog } from './SubmitReviewDialog';
import {
  useRepoBranches,
  useRepoDiff,
  useRepoWorktrees,
} from './useCodeReview';

const VIEW_TYPE_KEY = 'coppice.codeReview.viewType';

function readViewType(): 'split' | 'unified' {
  try {
    return localStorage.getItem(VIEW_TYPE_KEY) === 'unified' ? 'unified' : 'split';
  } catch {
    return 'split';
  }
}

function storeViewType(viewType: 'split' | 'unified') {
  try {
    localStorage.setItem(VIEW_TYPE_KEY, viewType);
  } catch {
    /* private mode or quota */
  }
}

function worktreeLabel(path: string, branch: string, ticketTitle?: string | null) {
  const shortPath = path.split('/').pop() ?? path;
  if (ticketTitle) return `${shortPath} · ${branch} · ${ticketTitle}`;
  return `${shortPath} · ${branch}`;
}

export function CodeReviewPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const repoId = searchParams.get('repoId') ?? undefined;
  const ticketId = searchParams.get('ticketId') ?? undefined;
  const worktreeFromUrl = searchParams.get('worktree') ?? undefined;
  const baseBranchFromUrl = searchParams.get('baseBranch') ?? undefined;

  const { data: repos } = useRepos();
  const repo = repos?.find((item) => item.id === repoId);
  const { data: ticket } = useTicket(ticketId);
  const { data: worktrees, isLoading: worktreesLoading } =
    useRepoWorktrees(repoId);
  const { data: branchesData } = useRepoBranches(repoId);

  const [selectedFile, setSelectedFile] = useState<string | undefined>();
  const [viewType, setViewType] = useState<'split' | 'unified'>(readViewType);
  const [hideWhitespace, setHideWhitespace] = useState(false);
  const chooseViewType = (next: 'split' | 'unified') => {
    setViewType(next);
    storeViewType(next);
  };
  const [inlineComments, setInlineComments] = useState<InlineCommentDraft[]>(
    [],
  );
  const [submitOpen, setSubmitOpen] = useState(false);

  const defaultBranch =
    baseBranchFromUrl ?? branchesData?.defaultBranch ?? repo?.defaultBranch ?? 'main';

  const selectedWorktreePath = useMemo(() => {
    if (worktreeFromUrl) return worktreeFromUrl;
    if (worktrees?.length) return worktrees[0].path;
    return undefined;
  }, [worktreeFromUrl, worktrees]);

  const selectedWorktree = worktrees?.find(
    (item) => item.path === selectedWorktreePath,
  );

  const baseBranch = baseBranchFromUrl ?? defaultBranch;

  const reviewTicketId =
    ticket?.status === 'wait_for_final_review' ? ticketId : undefined;
  const { data: diffSummary, isLoading: diffLoading } = useRepoDiff(
    repoId,
    selectedWorktreePath,
    baseBranch,
    reviewTicketId,
    hideWhitespace,
  );

  const syncParams = useCallback(
    (updates: {
      worktree?: string;
      baseBranch?: string;
    }) => {
      setSearchParams((current) => {
        const next = new URLSearchParams(current);
        if (updates.worktree !== undefined) {
          if (updates.worktree) next.set('worktree', updates.worktree);
          else next.delete('worktree');
        }
        if (updates.baseBranch !== undefined) {
          if (updates.baseBranch) next.set('baseBranch', updates.baseBranch);
          else next.delete('baseBranch');
        }
        return next;
      });
    },
    [setSearchParams],
  );

  useEffect(() => {
    if (worktreeFromUrl || !worktrees?.length) return;
    syncParams({ worktree: worktrees[0].path });
  }, [worktreeFromUrl, worktrees, syncParams]);

  useEffect(() => {
    if (baseBranchFromUrl || !branchesData?.defaultBranch) return;
    syncParams({ baseBranch: branchesData.defaultBranch });
  }, [baseBranchFromUrl, branchesData?.defaultBranch, syncParams]);

  useEffect(() => {
    if (!diffSummary?.files.length) {
      setSelectedFile(undefined);
      return;
    }
    setSelectedFile((current) => {
      if (current && diffSummary.files.some((file) => file.path === current)) {
        return current;
      }
      return diffSummary.files[0]?.path;
    });
  }, [diffSummary?.files]);

  useEffect(() => {
    setInlineComments([]);
  }, [repoId, selectedWorktreePath, baseBranch]);

  const headBranch =
    diffSummary?.headBranch ?? selectedWorktree?.branch ?? 'HEAD';
  const headSha = diffSummary?.headSha ?? selectedWorktree?.headSha ?? '';
  const reviewFiles = diffSummary?.files ?? [];
  const viewedKey =
    repoId && selectedWorktreePath && headSha
      ? viewedStorageKey(repoId, selectedWorktreePath, headSha)
      : null;
  const { viewed, setViewedPath, toggleViewed } = useViewedFiles(viewedKey);
  const canMoveFiles = reviewFiles.length > 0;
  const paths = reviewFiles.map((file) => file.path);

  useRegisterShortcut(
    'diff.nextFile',
    () => {
      const next = stepFile(paths, selectedFile, 1);
      if (next) setSelectedFile(next);
    },
    canMoveFiles,
  );
  useRegisterShortcut(
    'diff.previousFile',
    () => {
      const next = stepFile(paths, selectedFile, -1);
      if (next) setSelectedFile(next);
    },
    canMoveFiles,
  );
  useRegisterShortcut(
    'diff.markViewed',
    () => {
      if (!selectedFile) return;
      const nextViewed = toggleViewed(selectedFile);
      const jump = nextUnviewed(paths, nextViewed, selectedFile);
      if (jump && jump !== selectedFile) setSelectedFile(jump);
    },
    canMoveFiles,
  );

  const ticketBoardHref = ticket
    ? `/boards/${ticket.boardId}?ticket=${ticket.id}`
    : undefined;
  const backFallbackHref = ticketBoardHref ?? '/settings/repositories';

  if (!repoId) {
    return (
      <PageShell fallbackHref={backFallbackHref}>
        <EmptyState
          title="Repository required"
          description="Open code review from a repository or ticket with a repoId query parameter."
          actionHref="/settings/repositories"
          actionLabel="Go to Repositories"
        />
      </PageShell>
    );
  }

  if (repo && repo.verificationStatus !== 'ready') {
    return (
      <PageShell repoName={repo.name} fallbackHref={backFallbackHref}>
        <EmptyState
          title="Repository not ready"
          description={
            repo.verificationError ??
            'Verify the repository path before reviewing code.'
          }
          actionHref="/settings/repositories"
          actionLabel="Go to Repositories"
        />
      </PageShell>
    );
  }

  const selectedIndex = selectedFile
    ? reviewFiles.findIndex((file) => file.path === selectedFile)
    : -1;
  const selectedReviewFile =
    selectedIndex >= 0 ? reviewFiles[selectedIndex] : undefined;

  return (
    <div className="coppice-grain flex h-screen flex-col bg-background">
      <header className="shrink-0 border-b border-border bg-surface px-5 pt-3">
        <div className="flex flex-nowrap items-start gap-4">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <CodeReviewBackButton fallbackHref={backFallbackHref} />
              <span className="font-body text-xs text-text-muted">Code review</span>
            </div>
            <CodeReviewHeading
              ticketId={ticketId}
              title={ticket?.title}
              href={ticketBoardHref}
            />
            {diffSummary && (
              <MergeLine
                repoName={repo?.name ?? '…'}
                headBranch={headBranch}
                headSha={headSha}
                baseBranch={diffSummary.baseBranch}
                baseSha={diffSummary.baseSha}
              />
            )}
          </div>
          <Button
            type="button"
            className="mt-5 shrink-0"
            disabled={!selectedWorktreePath || !headSha}
            onClick={() => setSubmitOpen(true)}
          >
            Submit review
          </Button>
        </div>

        <div className="mt-2.5 flex h-12 flex-nowrap items-center gap-3 border-t border-border">
          <span className="shrink-0 font-body text-xs text-text-muted">Worktree</span>
          <Combobox
            aria-label="Worktree"
            value={selectedWorktreePath ?? ''}
            onValueChange={(value) => syncParams({ worktree: value })}
            disabled={!worktrees?.length}
            placeholder="Select worktree…"
            searchPlaceholder="Search worktrees…"
            className="w-72 min-w-0 shrink"
            options={(worktrees ?? []).map((worktree) => ({
              value: worktree.path,
              label: worktreeLabel(
                worktree.path,
                worktree.branch,
                worktree.ticketTitle,
              ),
            }))}
          />

          <span className="shrink-0 font-body text-xs text-text-muted">Base</span>
          <Combobox
            aria-label="Base branch"
            value={baseBranch}
            onValueChange={(value) => syncParams({ baseBranch: value })}
            disabled={!branchesData?.branches.length}
            placeholder="Base branch"
            searchPlaceholder="Search branches…"
            className="w-40 shrink-0"
            contentClassName="min-w-[14rem]"
            options={(branchesData?.branches ?? [defaultBranch]).map((branch) => ({
              value: branch,
              label: branch,
            }))}
          />

          <div className="ml-auto flex shrink-0 items-center gap-3">
            <div className="flex rounded-md border border-border bg-surface-raised p-0.5">
              <button
                type="button"
                onClick={() => chooseViewType('split')}
                className={[
                  'rounded px-2.5 py-1 font-body text-xs transition-colors duration-fast',
                  viewType === 'split'
                    ? 'bg-accent-muted text-accent'
                    : 'text-text-secondary hover:text-text-primary',
                ].join(' ')}
              >
                Split
              </button>
              <button
                type="button"
                onClick={() => chooseViewType('unified')}
                className={[
                  'rounded px-2.5 py-1 font-body text-xs transition-colors duration-fast',
                  viewType === 'unified'
                    ? 'bg-accent-muted text-accent'
                    : 'text-text-secondary hover:text-text-primary',
                ].join(' ')}
              >
                Unified
              </button>
            </div>

            <label className="flex items-center gap-1.5 whitespace-nowrap font-body text-xs text-text-secondary">
              <input
                type="checkbox"
                className="accent-accent"
                checked={hideWhitespace}
                onChange={(event) => setHideWhitespace(event.target.checked)}
              />
              Hide whitespace
            </label>
          </div>
        </div>
      </header>

      {!worktreesLoading && worktrees?.length === 0 ? (
        <EmptyState
          title="No worktrees found"
          description="Create a ticket worktree or register a repository with active worktrees."
          actionHref="/settings/repositories"
          actionLabel="Go to Repositories"
        />
      ) : (
        <div
          className={
            ticket?.status === 'wait_for_final_review' && ticket.approvedPlan
              ? 'grid min-h-0 flex-1 grid-cols-[minmax(200px,240px)_minmax(220px,280px)_1fr]'
              : 'grid min-h-0 flex-1 grid-cols-[minmax(220px,280px)_1fr]'
          }
        >
          {ticket?.status === 'wait_for_final_review' && ticket.approvedPlan && (
            <PlanChecklist plan={ticket.approvedPlan} />
          )}
          <ChangedFilesPanel
            files={reviewFiles}
            selectedPath={selectedFile}
            onSelect={setSelectedFile}
            isLoading={diffLoading}
            viewedPaths={viewed}
          />
          {repoId && selectedWorktreePath && (
            <div className="flex h-full min-h-0 min-w-0 flex-col">
              {ticket?.status === 'wait_for_final_review' &&
                ticket.reviewerBrief &&
                briefMatchesReviewedCommit(
                  ticket.reviewerBrief.commitSha,
                  headSha,
                ) && <ReviewerBriefBar brief={ticket.reviewerBrief} />}
              {selectedReviewFile && (
                <FileReviewHeader
                  path={selectedReviewFile.path}
                  additions={selectedReviewFile.additions}
                  deletions={selectedReviewFile.deletions}
                  index={selectedIndex + 1}
                  total={reviewFiles.length}
                  viewed={viewed.has(selectedReviewFile.path)}
                  onViewedChange={(on) => setViewedPath(selectedReviewFile.path, on)}
                  onPrevious={() => {
                    const next = stepFile(
                      reviewFiles.map((file) => file.path),
                      selectedReviewFile.path,
                      -1,
                    );
                    if (next) setSelectedFile(next);
                  }}
                  onNext={() => {
                    const next = stepFile(
                      reviewFiles.map((file) => file.path),
                      selectedReviewFile.path,
                      1,
                    );
                    if (next) setSelectedFile(next);
                  }}
                  canPrevious={selectedIndex > 0}
                  canNext={selectedIndex < reviewFiles.length - 1}
                />
              )}
              <DiffViewer
                repoId={repoId}
                worktreePath={selectedWorktreePath}
                baseBranch={baseBranch}
                filePath={selectedFile}
                viewType={viewType}
                ignoreWhitespace={hideWhitespace}
                inlineComments={inlineComments}
                onInlineCommentsChange={setInlineComments}
              />
            </div>
          )}
        </div>
      )}

      {repoId && selectedWorktreePath && headSha && (
        <SubmitReviewDialog
          open={submitOpen}
          onClose={() => setSubmitOpen(false)}
          repoId={repoId}
          repoName={repo?.name ?? 'Repository'}
          worktreePath={selectedWorktreePath}
          baseBranch={baseBranch}
          headBranch={headBranch}
          headSha={headSha}
          ticketId={ticketId}
          inlineComments={inlineComments}
          onSubmitted={() => setInlineComments([])}
        />
      )}
    </div>
  );
}

function PageShell({
  children,
  repoName,
  fallbackHref,
}: {
  children: ReactNode;
  repoName?: string;
  fallbackHref: string;
}) {
  return (
    <div className="coppice-grain flex h-screen flex-col bg-background">
      <header className="border-b border-border bg-surface px-5 pt-3 pb-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <CodeReviewBackButton fallbackHref={fallbackHref} />
            {repoName && (
              <span className="font-body text-xs text-text-muted">Code review</span>
            )}
          </div>
          <h1
            className="mt-1 truncate font-display text-xl font-semibold text-text-primary"
            title={repoName}
          >
            {repoName ?? 'Code review'}
          </h1>
        </div>
      </header>
      <div className="flex flex-1 items-center justify-center p-8">{children}</div>
    </div>
  );
}

function CodeReviewHeading({
  ticketId,
  title,
  href,
}: {
  ticketId?: string;
  title?: string;
  href?: string;
}) {
  if (href && title) {
    return (
      <h1 className="mt-1 min-w-0">
        <Link
          to={href}
          title={title}
          className="block truncate font-display text-xl font-semibold text-text-primary hover:underline"
        >
          {title}
        </Link>
      </h1>
    );
  }

  return (
    <h1 className="mt-1 truncate font-display text-xl font-semibold text-text-muted">
      {ticketId ? 'Loading ticket…' : 'New review — ticket created on submit'}
    </h1>
  );
}

function BranchChip({ name }: { name: string }) {
  return (
    <code
      title={name}
      className="inline-block max-w-[12rem] min-w-0 shrink truncate rounded bg-accent-muted px-1.5 py-px font-mono text-xs text-accent"
    >
      {name}
    </code>
  );
}

function MergeLine({
  repoName,
  headBranch,
  headSha,
  baseBranch,
  baseSha,
}: {
  repoName: string;
  headBranch: string;
  headSha: string;
  baseBranch: string;
  baseSha: string;
}) {
  return (
    <p className="mt-0.5 flex min-w-0 flex-nowrap items-center gap-1.5 overflow-hidden font-body text-sm text-text-secondary">
      <span className="max-w-[12rem] min-w-0 shrink truncate" title={repoName}>
        {repoName}
      </span>
      <span className="shrink-0 text-text-muted" aria-hidden="true">
        ·
      </span>
      <BranchChip name={headBranch} />
      <span className="shrink-0 font-mono text-xs text-text-muted">
        {headSha.slice(0, 7)}
      </span>
      <span className="shrink-0">into</span>
      <BranchChip name={baseBranch} />
      <span className="shrink-0 font-mono text-xs text-text-muted">
        {baseSha.slice(0, 7)}
      </span>
    </p>
  );
}

function EmptyState({
  title,
  description,
  actionHref,
  actionLabel,
}: {
  title: string;
  description: string;
  actionHref: string;
  actionLabel: string;
}) {
  return (
    <div className="max-w-md text-center">
      <h2 className="font-display text-xl font-semibold text-text-primary">
        {title}
      </h2>
      <p className="mt-2 font-body text-sm text-text-secondary">{description}</p>
      <Link
        to={actionHref}
        className="mt-4 inline-flex rounded-md bg-accent px-4 py-2 font-body text-sm font-medium text-accent-foreground hover:bg-accent-hover"
      >
        {actionLabel}
      </Link>
    </div>
  );
}
