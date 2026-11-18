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

/**
 * What a connector's output says about token usage. Facts about its format:
 * a count left out here is never reported by that connector ("—"), unlike a
 * run that did not report a count ("not reported").
 */
export const usageCoverageSchema = z.object({
  granularity: z.enum(['request', 'turn', 'runTotalOnly']),
  model: z.enum(['perRequest', 'perModelTotal', 'runOnly', 'none']),
  fields: z.object({
    input: z.boolean(),
    cacheRead: z.boolean(),
    cacheWrite: z.boolean(),
    output: z.boolean(),
    reasoning: z.boolean(),
  }),
  subagents: z.enum(['breakdown', 'totalOnly', 'none']),
  contextWindow: z.enum(['withSize', 'usedOnly', 'none']),
});

export type UsageCoverage = z.infer<typeof usageCoverageSchema>;

export const connectorSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  // Unknown kinds fall back so one new connector cannot break the whole list.
  console: connectorConsoleSchema.catch('plain'),
  caps: z.object({
    readOnlyTools: z.boolean(),
    chatResume: z.boolean(),
    thinkingLevel: z.boolean().optional(),
  }),
  // `null`: Coppice does not read this connector's usage. A shape this
  // version does not know is treated the same, not as an error.
  usageCoverage: usageCoverageSchema.nullish().catch(null),
  enabled: z.boolean().optional(),
  readiness: connectorReadinessSchema.nullish(),
  docsUrl: z.string().optional(),
});

export type Connector = z.infer<typeof connectorSchema>;

export const connectorListSchema = z.object({
  items: z.array(connectorSchema),
});
