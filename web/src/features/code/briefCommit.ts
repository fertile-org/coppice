/** True when the brief was written for the commit Human Review is bound to. */
export function briefMatchesReviewedCommit(
  briefSha: string | null | undefined,
  reviewedSha: string | null | undefined,
): boolean {
  const brief = briefSha?.trim().toLowerCase() ?? '';
  const reviewed = reviewedSha?.trim().toLowerCase() ?? '';
  return brief.length > 0 && brief === reviewed;
}
