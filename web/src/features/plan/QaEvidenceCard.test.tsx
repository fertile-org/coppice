import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { ApprovedPlan } from '../board/useTickets';
import { PLAN_COPY } from './copy';
import { QaEvidenceCard } from './QaEvidenceCard';

const planWithEvidence: ApprovedPlan = {
  planId: 'p1',
  markdown: '## Plan',
  steps: ['Disable Save'],
  criteriaCommitSha: 'abcdef0123456789',
  criteria: [
    {
      text: 'Save button is disabled while saving',
      status: 'met',
      screenshots: ['c0-s0.png', 'c0-s1.png', 'c0-s2.png', 'c0-s3.png'],
      steps: ['open', 'click', 'wait', 'assert'],
    },
    {
      text: 'Unit tests pass',
      status: 'met',
      evidence: 'make web-test',
    },
    {
      text: 'Works on Safari',
      status: 'not_checked',
      evidence: 'not checked: no WebKit on host',
    },
  ],
};

describe('QaEvidenceCard', () => {
  it('hides when there is no stored criteria commit', () => {
    const { container } = render(
      <QaEvidenceCard
        ticketId="t1"
        plan={{ planId: 'p1', markdown: '', steps: [], criteria: planWithEvidence.criteria }}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('shows badge, short commit, thumbs capped at 3, and opens the lightbox', () => {
    render(<QaEvidenceCard ticketId="t1" plan={planWithEvidence} />);

    const card = screen.getByTestId('qa-evidence-card');
    expect(within(card).getByText(PLAN_COPY.qaEvidence)).toBeVisible();
    expect(within(card).getByText(PLAN_COPY.qaEvidenceMetBadge(2, 3))).toBeVisible();
    expect(within(card).getByText(PLAN_COPY.qaEvidenceCheckedCommit('abcdef0'))).toBeVisible();
    expect(
      within(card).getByRole('button', { name: PLAN_COPY.qaEvidenceViewScreenshots(4) }),
    ).toBeVisible();
    expect(within(card).getByText(PLAN_COPY.qaEvidenceMore(1))).toBeVisible();

    const thumbs = within(card).getAllByRole('button', {
      name: /Screenshot:/,
    });
    expect(thumbs).toHaveLength(3);

    fireEvent.click(thumbs[0]!);
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeVisible();
    expect(within(dialog).getByText('Save button is disabled while saving')).toBeVisible();
    expect(within(dialog).getByAltText('open')).toHaveAttribute(
      'src',
      '/api/tickets/t1/qa-evidence/c0-s0.png',
    );
  });

  it('keeps the same statuses PlanChecklist would show', () => {
    render(<QaEvidenceCard ticketId="t1" plan={planWithEvidence} />);
    const rows = screen.getAllByRole('listitem');
    expect(rows[0]).toHaveTextContent('✓');
    expect(rows[1]).toHaveTextContent('✓');
    expect(rows[2]).toHaveTextContent('–');
    expect(rows[2]).toHaveTextContent('not checked: no WebKit on host');
  });
});
