import { z } from 'zod';

export const connectorConsoleSchema = z.enum([
  'openCodeSession',
  'structured',
  'plain',
]);

export type ConnectorConsole = z.infer<typeof connectorConsoleSchema>;

export const connectorSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  // Unknown kinds fall back so one new connector cannot break the whole list.
  console: connectorConsoleSchema.catch('plain'),
  caps: z.object({
    readOnlyTools: z.boolean(),
    chatResume: z.boolean(),
  }),
});

export type Connector = z.infer<typeof connectorSchema>;

export const connectorListSchema = z.object({
  items: z.array(connectorSchema),
});
