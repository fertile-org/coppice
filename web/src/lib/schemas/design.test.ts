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
      fromChat: false,
      inherited: false,
      choice: { mode: 'update_plan', planOutOfDate: false },
    });
    expect(parsed.kind).toBe('ui');
  });

  it('keeps technical option fields and recommendedReason', () => {
    const parsed = designSetSchema.parse({
      id: '11111111-1111-4111-8111-111111111111',
      title: 'Event delivery',
      kind: 'architecture',
      runId: '22222222-2222-4222-8222-222222222222',
      options: [
        {
          id: 'a',
          title: 'LISTEN/NOTIFY',
          entry: '',
          label: 'A',
          displayTitle: 'LISTEN/NOTIFY',
          changes: 'events.rs',
          effort: 'S',
          risk: 'Low',
          reversibility: 'Easy',
          rulesOut: 'Multi-node',
          sketch: 'flowchart LR\n  A --> B',
        },
      ],
      chosen: null,
      recommended: {
        id: 'a',
        title: 'LISTEN/NOTIFY',
        label: 'A',
        displayTitle: 'LISTEN/NOTIFY',
      },
      recommendedReason: 'Fits bundled Postgres.',
      selected: null,
      superseded: false,
      fromPlanRun: true,
      fromChat: false,
      inherited: false,
      choice: { mode: 'update_plan', planOutOfDate: false },
    });
    expect(parsed.kind).toBe('architecture');
    expect(parsed.recommendedReason).toBe('Fits bundled Postgres.');
    expect(parsed.options[0]?.sketch).toContain('flowchart');
    expect(parsed.options[0]?.rulesOut).toBe('Multi-node');
  });
});
