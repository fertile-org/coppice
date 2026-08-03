import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { KnowledgeUsed } from './KnowledgeUsed';

const openTicket = vi.fn();

vi.mock('./useKnowledge', () => ({
  useKnowledgeUsed: () => ({
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
        sourceType: 'ticket',
        sourceId: '00000000-0000-4000-8000-000000000003',
        includedAt: '2026-08-03T12:00:00Z',
      },
    ],
    isLoading: false,
    isError: false,
  }),
}));

describe('KnowledgeUsed', () => {
  it('shows the exact revision audit and links ticket provenance', () => {
    render(
      <KnowledgeUsed
        runId="00000000-0000-4000-8000-000000000010"
        enabled
        onOpenTicket={openTicket}
      />,
    );

    expect(screen.getByText('Knowledge Used')).toBeVisible();
    expect(screen.getByText(/18 tokens · similarity 0.912/)).toBeVisible();
    fireEvent.click(screen.getByText('Exact rendered revision'));
    expect(screen.getByText(/Do the safe thing/)).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Source ticket' }));
    expect(openTicket).toHaveBeenCalledWith(
      '00000000-0000-4000-8000-000000000003',
    );
  });
});
