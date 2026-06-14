import type { InlineComment } from '../../lib/schemas/codeReview';

export function formatReviewPreview(
  repoName: string,
  worktreePath: string,
  baseBranch: string,
  headBranch: string,
  headSha: string,
  summary: string,
  inlineComments: InlineComment[],
): string {
  let body = `## Code review\n\n**Repo:** ${repoName} · **Worktree:** ${worktreePath} · **Compare:** ${baseBranch} → ${headBranch} (${headSha})\n\n### Summary\n${summary}\n`;
  if (inlineComments.length === 0) {
    return body;
  }

  body += '\n### Inline comments\n';

  const byPath = new Map<string, InlineComment[]>();
  for (const comment of inlineComments) {
    const existing = byPath.get(comment.path) ?? [];
    existing.push(comment);
    byPath.set(comment.path, existing);
  }

  for (const path of [...byPath.keys()].sort()) {
    body += `\n#### \`${path}\`\n`;
    for (const comment of byPath.get(path)!) {
      body += `- **L${comment.line}** (${comment.side}): ${comment.body.trim()}\n`;
    }
  }

  return body;
}
