import { z } from 'zod';
import { pluginTestResultSchema } from './plugin';

export const pluginBuildStatusSchema = z.enum([
  'drafting',
  'planning',
  'plan_ready',
  'building',
  'ready_to_try',
  'accepted',
  'failed',
  'cancelled',
  'discarded',
  'blocked',
]);

export type PluginBuildStatus = z.infer<typeof pluginBuildStatusSchema>;

export const pluginBuildPlanQuestionSchema = z.object({
  question: z.string(),
  choices: z.array(z.string()).optional().default([]),
});

export type PluginBuildPlanQuestion = z.infer<typeof pluginBuildPlanQuestionSchema>;

export const pluginBuildAdvancedSchema = z
  .object({
    pluginName: z.string().optional(),
    version: z.string().optional(),
    description: z.string().optional(),
    files: z.array(z.string()).optional(),
    skills: z
      .array(
        z.object({
          name: z.string(),
          description: z.string().optional(),
        }),
      )
      .optional(),
    servers: z.array(z.unknown()).optional(),
    settings: z.array(z.unknown()).optional(),
    permissions: z.array(z.string()).optional(),
    manifestPreview: z.string().optional(),
  })
  .passthrough();

export type PluginBuildAdvanced = z.infer<typeof pluginBuildAdvancedSchema>;

export const pluginBuildPlanSchema = z.object({
  revision: z.number().int(),
  body: z.string(),
  changeRequest: z.string().optional(),
  createdAt: z.string().optional(),
  advanced: pluginBuildAdvancedSchema.optional(),
});

export type PluginBuildPlan = z.infer<typeof pluginBuildPlanSchema>;

export const pluginPublishResultSchema = z.object({
  status: z.enum(['pushed', 'failed', 'skipped']),
  branch: z.string().optional().nullable(),
  error: z.string().optional().nullable(),
  suggestedBranch: z.string().optional().nullable(),
  url: z.string().optional().nullable(),
});

export type PluginPublishResult = z.infer<typeof pluginPublishResultSchema>;

export const pluginBuildSchema = z.object({
  id: z.string().uuid(),
  answers: z.record(z.string(), z.unknown()).default({}),
  plans: z.array(pluginBuildPlanSchema).default([]),
  status: z.string(),
  pluginName: z.string().nullable(),
  workspacePath: z.string(),
  publishTarget: z.string().nullable(),
  planQuestions: z.array(pluginBuildPlanQuestionSchema).nullable().optional(),
  checkErrors: z.array(z.string()).nullable().optional(),
  formatRetries: z.number().int().optional().default(0),
  pluginId: z.string().uuid().nullable().optional(),
  testResult: pluginTestResultSchema.nullable().optional(),
  activeRunId: z.string().uuid().nullable().optional(),
  lastRunId: z.string().uuid().nullable().optional(),
  lastRunStatus: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
  publish: pluginPublishResultSchema.optional().nullable(),
});

export type PluginBuild = z.infer<typeof pluginBuildSchema>;

export const pluginBuildRunStartSchema = z.object({
  build: pluginBuildSchema,
  runId: z.string().uuid(),
});

export type PluginBuildRunStart = z.infer<typeof pluginBuildRunStartSchema>;

function normalizeCheckErrors(raw: unknown): string[] | null {
  if (raw == null) return null;
  if (Array.isArray(raw)) {
    return raw.filter((item): item is string => typeof item === 'string');
  }
  return null;
}

function normalizeTestResult(raw: unknown) {
  if (raw == null) return null;
  const parsed = pluginTestResultSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}

/** Lenient parse: plans may arrive as a raw JSON array from the API. */
export function parsePluginBuild(data: unknown): PluginBuild {
  const raw = data as Record<string, unknown>;
  const plans = Array.isArray(raw.plans) ? raw.plans : [];
  const answers =
    raw.answers && typeof raw.answers === 'object' && !Array.isArray(raw.answers)
      ? (raw.answers as Record<string, unknown>)
      : {};
  let planQuestions = raw.planQuestions;
  if (planQuestions && !Array.isArray(planQuestions)) {
    const wrapped = planQuestions as { questions?: unknown };
    planQuestions = Array.isArray(wrapped.questions) ? wrapped.questions : null;
  }
  const publishRaw = raw.publish;
  const publish =
    publishRaw && typeof publishRaw === 'object'
      ? pluginPublishResultSchema.safeParse(publishRaw).success
        ? pluginPublishResultSchema.parse(publishRaw)
        : null
      : null;
  return pluginBuildSchema.parse({
    ...raw,
    answers,
    plans,
    planQuestions: planQuestions ?? null,
    checkErrors: normalizeCheckErrors(raw.checkErrors),
    testResult: normalizeTestResult(raw.testResult),
    activeRunId: raw.activeRunId ?? null,
    lastRunId: raw.lastRunId ?? null,
    lastRunStatus: raw.lastRunStatus ?? null,
    pluginId: raw.pluginId ?? null,
    publish,
  });
}

/** Empty servers (skills-only) counts as a pass; any `error` fails. */
export function pluginBuildTestPassed(
  result: PluginBuild['testResult'] | null | undefined,
): boolean {
  if (!result) return false;
  return result.servers.every((server) => server.status !== 'error');
}
