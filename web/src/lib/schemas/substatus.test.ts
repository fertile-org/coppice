import { describe, expect, it } from 'vitest';
import { substatusMetadataSchema } from './substatus';

describe('substatusMetadataSchema', () => {
  it('requires agentId for waiting_for_agent', () => {
    const r = substatusMetadataSchema.safeParse({
      substatus: 'waiting_for_agent',
      metadata: {},
    });
    expect(r.success).toBe(false);
  });

  it('accepts valid waiting_for_agent metadata', () => {
    const r = substatusMetadataSchema.safeParse({
      substatus: 'waiting_for_agent',
      metadata: { agentId: '550e8400-e29b-41d4-a716-446655440000' },
    });
    expect(r.success).toBe(true);
  });

  it('requires capability for blocked_by_missing_capability', () => {
    const r = substatusMetadataSchema.safeParse({
      substatus: 'blocked_by_missing_capability',
      metadata: {},
    });
    expect(r.success).toBe(false);
  });

  it('requires secretKey for blocked_by_missing_secret', () => {
    const r = substatusMetadataSchema.safeParse({
      substatus: 'blocked_by_missing_secret',
      metadata: {},
    });
    expect(r.success).toBe(false);
  });
});
