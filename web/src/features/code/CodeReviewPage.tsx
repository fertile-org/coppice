import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../components/ui/select';
import { useRepos } from '../repos/useRepos';
import { useTicket } from '../tickets/useTicket';
import { ChangedFilesPanel } from './ChangedFilesPanel';
import { DiffViewer, type InlineCommentDraft } from './DiffViewer';
import { SubmitReviewDialog } from './SubmitReviewDialog';
import {
  useRepoBranches,
  useRepoDiff,
  useRepoWorktrees,
} from './useCodeReview';

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
  const [viewType, setViewType] = useState<'split' | 'unified'>('split');
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

  const { data: diffSummary, isLoading: diffLoading } = useRepoDiff(
    repoId,
    selectedWorktreePath,
    baseBranch,
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

  const ticketBoardHref = ticket
    ? `/projects/${ticket.projectId}/board?ticket=${ticket.id}`
    : undefined;

  if (!repoId) {
    return (
      <PageShell>
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
      <PageShell repoName={repo.name}>
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

  return (
    <div className="coppice-grain flex h-screen flex-col bg-background">
      <header className="shrink-0 border-b border-border bg-surface px-6 py-3">
        <div className="flex flex-wrap items-center gap-4">
          <Link to="/settings/repositories" className="flex items-center gap-2">
            <img
              src="/logo.webp"
              srcSet="/logo.webp 1x, /logo@2x.webp 2x"
              alt="Coppice"
              width={28}
              height={28}
              className="h-7 w-7 shrink-0"
            />
            <span className="font-display text-lg font-semibold text-text-primary">
              Code review
            </span>
          </Link>

          <div className="hidden h-6 w-px bg-border sm:block" aria-hidden="true" />

          <div className="flex min-w-0 flex-wrap items-center gap-3">
            <div className="font-body text-sm text-text-secondary">
              <span className="text-text-muted">Repo</span>{' '}
              <span className="font-medium text-text-primary">
                {repo?.name ?? '…'}
              </span>
            </div>

            <Select
              value={selectedWorktreePath ?? ''}
              onValueChange={(value) => syncParams({ worktree: value })}
              disabled={!worktrees?.length}
            >
              <SelectTrigger className="w-[min(100vw-3rem,20rem)]">
                <SelectValue placeholder="Select worktree…" />
              </SelectTrigger>
              <SelectContent>
                {(worktrees ?? []).map((worktree) => (
                  <SelectItem
                    key={worktree.path}
                    value={worktree.path}
                    textValue={worktree.path}
                  >
                    {worktreeLabel(
                      worktree.path,
                      worktree.branch,
                      worktree.ticketTitle,
                    )}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Select
              value={baseBranch}
              onValueChange={(value) => syncParams({ baseBranch: value })}
              disabled={!branchesData?.branches.length}
            >
              <SelectTrigger className="w-40">
                <SelectValue placeholder="Base branch" />
              </SelectTrigger>
              <SelectContent>
                {(branchesData?.branches ?? [defaultBranch]).map((branch) => (
                  <SelectItem key={branch} value={branch} textValue={branch}>
                    {branch}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <div className="flex rounded-md border border-border bg-surface-raised p-0.5">
              <button
                type="button"
                onClick={() => setViewType('split')}
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
                onClick={() => setViewType('unified')}
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
          </div>

          <div className="ml-auto flex items-center gap-3">
            {ticketId ? (
              ticketBoardHref ? (
                <Link
                  to={ticketBoardHref}
                  className="max-w-xs truncate font-body text-sm text-accent hover:underline"
                >
                  Ticket: {ticket?.title ?? ticketId}
                </Link>
              ) : (
                <span className="font-body text-sm text-text-secondary">
                  Loading ticket…
                </span>
              )
            ) : (
              <span className="font-body text-sm text-text-muted">
                No ticket — will create on submit
              </span>
            )}

            <Button
              type="button"
              disabled={!selectedWorktreePath || !headSha}
              onClick={() => setSubmitOpen(true)}
            >
              Submit review
            </Button>
          </div>
        </div>

        {diffSummary && (
          <p className="mt-2 font-body text-xs text-text-muted">
            Compare {diffSummary.baseBranch} ({diffSummary.baseSha.slice(0, 7)})
            → {headBranch} ({headSha.slice(0, 7)})
          </p>
        )}
      </header>

      {!worktreesLoading && worktrees?.length === 0 ? (
        <EmptyState
          title="No worktrees found"
          description="Create a ticket worktree or register a repository with active worktrees."
          actionHref="/settings/repositories"
          actionLabel="Go to Repositories"
        />
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(220px,280px)_1fr]">
          <ChangedFilesPanel
            files={diffSummary?.files ?? []}
            selectedPath={selectedFile}
            onSelect={setSelectedFile}
            isLoading={diffLoading}
          />
          {repoId && selectedWorktreePath && (
            <DiffViewer
              repoId={repoId}
              worktreePath={selectedWorktreePath}
              baseBranch={baseBranch}
              filePath={selectedFile}
              viewType={viewType}
              inlineComments={inlineComments}
              onInlineCommentsChange={setInlineComments}
            />
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
}: {
  children: ReactNode;
  repoName?: string;
}) {
  return (
    <div className="coppice-grain flex h-screen flex-col bg-background">
      <header className="border-b border-border bg-surface px-6 py-3">
        <div className="flex items-center gap-3">
          <Link to="/settings/repositories" className="flex items-center gap-2">
            <img
              src="/logo.webp"
              alt="Coppice"
              width={28}
              height={28}
              className="h-7 w-7"
            />
            <span className="font-display text-lg font-semibold text-text-primary">
              Code review
            </span>
          </Link>
          {repoName && (
            <span className="font-body text-sm text-text-secondary">
              {repoName}
            </span>
          )}
        </div>
      </header>
      <div className="flex flex-1 items-center justify-center p-8">{children}</div>
    </div>
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
      <h1 className="font-display text-xl font-semibold text-bark-900">
        {title}
      </h1>
      <p className="mt-2 font-body text-sm text-text-secondary">{description}</p>
      <Link
        to={actionHref}
        className="mt-4 inline-flex rounded-md bg-accent px-4 py-2 font-body text-sm font-medium text-white hover:bg-accent-hover"
      >
        {actionLabel}
      </Link>
    </div>
  );
}
