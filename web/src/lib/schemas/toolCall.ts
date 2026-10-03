import { z } from 'zod';

export const toolCallSourceSchema = z.enum(['core', 'skill', 'plugin']);

export type ToolCallSource = z.infer<typeof toolCallSourceSchema>;

export const toolCallStatusSchema = z.enum(['ok', 'error', 'denied', 'timeout']);

export type ToolCallStatus = z.infer<typeof toolCallStatusSchema>;

export const runToolCallSchema = z.object({
  id: z.string().uuid(),
  tool: z.string(),
  source: toolCallSourceSchema,
  pluginId: z.string().uuid().nullable(),
  pluginName: z.string().nullable(),
  status: toolCallStatusSchema,
  error: z.string().nullable(),
  durationMs: z.number().int().nonnegative(),
  argsSummary: z.string(),
  createdAt: z.string(),
});

export type RunToolCall = z.infer<typeof runToolCallSchema>;

export const runToolCallsSchema = z.object({
  items: z.array(runToolCallSchema),
  skillsUsed: z.array(z.string()),
});

export type RunToolCalls = z.infer<typeof runToolCallsSchema>;
