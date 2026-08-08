import { z } from 'zod';

export const verificationStatusSchema = z.enum([
  'ready',
  'path_missing',
  'not_git_repo',
  'error',
]);

export type VerificationStatus = z.infer<typeof verificationStatusSchema>;

export const repoSchema = z.object({
  id: z.string().uuid(),
  name: z.string(),
  localPath: z.string(),
  remoteUrl: z.string().nullable(),
  defaultBranch: z.string(),
  verificationStatus: verificationStatusSchema,
  verificationError: z.string().nullable(),
  lastVerifiedAt: z.string().nullable(),
  forgeTokenConfigured: z.boolean().optional().default(false),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export type Repo = z.infer<typeof repoSchema>;

export const createRepoSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  localPath: z.string().min(1, 'Local path is required'),
  remoteUrl: z.string().optional(),
  defaultBranch: z.string().min(1, 'Default branch is required').default('main'),
});

export type CreateRepoInput = z.infer<typeof createRepoSchema>;

export const updateRepoSchema = z.object({
  name: z.string().min(1, 'Name is required').optional(),
  localPath: z.string().min(1, 'Local path is required').optional(),
  remoteUrl: z.string().nullable().optional(),
  defaultBranch: z.string().min(1, 'Default branch is required').optional(),
});

export type UpdateRepoInput = z.infer<typeof updateRepoSchema>;
