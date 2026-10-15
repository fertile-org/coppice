import { describe, it, expect } from 'vitest';
import { BOARD_COLUMNS } from './columns';

describe('BOARD_COLUMNS', () => {
  it('has nine columns in spec order', () => {
    expect(BOARD_COLUMNS.map((c) => c.status)).toEqual([
      'backlog',
      'plan_review',
      'ready',
      'in_progress',
      'in_review',
      'in_qa',
      'wait_for_final_review',
      'done',
      'blocked',
    ]);
    expect(BOARD_COLUMNS.map((c) => c.label)).toEqual([
      'Backlog',
      'Plan Review',
      'Ready',
      'In Progress',
      'In Review',
      'In QA',
      'Wait for Human Review',
      'Done',
      'Blocked',
    ]);
  });
});
