import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  branchesResponseSchema,
  diffSummarySchema,
  fileContentSchema,
  filePatchSchema,
  submitReviewResponseSchema,
  worktreesListSchema,
  type BranchesResponse,
  type DiffSummary,
  type FileContent,
  type FilePatch,
  type SubmitReviewInput,
  type SubmitReviewResponse,
  type WorktreeSummary,
} from '../../lib/schemas/codeReview';
import { commentsQueryKey, ticketQueryKey } from '../tickets/useTicket';

export function repoWorktreesQueryKey(repoId: string) {
  return ['repo-worktrees', repoId] as const;
}

export function repoBranchesQueryKey(repoId: string) {
  return ['repo-branches', repoId] as const;
}

export function repoDiffQueryKey(
  repoId: string,
  worktreePath: string,
  baseBranch: string,
  ticketId?: string,
  ignoreWhitespace = false,
) {
  return [
    'repo-diff',
    repoId,
    worktreePath,
    baseBranch,
    ticketId ?? '',
    ignoreWhitespace,
  ] as const;
}

export function filePatchQueryKey(
  repoId: string,
  worktreePath: string,
  baseBranch: string,
  path: string,
  ignoreWhitespace = false,
) {
  return ['file-patch', repoId, worktreePath, baseBranch, path, ignoreWhitespace] as const;
}

export function fileContentQueryKey(
  repoId: string,
  worktreePath: string,
  baseBranch: string,
  path: string,
  side: 'head' | 'base',
) {
  return ['file-content', repoId, worktreePath, baseBranch, path, side] as const;
}

async function fetchRepoWorktrees(repoId: string): Promise<WorktreeSummary[]> {
  const res = await apiFetch(`/api/repos/${repoId}/worktrees`);
  const data = worktreesListSchema.parse(await res.json());
  return data.worktrees;
}

async function fetchRepoBranches(repoId: string): Promise<BranchesResponse> {
  const res = await apiFetch(`/api/repos/${repoId}/branches`);
  return branchesResponseSchema.parse(await res.json());
}

async function fetchRepoDiff(
  repoId: string,
  worktreePath: string,
  baseBranch: string,
  ticketId?: string,
  ignoreWhitespace = false,
): Promise<DiffSummary> {
  const params = new URLSearchParams({ worktreePath, baseBranch });
  if (ticketId) params.set('ticketId', ticketId);
  if (ignoreWhitespace) params.set('ignoreWhitespace', 'true');
  const res = await apiFetch(`/api/repos/${repoId}/diff?${params}`);
  return diffSummarySchema.parse(await res.json());
}

async function fetchFilePatch(
  repoId: string,
  worktreePath: string,
  baseBranch: string,
  path: string,
  ignoreWhitespace = false,
): Promise<FilePatch> {
  const params = new URLSearchParams({ worktreePath, baseBranch, path });
  if (ignoreWhitespace) params.set('ignoreWhitespace', 'true');
  const res = await apiFetch(`/api/repos/${repoId}/diff/file?${params}`);
  return filePatchSchema.parse(await res.json());
}

async function fetchFileContent(
  repoId: string,
  worktreePath: string,
  baseBranch: string,
  path: string,
  side: 'head' | 'base',
): Promise<FileContent> {
  const params = new URLSearchParams({ worktreePath, baseBranch, path, side });
  const res = await apiFetch(`/api/repos/${repoId}/diff/file-content?${params}`);
  return fileContentSchema.parse(await res.json());
}

async function postSubmitCodeReview(
  input: SubmitReviewInput,
): Promise<SubmitReviewResponse> {
  const res = await apiFetch('/api/code-reviews/submit', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  return submitReviewResponseSchema.parse(await res.json());
}

export function useRepoWorktrees(repoId: string | undefined) {
  return useQuery({
    queryKey: repoWorktreesQueryKey(repoId ?? ''),
    queryFn: () => fetchRepoWorktrees(repoId!),
    enabled: Boolean(repoId),
  });
}

export function useRepoBranches(repoId: string | undefined) {
  return useQuery({
    queryKey: repoBranchesQueryKey(repoId ?? ''),
    queryFn: () => fetchRepoBranches(repoId!),
    enabled: Boolean(repoId),
  });
}

export function useRepoDiff(
  repoId: string | undefined,
  worktreePath: string | undefined,
  baseBranch: string | undefined,
  ticketId?: string,
  ignoreWhitespace = false,
) {
  return useQuery({
    queryKey: repoDiffQueryKey(
      repoId ?? '',
      worktreePath ?? '',
      baseBranch ?? '',
      ticketId,
      ignoreWhitespace,
    ),
    queryFn: () =>
      fetchRepoDiff(repoId!, worktreePath!, baseBranch!, ticketId, ignoreWhitespace),
    enabled: Boolean(repoId && worktreePath && baseBranch),
  });
}

export function useFilePatch(
  repoId: string | undefined,
  worktreePath: string | undefined,
  baseBranch: string | undefined,
  path: string | undefined,
  ignoreWhitespace = false,
) {
  return useQuery({
    queryKey: filePatchQueryKey(
      repoId ?? '',
      worktreePath ?? '',
      baseBranch ?? '',
      path ?? '',
      ignoreWhitespace,
    ),
    queryFn: () =>
      fetchFilePatch(repoId!, worktreePath!, baseBranch!, path!, ignoreWhitespace),
    enabled: Boolean(repoId && worktreePath && baseBranch && path),
  });
}

export function useFileContent(
  repoId: string | undefined,
  worktreePath: string | undefined,
  baseBranch: string | undefined,
  path: string | undefined,
  side: 'head' | 'base',
) {
  return useQuery({
    queryKey: fileContentQueryKey(
      repoId ?? '',
      worktreePath ?? '',
      baseBranch ?? '',
      path ?? '',
      side,
    ),
    queryFn: () =>
      fetchFileContent(repoId!, worktreePath!, baseBranch!, path!, side),
    enabled: Boolean(repoId && worktreePath && baseBranch && path),
  });
}

export function useSubmitCodeReview() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: SubmitReviewInput) => postSubmitCodeReview(input),
    onSuccess: (data) => {
      void queryClient.invalidateQueries({
        queryKey: commentsQueryKey(data.ticketId),
      });
      void queryClient.invalidateQueries({
        queryKey: ticketQueryKey(data.ticketId),
      });
    },
  });
}
