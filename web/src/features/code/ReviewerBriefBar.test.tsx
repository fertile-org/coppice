import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
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
    expect(section).toHaveAccessibleName('Reviewer brief');
    expect(
      screen.getByText(
        "The agent wrote these notes when it finished. Coppice tags any file that wasn't in the plan you approved.",
      ),
    ).toBeVisible();
    expect(screen.getByText('Reads the header.')).toBeVisible();
    expect(screen.getByText('Changes from the plan')).toBeVisible();
    expect(screen.getByText('None')).toBeVisible();
    expect(screen.getByText('Look here first')).toBeVisible();
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
    expect(screen.queryByText('None')).toBeNull();
  });
});
