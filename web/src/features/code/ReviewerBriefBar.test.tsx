import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { REVIEWER_BRIEF_COPY } from './reviewerBriefCopy';
import { ReviewerBriefBar } from './ReviewerBriefBar';

describe('ReviewerBriefBar', () => {
  it('shows the three fields and the none placeholder when the plan did not change', () => {
    render(
      <ReviewerBriefBar
        brief={{
          howItWorks: 'Reads the header.',
          changesFromPlan: '  ',
          lookHereFirst: 'src/parser.rs',
        }}
      />,
    );

    const section = screen.getByTestId('reviewer-brief');
    expect(section).toHaveAccessibleName(REVIEWER_BRIEF_COPY.section);
    expect(screen.getByText(REVIEWER_BRIEF_COPY.howItWorks)).toBeVisible();
    expect(screen.getByText('Reads the header.')).toBeVisible();
    expect(screen.getByText(REVIEWER_BRIEF_COPY.changesFromPlan)).toBeVisible();
    expect(screen.getByText(REVIEWER_BRIEF_COPY.none)).toBeVisible();
    expect(screen.getByText(REVIEWER_BRIEF_COPY.lookHereFirst)).toBeVisible();
    expect(screen.getByText('src/parser.rs')).toBeVisible();
  });

  it('shows the agent text when the plan did change', () => {
    render(
      <ReviewerBriefBar
        brief={{
          howItWorks: 'Same path.',
          changesFromPlan: 'Dropped the cache.',
          lookHereFirst: 'src/lib.rs',
        }}
      />,
    );
    expect(screen.getByText('Dropped the cache.')).toBeVisible();
    expect(screen.queryByText(REVIEWER_BRIEF_COPY.none)).toBeNull();
  });
});
