import type { DesignSet } from '../../lib/schemas/design';

/** A published set with three options; override fields per test. */
export function makeDesignSet(overrides: Partial<DesignSet> = {}): DesignSet {
  return {
    id: '11111111-1111-4111-8111-111111111111',
    title: 'Landing page',
    runId: '22222222-2222-4222-8222-222222222222',
    options: [
      { id: 'a', title: 'Quiet button after the last card', entry: 'a/index.html' },
      { id: 'b', title: 'Full-width button with Show all', entry: 'b/index.html' },
      { id: 'c', title: 'Footer pinned to the viewport', entry: 'c/index.html' },
    ],
    chosen: null,
    recommended: { id: 'a', title: 'Quiet button after the last card' },
    selected: { id: 'a', title: 'Quiet button after the last card' },
    superseded: false,
    fromPlanRun: true,
    inherited: false,
    choice: { mode: 'update_plan', planOutOfDate: false },
    ...overrides,
  };
}
