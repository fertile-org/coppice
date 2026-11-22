import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ThemeProvider } from '../theme/ThemeProvider';
import { CodeReviewPage } from './CodeReviewPage';

vi.mock('../repos/useRepos', () => ({
  useRepos: () => ({
    data: [
      {
        id: 'repo-1',
        name: 'coppice',
        verificationStatus: 'ready',
        defaultBranch: 'main',
      },
    ],
  }),
}));

vi.mock('../tickets/useTicket', () => ({
  useTicket: () => ({ data: undefined }),
}));

const diffCalls = vi.hoisted(() => ({ ignoreWhitespace: false }));

vi.mock('./useCodeReview', () => ({
  useRepoWorktrees: () => ({ data: [], isLoading: false }),
  useRepoBranches: () => ({ data: { defaultBranch: 'main', branches: ['main'] } }),
  useRepoDiff: (
    _repoId: string | undefined,
    _worktree: string | undefined,
    _base: string | undefined,
    _ticketId?: string,
    ignoreWhitespace = false,
  ) => {
    diffCalls.ignoreWhitespace = ignoreWhitespace;
    return { data: undefined, isLoading: false };
  },
}));

function renderPage() {
  return render(
    <ThemeProvider>
      <MemoryRouter initialEntries={['/code?repoId=repo-1']}>
        <CodeReviewPage />
      </MemoryRouter>
    </ThemeProvider>,
  );
}

describe('Code review view mode', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('keeps Split or Unified after a reload', () => {
    const first = renderPage();
    fireEvent.click(screen.getByRole('button', { name: 'Unified' }));
    expect(localStorage.getItem('coppice.codeReview.viewType')).toBe('unified');
    first.unmount();

    renderPage();
    expect(localStorage.getItem('coppice.codeReview.viewType')).toBe('unified');
  });

  it('sends hide whitespace to the diff', () => {
    renderPage();
    const box = screen.getByRole('checkbox', { name: 'Hide whitespace' });
    expect(box).not.toBeChecked();
    expect(diffCalls.ignoreWhitespace).toBe(false);

    fireEvent.click(box);
    expect(box).toBeChecked();
    expect(diffCalls.ignoreWhitespace).toBe(true);
  });
});
