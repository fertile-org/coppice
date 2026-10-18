import '@testing-library/jest-dom/vitest';
import { render, screen, within } from '@testing-library/react';
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

  it('lists criteria first and ticks only the met ones', () => {
    render(
      <PlanChecklist
        plan={{
          commentId: 'c1',
          markdown: '## Plan',
          steps: ['Write the gate'],
          criteria: [
            { text: 'Drag is refused', status: 'met', evidence: 'test plan_gate_refuses_drag' },
            { text: 'API returns 409', status: 'not_met', evidence: 'returns 400' },
            { text: 'Docs mention it', status: 'not_checked' },
          ],
        }}
      />,
    );
    const section = screen.getByRole('region', { name: PLAN_COPY.criteria });
    const boxes = within(section).getAllByRole('checkbox');
    expect(boxes.map((box) => (box as HTMLInputElement).checked)).toEqual([true, false, false]);
    expect(within(section).getByText(PLAN_COPY.criterionMet)).toBeVisible();
    expect(within(section).getByText(PLAN_COPY.criterionNotMet)).toBeVisible();
    expect(within(section).getByText(PLAN_COPY.criterionNotChecked)).toBeVisible();
    expect(screen.getByText('Drag is refused').closest('li')).toHaveAttribute(
      'title',
      'test plan_gate_refuses_drag',
    );
    expect(screen.getByText(PLAN_COPY.steps)).toBeVisible();
    expect(screen.getByText('Write the gate')).toBeVisible();
  });

  it('shows only steps when the plan has no criteria', () => {
    render(
      <PlanChecklist plan={{ commentId: 'c1', markdown: '## Plan', steps: ['One'], criteria: [] }} />,
    );
    expect(screen.queryByRole('region', { name: PLAN_COPY.criteria })).not.toBeInTheDocument();
    expect(screen.queryByText(PLAN_COPY.steps)).not.toBeInTheDocument();
    expect(screen.getByText('One')).toBeVisible();
  });
});
