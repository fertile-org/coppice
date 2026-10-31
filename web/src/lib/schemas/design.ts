import { z } from 'zod';

export const designOptionSchema = z.object({
  id: z.string(),
  title: z.string(),
  entry: z.string(),
  label: z.string(),
  displayTitle: z.string(),
});

export const chosenDesignOptionSchema = z.object({
  id: z.string(),
  title: z.string(),
  label: z.string(),
  displayTitle: z.string(),
});

export const designChoiceSchema = z.object({
  mode: z.enum(['update_plan', 'save_only', 'plan_running', 'plan_final', 'read_only']),
  planOutOfDate: z.boolean(),
});

export const designSetSchema = z.object({
  id: z.string().uuid(),
  title: z.string(),
  runId: z.string().uuid(),
  options: z.array(designOptionSchema),
  chosen: chosenDesignOptionSchema.nullable(),
  recommended: chosenDesignOptionSchema.nullable(),
  selected: chosenDesignOptionSchema.nullable(),
  superseded: z.boolean(),
  fromPlanRun: z.boolean(),
  inherited: z.boolean(),
  choice: designChoiceSchema,
});

export const designSetListSchema = z.array(designSetSchema);

export type DesignOption = z.infer<typeof designOptionSchema>;
export type ChosenDesignOption = z.infer<typeof chosenDesignOptionSchema>;
export type DesignSet = z.infer<typeof designSetSchema>;
