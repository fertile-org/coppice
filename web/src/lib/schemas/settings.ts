import { z } from 'zod';

export const knowledgeSettingsSchema = z.object({
  compactionAgentId: z.string().uuid().nullable(),
  compactionAgent: z
    .object({
      id: z.string().uuid(),
      name: z.string(),
      enabled: z.boolean(),
      connector: z.string(),
    })
    .nullable(),
  readOnlyConnectors: z.array(z.string()),
});

export type KnowledgeSettings = z.infer<typeof knowledgeSettingsSchema>;
