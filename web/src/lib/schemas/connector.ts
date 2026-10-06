import { z } from 'zod';

export const connectorConsoleSchema = z.enum([
  'openCodeSession',
  'structured',
  'plain',
]);

export type ConnectorConsole = z.infer<typeof connectorConsoleSchema>;

export const connectorReadinessSchema = z.enum([
  'ready',
  'found_not_signed_in',
  'not_on_path',
  'found',
]);

export const connectorSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  // Unknown kinds fall back so one new connector cannot break the whole list.
  console: connectorConsoleSchema.catch('plain'),
  caps: z.object({
    readOnlyTools: z.boolean(),
    chatResume: z.boolean(),
  }),
  enabled: z.boolean().optional(),
  readiness: connectorReadinessSchema.nullish(),
  docsUrl: z.string().optional(),
});

export type Connector = z.infer<typeof connectorSchema>;

export const connectorListSchema = z.object({
  items: z.array(connectorSchema),
});
