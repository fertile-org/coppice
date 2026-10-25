import { z } from 'zod';

export const marketplaceSourceSchema = z.object({
  id: z.string().uuid(),
  name: z.string(),
  kind: z.enum(['builtin', 'git']),
  gitUrl: z.string().nullable(),
  gitRef: z.string().nullable(),
  commitSha: z.string().nullable(),
  refreshedAt: z.string().nullable(),
  lastError: z.string().nullable(),
  position: z.number().int(),
});

export type MarketplaceSource = z.infer<typeof marketplaceSourceSchema>;

export const marketplaceEntrySchema = z.object({
  sourceId: z.string().uuid(),
  sourceName: z.string(),
  name: z.string(),
  description: z.string(),
  version: z.string(),
  kind: z.string(),
  installable: z.boolean(),
  reason: z.string().nullish(),
  category: z.string().nullish(),
  installedPluginId: z.string().uuid().nullable(),
  updateAvailable: z.boolean(),
});

export type MarketplaceEntry = z.infer<typeof marketplaceEntrySchema>;

/** Path entries answer with the plugin; object entries answer with an install job. */
export function isPathEntryKind(kind: string): boolean {
  return kind === 'path';
}
