import { z } from 'zod';

export const worktreeSummarySchema = z.object({
  path: z.string(),
  branch: z.string(),
  headSha: z.string(),
  ticketId: z.string().uuid().nullable(),
  ticketTitle: z.string().nullable(),
});

export type WorktreeSummary = z.infer<typeof worktreeSummarySchema>;

export const worktreesListSchema = z.object({
  worktrees: z.array(worktreeSummarySchema),
});

export const branchesResponseSchema = z.object({
  defaultBranch: z.string(),
  branches: z.array(z.string()),
});

export type BranchesResponse = z.infer<typeof branchesResponseSchema>;

export const diffFileSummarySchema = z.object({
  path: z.string(),
  status: z.string(),
  additions: z.number(),
  deletions: z.number(),
});

export type DiffFileSummary = z.infer<typeof diffFileSummarySchema>;

export const diffSummarySchema = z.object({
  baseBranch: z.string(),
  baseSha: z.string(),
  headSha: z.string(),
  headBranch: z.string(),
  files: z.array(diffFileSummarySchema),
});

export type DiffSummary = z.infer<typeof diffSummarySchema>;

export const filePatchSchema = z.object({
  path: z.string(),
  patch: z.string(),
});

export type FilePatch = z.infer<typeof filePatchSchema>;

export const inlineCommentSchema = z.object({
  path: z.string(),
  line: z.number(),
  side: z.enum(['old', 'new', 'delete']),
  body: z.string(),
});

export type InlineComment = z.infer<typeof inlineCommentSchema>;

export const submitReviewSchema = z.object({
  repoId: z.string().uuid(),
  worktreePath: z.string(),
  baseBranch: z.string(),
  headSha: z.string(),
  ticketId: z.string().uuid().nullable().optional(),
  newTicket: z
    .object({
      projectId: z.string().uuid(),
      title: z.string().min(1),
      description: z.string().optional(),
    })
    .nullable()
    .optional(),
  summary: z.string().min(1),
  inlineComments: z.array(inlineCommentSchema),
  workflowAction: z
    .enum(['none', 'move_to_in_progress', 'reassign_engineer'])
    .optional(),
  reassignAgentId: z.string().uuid().optional(),
});

export type SubmitReviewInput = z.infer<typeof submitReviewSchema>;

export const submitReviewResponseSchema = z.object({
  ticketId: z.string().uuid(),
  commentId: z.string().uuid(),
  ticketCreated: z.boolean(),
});

export type SubmitReviewResponse = z.infer<typeof submitReviewResponseSchema>;
