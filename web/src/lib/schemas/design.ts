import { z } from 'zod';

export const designOptionSchema = z.object({
  id: z.string(),
  title: z.string(),
  entry: z.string(),
});

export const chosenDesignOptionSchema = z.object({
  id: z.string(),
  title: z.string(),
});

export const designSetSchema = z.object({
  id: z.string().uuid(),
  title: z.string(),
  runId: z.string().uuid(),
  options: z.array(designOptionSchema),
  chosen: chosenDesignOptionSchema.nullable(),
});

export const designSetListSchema = z.array(designSetSchema);

export type DesignOption = z.infer<typeof designOptionSchema>;
export type ChosenDesignOption = z.infer<typeof chosenDesignOptionSchema>;
export type DesignSet = z.infer<typeof designSetSchema>;
