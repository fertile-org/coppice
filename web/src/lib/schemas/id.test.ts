import { describe, expect, it } from 'vitest';
import { idSchema } from './id';

describe('idSchema', () => {
  it('accepts uuid and lowercase short ids', () => {
    expect(idSchema.safeParse('550e8400-e29b-41d4-a716-446655440000').success).toBe(true);
    expect(idSchema.safeParse('550E8400-E29B-41D4-A716-446655440000').success).toBe(true);
    expect(idSchema.safeParse('abc123xyz789').success).toBe(true);
  });

  it('rejects uppercase short ids and junk', () => {
    expect(idSchema.safeParse('ABC123XYZ789').success).toBe(false);
    expect(idSchema.safeParse('not-an-id').success).toBe(false);
    expect(idSchema.safeParse('').success).toBe(false);
  });
});
