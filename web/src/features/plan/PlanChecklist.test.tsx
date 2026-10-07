import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { PLAN_COPY } from './copy';
import { PlanChecklist } from './PlanChecklist';

describe('PlanChecklist', () => {
  it('shows the approved steps as read-only boxes', () => {
    render(
      <PlanChecklist
        plan={{
          commentId: 'c1',
          markdown: '## Plan',
          steps: ['Add the column', 'Gate In Progress'],
        }}
      />,
    );

    expect(screen.getByRole('complementary', { name: PLAN_COPY.approvedPlan })).toBeVisible();
    const boxes = screen.getAllByRole('checkbox');
    expect(boxes).toHaveLength(2);
    for (const box of boxes) {
      expect(box).toBeDisabled();
      expect(box).not.toBeChecked();
    }
    expect(screen.getByText('Add the column')).toBeVisible();
    expect(screen.getByText('Gate In Progress')).toBeVisible();
  });

  it('says when the plan has no steps', () => {
    render(
      <PlanChecklist plan={{ commentId: 'c1', markdown: '## Plan', steps: [] }} />,
    );
    expect(screen.getByText(PLAN_COPY.noSteps)).toBeVisible();
  });
});
