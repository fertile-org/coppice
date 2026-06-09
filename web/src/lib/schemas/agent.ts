import { z } from 'zod';

export const createAgentSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  presetId: z.string().uuid().optional(),
  role: z.string().optional(),
  skills: z.array(z.string()).optional(),
  responsibilities: z.array(z.string()).optional(),
  systemPrompt: z.string().optional(),
  provider: z.string().optional(),
  model: z.string().optional(),
  enabled: z.boolean().optional(),
});

export type CreateAgentInput = z.infer<typeof createAgentSchema>;

export const updateAgentSchema = z.object({
  name: z.string().min(1, 'Name is required').optional(),
  role: z.string().optional(),
  skills: z.array(z.string()).optional(),
  responsibilities: z.array(z.string()).optional(),
  systemPrompt: z.string().optional(),
  provider: z.string().optional(),
  model: z.string().optional(),
  enabled: z.boolean().optional(),
});

export type UpdateAgentInput = z.infer<typeof updateAgentSchema>;

export const createUserSchema = z.object({
  email: z.string().email('Valid email is required'),
  password: z.string().min(1, 'Password is required'),
});

export type CreateUserInput = z.infer<typeof createUserSchema>;
