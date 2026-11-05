import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import { ThemeProvider } from '../theme/ThemeProvider';
import { CodeReviewPage } from './CodeReviewPage';

const state = vi.hoisted(() => ({
  headSha: 'aaaaaaaa',
  files: [
    {
      path: 'web/src/components/ui/button.tsx',
      status: 'modified',
      additions: 6,
      deletions: 3,
    },
    {
      path: 'web/src/styles/tokens.css',
      status: 'modified',
      additions: 22,
      deletions: 8,
    },
    {
      path: 'web/src/legacy/darkButtons.css',
      status: 'deleted',
      additions: 0,
      deletions: 31,
    },
  ],
}));

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

vi.mock('./SubmitReviewDialog', () => ({
  SubmitReviewDialog: () => null,
}));

vi.mock('./useCodeReview', () => ({
  useRepoWorktrees: () => ({
    data: [
      {
        path: '/wt',
        branch: 'agent/review',
        headSha: state.headSha,
        ticketId: null,
        ticketTitle: null,
      },
    ],
    isLoading: false,
  }),
  useRepoBranches: () => ({ data: { defaultBranch: 'main', branches: ['main'] } }),
  useRepoDiff: () => ({
    data: {
      baseBranch: 'main',
      baseSha: 'bbbbbbbb',
      headSha: state.headSha,
      headBranch: 'agent/review',
      files: state.files,
    },
    isLoading: false,
  }),
  useFilePatch: () => ({
    data: undefined,
    isLoading: true,
    isError: false,
    error: null,
  }),
}));

function renderPage() {
  return render(
    <ThemeProvider>
      <MemoryRouter initialEntries={['/code?repoId=repo-1&worktree=/wt&baseBranch=main']}>
        <KeyboardProvider>
          <CodeReviewPage />
        </KeyboardProvider>
      </MemoryRouter>
    </ThemeProvider>,
  );
}

describe('code review file header and viewed files', () => {
  beforeEach(() => {
    localStorage.clear();
    state.headSha = 'aaaaaaaa';
  });

  it('shows the path, counts, and position in a header that stays outside the diff scroll', () => {
    renderPage();
    const header = screen.getByTestId('file-review-header');
    expect(header).toHaveClass('sticky');
    expect(header).toHaveTextContent('web/src/components/ui/');
    expect(header).toHaveTextContent('button.tsx');
    expect(header).toHaveTextContent('+6');
    expect(header).toHaveTextContent('−3');
    expect(header).toHaveTextContent('1 / 3');
    expect(header.parentElement?.querySelector('.overflow-auto')?.contains(header)).toBe(
      false,
    );
    expect(screen.getByRole('button', { name: 'Previous file' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Next file' })).toBeEnabled();
  });

  it('keeps viewed files across reload and clears them when the head SHA changes', () => {
    const first = renderPage();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Viewed' }));
    expect(screen.getByText('1 of 3 viewed')).toBeVisible();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '1');
    expect(screen.getAllByText('✓')).toHaveLength(1);
    expect(screen.getByRole('checkbox', { name: 'Viewed' })).toBeChecked();
    first.unmount();

    const second = renderPage();
    expect(screen.getByRole('checkbox', { name: 'Viewed' })).toBeChecked();
    expect(screen.getByText('1 of 3 viewed')).toBeVisible();

    state.headSha = 'bbbbbbbb';
    second.rerender(
      <ThemeProvider>
        <MemoryRouter initialEntries={['/code?repoId=repo-1&worktree=/wt&baseBranch=main']}>
          <KeyboardProvider>
            <CodeReviewPage />
          </KeyboardProvider>
        </MemoryRouter>
      </ThemeProvider>,
    );
    expect(screen.getByRole('checkbox', { name: 'Viewed' })).not.toBeChecked();
    expect(screen.getByText('0 of 3 viewed')).toBeVisible();
  });

  it('moves with j and k, marks viewed with v, and ignores those keys in a textarea', () => {
    renderPage();
    fireEvent.keyDown(document.body, { key: 'j' });
    expect(screen.getByTestId('file-review-header')).toHaveTextContent('tokens.css');
    expect(screen.getByTestId('file-review-header')).toHaveTextContent('2 / 3');

    fireEvent.keyDown(document.body, { key: 'k' });
    expect(screen.getByTestId('file-review-header')).toHaveTextContent('button.tsx');

    fireEvent.keyDown(document.body, { key: 'v' });
    expect(screen.getByTestId('file-review-header')).toHaveTextContent('tokens.css');
    expect(screen.getByText('1 of 3 viewed')).toBeVisible();
    expect(screen.getByRole('checkbox', { name: 'Viewed' })).not.toBeChecked();

    const box = document.createElement('textarea');
    document.body.append(box);
    fireEvent.keyDown(box, { key: 'j' });
    fireEvent.keyDown(box, { key: 'v' });
    expect(screen.getByTestId('file-review-header')).toHaveTextContent('tokens.css');
    expect(screen.getByText('1 of 3 viewed')).toBeVisible();
    box.remove();

    fireEvent.keyDown(document.body, { key: '?' });
    const help = screen.getByRole('dialog');
    expect(help).toHaveTextContent('Next file');
    expect(help).toHaveTextContent('Previous file');
    expect(help).toHaveTextContent('Mark viewed');
  });
});
