import { describe, expect, it } from 'vitest';
import { boardRefetchInterval } from './useTickets';

describe('boardRefetchInterval', () => {
  it('relies on the event socket while it is open', () => {
    expect(boardRefetchInterval([{ hasActiveRun: true }], true)).toBe(false);
  });

  it('polls every 3 s only while the socket is down and a run is live', () => {
    expect(boardRefetchInterval([{ hasActiveRun: true }], false)).toBe(3000);
    expect(boardRefetchInterval([{ hasActiveRun: false }], false)).toBe(false);
    expect(boardRefetchInterval(undefined, false)).toBe(false);
  });
});
