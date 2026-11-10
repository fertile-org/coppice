import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Repo } from '../../lib/schemas/repo';
import { RepoGitFlowSection } from './RepoDrawer';

const mocks = vi.hoisted(() => ({ update: vi.fn() }));

vi.mock('./useRepos', () => ({
  useUpdateRepo: () => ({ mutateAsync: mocks.update, isPending: false }),
}));

const repo: Repo = {
  id: '00000000-0000-4000-8000-000000000001',
  name: 'coppice',
  localPath: '/repos/coppice',
  remoteUrl: 'https://github.com/org/coppice.git',
  defaultBranch: 'main',
  verificationStatus: 'ready',
  verificationError: null,
  lastVerifiedAt: null,
  forgeTokenConfigured: false,
  mergeAfterAccept: false,
  pushAfterMerge: false,
  pushAfterMergeDisabledReason: null,
  createdAt: '2026-09-04T00:00:00Z',
  updatedAt: '2026-09-04T00:00:00Z',
};

describe('RepoGitFlowSection', () => {
  beforeEach(() => {
    mocks.update.mockReset();
    mocks.update.mockResolvedValue(repo);
  });

  it('renders both switches off by default', () => {
    render(<RepoGitFlowSection repo={repo} />);
    expect(screen.getByText('When a ticket is accepted')).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: 'Merge after accept' })).toHaveAttribute(
      'aria-checked',
      'false',
    );
    const push = screen.getByRole('switch', { name: 'Push after merge' });
    expect(push).toHaveAttribute('aria-checked', 'false');
    expect(push).toBeEnabled();
    expect(
      screen.getByText(
        "Merge the ticket branch into main when you accept the ticket.",
      ),
    ).toBeInTheDocument();
  });

  it('disables the push switch and shows the reason', () => {
    render(
      <RepoGitFlowSection
        repo={{ ...repo, pushAfterMergeDisabledReason: 'git.push_enabled is false in server config' }}
      />,
    );
    expect(screen.getByRole('switch', { name: 'Push after merge' })).toBeDisabled();
    expect(screen.getByText('git.push_enabled is false in server config')).toBeInTheDocument();
  });

  it('saves a toggled switch', async () => {
    render(<RepoGitFlowSection repo={repo} />);
    fireEvent.click(screen.getByRole('switch', { name: 'Merge after accept' }));
    await waitFor(() =>
      expect(mocks.update).toHaveBeenCalledWith({ id: repo.id, mergeAfterAccept: true }),
    );
  });
});
