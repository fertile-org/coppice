import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FlaggedByAgents, KnowledgeFreshness } from './KnowledgeFreshness';

const { useKnowledgeFreshness } = vi.hoisted(() => ({
  useKnowledgeFreshness: vi.fn(),
}));

vi.mock('./useKnowledge', () => ({ useKnowledgeFreshness }));

const ITEM_ID = '00000000-0000-4000-8000-000000000001';

describe('FlaggedByAgents', () => {
  it('shows the count when agents flagged the item', () => {
    render(<FlaggedByAgents count={2} />);
    expect(screen.getByText('Flagged by agents (2)')).toBeVisible();
  });

  it('shows nothing without flags', () => {
    const { container } = render(<FlaggedByAgents count={0} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe('KnowledgeFreshness', () => {
  beforeEach(() => {
    useKnowledgeFreshness.mockReset();
    useKnowledgeFreshness.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: false,
    });
  });

  it('does not load until expanded', () => {
    render(<KnowledgeFreshness itemId={ITEM_ID} />);
    expect(useKnowledgeFreshness).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: /Cited code/ })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
  });

  it('lists cited paths with their state once expanded', () => {
    useKnowledgeFreshness.mockReturnValue({
      data: {
        state: 'missing',
        citations: [
          { path: 'src/a.rs', repoName: 'coppice', commitSha: 'a'.repeat(40) },
          { path: 'src/b.rs', repoName: 'coppice', commitSha: null },
        ],
      },
      isLoading: false,
      isError: false,
    });
    render(<KnowledgeFreshness itemId={ITEM_ID} />);
    fireEvent.click(screen.getByRole('button', { name: /Cited code/ }));

    expect(useKnowledgeFreshness).toHaveBeenCalledWith(ITEM_ID);
    expect(screen.getByText('src/a.rs')).toBeVisible();
    expect(screen.getByText('src/b.rs')).toBeVisible();
    expect(screen.getByTestId('freshness-state')).toHaveTextContent('Missing');
    expect(screen.getByTestId('freshness-state').className).toContain('text-danger');
  });

  it('styles a changed state as a warning', () => {
    useKnowledgeFreshness.mockReturnValue({
      data: {
        state: 'changed',
        citations: [{ path: 'src/a.rs', repoName: 'coppice', commitSha: 'b'.repeat(40) }],
      },
      isLoading: false,
      isError: false,
    });
    render(<KnowledgeFreshness itemId={ITEM_ID} />);
    fireEvent.click(screen.getByRole('button', { name: /Cited code/ }));
    expect(screen.getByTestId('freshness-state')).toHaveTextContent('Changed');
    expect(screen.getByTestId('freshness-state').className).toContain('text-warning');
  });

  it('says so when the item cites no code', () => {
    useKnowledgeFreshness.mockReturnValue({
      data: { state: 'uncited', citations: [] },
      isLoading: false,
      isError: false,
    });
    render(<KnowledgeFreshness itemId={ITEM_ID} />);
    fireEvent.click(screen.getByRole('button', { name: /Cited code/ }));
    expect(screen.getByText('This item cites no code.')).toBeVisible();
  });

  it('shows a load error', () => {
    useKnowledgeFreshness.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
    });
    render(<KnowledgeFreshness itemId={ITEM_ID} />);
    fireEvent.click(screen.getByRole('button', { name: /Cited code/ }));
    expect(screen.getByText('Unable to check the cited code.')).toBeVisible();
  });
});
