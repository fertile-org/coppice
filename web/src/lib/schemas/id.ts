import { z } from 'zod';

/** UUID or 12-char `[0-9a-z]` entity id (matches server `Id`). */
export const idSchema = z.union([
  z.string().uuid(),
  z.string().regex(/^[0-9a-z]{12}$/),
]);

export type EntityId = z.infer<typeof idSchema>;
