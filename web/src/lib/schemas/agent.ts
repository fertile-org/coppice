import { z } from 'zod';

/** One scale for every connector; each adapter maps it to its CLI's flag. */
export const thinkingLevelSchema = z.enum(['low', 'medium', 'high', 'max']);

export type ThinkingLevel = z.infer<typeof thinkingLevelSchema>;

export const createAgentSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  presetId: z.string().uuid().optional(),
  role: z.string().optional(),
  responsibilities: z.array(z.string()).optional(),
  systemPrompt: z.string().optional(),
  connector: z.string().optional(),
  modelProvider: z.string().optional(),
  model: z.string().optional(),
  thinkingLevel: thinkingLevelSchema.optional(),
  enabled: z.boolean().optional(),
});

export type CreateAgentInput = z.infer<typeof createAgentSchema>;

export const updateAgentSchema = z.object({
  name: z.string().min(1, 'Name is required').optional(),
  role: z.string().optional(),
  responsibilities: z.array(z.string()).optional(),
  systemPrompt: z.string().optional(),
  connector: z.string().optional(),
  modelProvider: z.string().nullable().optional(),
  model: z.string().nullable().optional(),
  thinkingLevel: thinkingLevelSchema.nullable().optional(),
  enabled: z.boolean().optional(),
});

export type UpdateAgentInput = z.infer<typeof updateAgentSchema>;

export const createUserSchema = z.object({
  email: z.string().email('Valid email is required'),
  password: z.string().min(1, 'Password is required'),
});

export type CreateUserInput = z.infer<typeof createUserSchema>;
