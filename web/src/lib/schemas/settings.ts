import { z } from 'zod';

export const readOnlyConnectorSchema = z.object({
  id: z.string(),
  displayName: z.string(),
});

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
  readOnlyConnectors: z.array(readOnlyConnectorSchema),
});

export type KnowledgeSettings = z.infer<typeof knowledgeSettingsSchema>;
