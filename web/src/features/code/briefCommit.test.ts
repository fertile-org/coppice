import { describe, expect, it } from 'vitest';
import { briefMatchesReviewedCommit } from './briefCommit';

describe('briefMatchesReviewedCommit', () => {
  it('matches the reviewed commit ignoring case and surrounding space', () => {
    expect(
      briefMatchesReviewedCommit(
        ' ABC1234deadbeef ',
        'abc1234deadbeef',
      ),
    ).toBe(true);
  });

  it('hides the brief when the reviewed commit differs or is missing', () => {
    expect(
      briefMatchesReviewedCommit('abc1234deadbeef', 'fff0000deadbeef'),
    ).toBe(false);
    expect(briefMatchesReviewedCommit('abc1234deadbeef', '')).toBe(false);
    expect(briefMatchesReviewedCommit('abc1234deadbeef', null)).toBe(false);
    expect(briefMatchesReviewedCommit('', 'abc1234deadbeef')).toBe(false);
  });
});
