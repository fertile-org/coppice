import { describe, expect, it } from 'vitest';
import { designSetSchema } from './design';

describe('designSetSchema', () => {
  it('defaults missing kind to ui', () => {
    const parsed = designSetSchema.parse({
      id: '11111111-1111-4111-8111-111111111111',
      title: 'Landing',
      runId: '22222222-2222-4222-8222-222222222222',
      options: [],
      chosen: null,
      recommended: null,
      selected: null,
      superseded: false,
      fromPlanRun: true,
      inherited: false,
      choice: { mode: 'update_plan', planOutOfDate: false },
    });
    expect(parsed.kind).toBe('ui');
  });
});
