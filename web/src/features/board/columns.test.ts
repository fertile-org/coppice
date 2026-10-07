import { describe, it, expect } from 'vitest';
import { BOARD_COLUMNS } from './columns';

describe('BOARD_COLUMNS', () => {
  it('has nine columns in spec order', () => {
    expect(BOARD_COLUMNS.map((c) => c.status)).toEqual([
      'backlog',
      'ready',
      'plan_review',
      'in_progress',
      'in_review',
      'in_qa',
      'wait_for_final_review',
      'done',
      'blocked',
    ]);
    expect(BOARD_COLUMNS.map((c) => c.label)).toEqual([
      'Backlog',
      'Ready',
      'Plan Review',
      'In Progress',
      'In Review',
      'In QA',
      'Wait for Human Review',
      'Done',
      'Blocked',
    ]);
  });
});
