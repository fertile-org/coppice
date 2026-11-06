import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter, useSearchParams } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { selectComboboxOption } from '../../test/combobox';
import { ThemeProvider } from '../theme/ThemeProvider';
import { CodeReviewPage } from './CodeReviewPage';

const state = vi.hoisted(() => ({
  verificationStatus: 'ready',
  ticket: null as null | {
    id: string;
    boardId: string;
    title: string;
    status: 'in_progress';
  },
  diff: null as null | {
    baseBranch: string;
    baseSha: string;
    headSha: string;
    headBranch: string;
    files: [];
  },
  worktrees: [] as {
    path: string;
    branch: string;
    headSha: string;
    ticketId: null;
    ticketTitle: null;
  }[],
}));

vi.mock('../repos/useRepos', () => ({
  useRepos: () => ({
    data: [
      {
        id: 'repo-1',
        name: 'coppice',
        verificationStatus: state.verificationStatus,
        defaultBranch: 'main',
      },
    ],
  }),
}));

vi.mock('../tickets/useTicket', () => ({
  useTicket: () => ({ data: state.ticket ?? undefined }),
}));

vi.mock('./SubmitReviewDialog', () => ({
  SubmitReviewDialog: () => null,
}));

vi.mock('./useCodeReview', () => ({
  useRepoWorktrees: () => ({ data: state.worktrees, isLoading: false }),
  useRepoBranches: () => ({
    data: { defaultBranch: 'main', branches: ['main', 'develop'] },
  }),
  useRepoDiff: () => ({ data: state.diff ?? undefined, isLoading: false }),
  useFilePatch: () => ({
    data: undefined,
    isLoading: true,
    isError: false,
    error: null,
  }),
  useFileContent: () => ({
    data: undefined,
    isLoading: true,
    isError: false,
  }),
}));

function SearchProbe() {
  const [params] = useSearchParams();
  return <output data-testid="search">{params.toString()}</output>;
}

function renderPage(entry: string) {
  return render(
    <ThemeProvider>
      <MemoryRouter initialEntries={[entry]}>
        <CodeReviewPage />
        <SearchProbe />
      </MemoryRouter>
    </ThemeProvider>,
  );
}

describe('Code review header', () => {
  beforeEach(() => {
    state.verificationStatus = 'ready';
    state.ticket = null;
    state.diff = null;
    state.worktrees = [];
  });

  it('renders the Code review eyebrow exactly once', () => {
    renderPage('/code?repoId=repo-1');
    expect(screen.getAllByText('Code review', { exact: true })).toHaveLength(1);
    expect(
      screen.getByRole('heading', {
        level: 1,
        name: 'New review — ticket created on submit',
      }),
    ).toBeVisible();
    expect(screen.queryByText(/^Compare /)).toBeNull();
  });

  it('links the heading to the ticket', () => {
    state.ticket = {
      id: 'ticket-1',
      boardId: 'board-1',
      title: 'Diff review is hard to review the code',
      status: 'in_progress',
    };
    renderPage('/code?repoId=repo-1&ticketId=ticket-1');

    const heading = screen.getByRole('link', {
      name: 'Diff review is hard to review the code',
    });
    expect(heading).toHaveAttribute('href', '/boards/board-1?ticket=ticket-1');
    expect(heading).toHaveAttribute(
      'title',
      'Diff review is hard to review the code',
    );
    expect(screen.getAllByText('Code review', { exact: true })).toHaveLength(1);
  });

  it('shows a loading heading while the ticket is missing', () => {
    renderPage('/code?repoId=repo-1&ticketId=ticket-1');
    expect(screen.getByRole('heading', { level: 1, name: 'Loading ticket…' })).toBeVisible();
    expect(screen.queryByRole('link', { name: 'Loading ticket…' })).toBeNull();
  });

  it('shows both branches and 7-character SHAs on the merge line', () => {
    state.worktrees = [
      {
        path: '/trees/alpha',
        branch: 'agent/TICKET-3f5422e9-coppice',
        headSha: 'cfe5792bbbb',
        ticketId: null,
        ticketTitle: null,
      },
    ];
    state.diff = {
      baseBranch: 'main',
      baseSha: '79e6dc4aaaa',
      headSha: 'cfe5792bbbb',
      headBranch: 'agent/TICKET-3f5422e9-coppice',
      files: [],
    };
    renderPage('/code?repoId=repo-1&worktree=/trees/alpha&baseBranch=main');

    expect(screen.getByTitle('agent/TICKET-3f5422e9-coppice')).toHaveTextContent(
      'agent/TICKET-3f5422e9-coppice',
    );
    expect(screen.getByTitle('main')).toHaveTextContent('main');
    expect(screen.getByText('cfe5792')).toBeVisible();
    expect(screen.getByText('79e6dc4')).toBeVisible();
    expect(screen.getByText('into')).toBeVisible();
    expect(screen.getByText('coppice')).toBeVisible();
  });

  it('omits the merge line until the diff summary exists', () => {
    state.worktrees = [
      {
        path: '/trees/alpha',
        branch: 'agent/one',
        headSha: 'cfe5792bbbb',
        ticketId: null,
        ticketTitle: null,
      },
    ];
    renderPage('/code?repoId=repo-1&worktree=/trees/alpha&baseBranch=main');
    expect(screen.queryByText('into')).toBeNull();
    expect(screen.queryByText('cfe5792')).toBeNull();
  });

  it('writes worktree and base picks into the URL', () => {
    state.worktrees = [
      {
        path: '/trees/alpha',
        branch: 'agent/one',
        headSha: 'aaaaaaaa',
        ticketId: null,
        ticketTitle: null,
      },
      {
        path: '/trees/beta',
        branch: 'agent/two',
        headSha: 'bbbbbbbb',
        ticketId: null,
        ticketTitle: null,
      },
    ];
    renderPage('/code?repoId=repo-1&worktree=/trees/alpha&baseBranch=main');

    selectComboboxOption(screen.getByRole('combobox', { name: 'Worktree' }), /beta/);
    selectComboboxOption(screen.getByRole('combobox', { name: 'Base branch' }), 'develop');

    const params = new URLSearchParams(screen.getByTestId('search').textContent ?? '');
    expect(params.get('worktree')).toBe('/trees/beta');
    expect(params.get('baseBranch')).toBe('develop');
  });

  it('uses Code review as the only heading when no repo is open', () => {
    renderPage('/code');
    expect(screen.getAllByText('Code review', { exact: true })).toHaveLength(1);
    expect(screen.getByRole('heading', { level: 1, name: 'Code review' })).toBeVisible();
  });

  it('uses the repo name as the heading when the repo is not ready', () => {
    state.verificationStatus = 'failed';
    renderPage('/code?repoId=repo-1');
    expect(screen.getAllByText('Code review', { exact: true })).toHaveLength(1);
    expect(screen.getByRole('heading', { level: 1, name: 'coppice' })).toHaveAttribute(
      'title',
      'coppice',
    );
  });
});
