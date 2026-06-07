import { describe, it, expect } from 'vitest';
import { BOARD_COLUMNS } from './columns';

describe('BOARD_COLUMNS', () => {
  it('has eight columns in spec order', () => {
    expect(BOARD_COLUMNS.map((c) => c.status)).toEqual([
      'backlog',
      'ready',
      'in_progress',
      'in_review',
      'in_qa',
      'wait_for_final_review',
      'done',
      'blocked',
    ]);
  });
});
