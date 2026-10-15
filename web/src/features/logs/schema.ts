import { z } from 'zod';

export const logLineSchema = z.object({
  timestamp: z.string().nullable(),
  level: z.string(),
  source: z.string(),
  target: z.string().nullable(),
  message: z.string(),
  runId: z.string().nullable(),
  ticketId: z.string().nullable(),
  stream: z.string().nullable(),
  fields: z.record(z.unknown()),
});

export const logPageSchema = z.object({
  lines: z.array(logLineSchema),
  nextCursor: z.string().nullable(),
});

export type LogLine = z.infer<typeof logLineSchema>;
export type LogPage = z.infer<typeof logPageSchema>;
