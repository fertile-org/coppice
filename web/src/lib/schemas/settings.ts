import { z } from 'zod';
import { idSchema } from './id';

export const readOnlyConnectorSchema = z.object({
  id: z.string(),
  displayName: z.string(),
});

export const knowledgeSettingsSchema = z.object({
  compactionAgentId: idSchema.nullable(),
  compactionAgent: z
    .object({
      id: idSchema,
      name: z.string(),
      enabled: z.boolean(),
      connector: z.string(),
    })
    .nullable(),
  readOnlyConnectors: z.array(readOnlyConnectorSchema),
});

export type KnowledgeSettings = z.infer<typeof knowledgeSettingsSchema>;
