import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { KnowledgeUsed } from './KnowledgeUsed';

const { openTicket, useKnowledgeUsed } = vi.hoisted(() => ({
  openTicket: vi.fn(),
  useKnowledgeUsed: vi.fn(),
}));

vi.mock('./useKnowledge', () => ({
  useKnowledgeUsed,
}));

describe('KnowledgeUsed', () => {
  it('shows loading without also claiming the audit is empty', () => {
    useKnowledgeUsed.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    });

    render(
      <KnowledgeUsed runId="00000000-0000-4000-8000-000000000010" enabled onOpenTicket={openTicket} />,
    );

    expect(screen.getByText('Loading knowledge audit…')).toBeVisible();
    expect(screen.queryByText(/did not include stored knowledge/)).toBeNull();
  });

  it('shows the audit error state', () => {
    useKnowledgeUsed.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
    });

    render(
      <KnowledgeUsed runId="00000000-0000-4000-8000-000000000010" enabled onOpenTicket={openTicket} />,
    );

    expect(screen.getByText('Unable to load the knowledge audit.')).toBeVisible();
  });

  it('shows the explicit empty audit state', () => {
    useKnowledgeUsed.mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    });

    render(
      <KnowledgeUsed runId="00000000-0000-4000-8000-000000000010" enabled onOpenTicket={openTicket} />,
    );

    expect(screen.getByText('This run did not include stored knowledge.')).toBeVisible();
  });

  it('shows ranked immutable revisions and links only ticket-backed provenance', () => {
    useKnowledgeUsed.mockReturnValue({
      data: [
        {
          itemId: '00000000-0000-4000-8000-000000000001',
          revisionId: '00000000-0000-4000-8000-000000000002',
          rank: 1,
          similarity: 0.91234,
          tokenCount: 18,
          renderedContent: '<knowledge revision="exact">Do the safe thing.</knowledge>',
          title: 'Safe deployment',
          knowledgeType: 'operational_runbook',
          scope: 'project',
          sourceType: 'agent_summary',
          sourceId: '00000000-0000-4000-8000-000000000003',
          includedAt: '2026-08-03T12:00:00Z',
        },
        {
          itemId: '00000000-0000-4000-8000-000000000004',
          revisionId: '00000000-0000-4000-8000-000000000005',
          rank: 2,
          similarity: 0.8,
          tokenCount: 12,
          renderedContent: '<knowledge revision="comment">Review this.</knowledge>',
          title: 'Review note',
          knowledgeType: 'review_feedback',
          scope: 'project',
          sourceType: 'comment',
          sourceId: '00000000-0000-4000-8000-000000000006',
          includedAt: '2026-08-03T12:00:00Z',
        },
      ],
      isLoading: false,
      isError: false,
    });

    render(
      <KnowledgeUsed
        runId="00000000-0000-4000-8000-000000000010"
        enabled
        onOpenTicket={openTicket}
      />,
    );

    expect(screen.getByText('Knowledge Used')).toBeVisible();
    expect(screen.getByText('1. Safe deployment')).toBeVisible();
    expect(screen.getByText('2. Review note')).toBeVisible();
    expect(screen.getByText(/18 tokens · similarity 0.912/)).toBeVisible();
    expect(screen.getByText('00000000-0000-4000-8000-000000000002')).toBeVisible();
    expect(screen.getByText('00000000-0000-4000-8000-000000000005')).toBeVisible();
    expect(screen.getByText(/Agent Summary/)).toBeVisible();
    expect(screen.getByText('00000000-0000-4000-8000-000000000003')).toBeVisible();
    expect(screen.getByText(/Comment/)).toBeVisible();
    expect(screen.getByText('00000000-0000-4000-8000-000000000006')).toBeVisible();
    fireEvent.click(screen.getAllByText('Exact rendered revision')[0]);
    expect(screen.getByText(/Do the safe thing/)).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Source ticket' }));
    expect(openTicket).toHaveBeenCalledWith(
      '00000000-0000-4000-8000-000000000003',
    );
    expect(screen.getAllByRole('button', { name: 'Source ticket' })).toHaveLength(1);
  });
});
