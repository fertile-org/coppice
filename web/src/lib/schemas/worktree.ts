import { z } from 'zod';
import { idSchema } from './id';

export const worktreeAdminEntrySchema = z.object({
  folderName: z.string(),
  path: z.string(),
  repoId: idSchema.nullable(),
  repoName: z.string().nullable(),
  ticketId: idSchema.nullable(),
  ticketTitle: z.string().nullable(),
  ticketStatus: z.string().nullable(),
  archived: z.boolean(),
  merged: z.boolean(),
  hasUncommittedChanges: z.boolean().nullable(),
  hasActiveRun: z.boolean(),
  lastUsedAt: z.string(),
  sizeBytes: z.number().nullable(),
  finished: z.boolean(),
});

export type WorktreeAdminEntry = z.infer<typeof worktreeAdminEntrySchema>;

export const worktreeListSchema = z.object({
  worktrees: z.array(worktreeAdminEntrySchema),
});

export const removeStatusSchema = z.enum([
  'removed',
  'active_run',
  'not_found',
  'error',
]);

export const removeResultSchema = z.object({
  folderName: z.string(),
  status: removeStatusSchema,
  message: z.string().optional(),
});

export const removeResponseSchema = z.object({
  results: z.array(removeResultSchema),
});

export type RemoveResult = z.infer<typeof removeResultSchema>;
