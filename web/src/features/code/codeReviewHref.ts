export function codeReviewHref(
  ticketId: string,
  repoId: string,
  worktreePath: string | null,
): string {
  const params = new URLSearchParams({ repoId, ticketId });
  if (worktreePath) params.set('worktree', worktreePath);
  return `/code?${params.toString()}`;
}
