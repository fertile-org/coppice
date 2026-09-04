import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Repo } from '../../lib/schemas/repo';
import { DefaultBranchSyncControls } from './DefaultBranchSyncControls';
import type { DefaultBranchSyncStatus } from './useRepos';

const { toastSuccess, toastError } = vi.hoisted(() => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock('../../components/ToastProvider', () => ({
  useToast: () => ({ success: toastSuccess, error: toastError }),
}));

vi.mock('./useRepos', async () => {
  const actual = await vi.importActual<typeof import('./useRepos')>('./useRepos');
  return {
    ...actual,
    useDefaultBranchSync: () => ({
      data: undefined,
      isLoading: false,
    }),
    useFetchDefaultBranch: () => ({
      mutateAsync: vi.fn(),
      isPending: false,
    }),
    usePushDefaultBranch: () => ({
      mutateAsync: vi.fn(),
      isPending: false,
    }),
  };
});

const repo: Repo = {
  id: 'repo-1',
  name: 'demo',
  localPath: '/repos/demo',
  remoteUrl: 'https://github.com/org/demo.git',
  defaultBranch: 'main',
  verificationStatus: 'ready',
  verificationError: null,
  lastVerifiedAt: '2026-09-04T00:00:00Z',
  forgeTokenConfigured: true,
  createdAt: '2026-09-04T00:00:00Z',
  updatedAt: '2026-09-04T00:00:00Z',
};

function baseStatus(
  overrides: Partial<DefaultBranchSyncStatus> = {},
): DefaultBranchSyncStatus {
  return {
    defaultBranch: 'main',
    localSha: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    remoteSha: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
    aheadCount: 1,
    behindCount: 0,
    workingTreeClean: true,
    pushEnabled: true,
    forgeTokenConfigured: true,
    canFetch: true,
    canPush: true,
    fetchDisabledReason: null,
    pushDisabledReason: null,
    ...overrides,
  };
}

describe('DefaultBranchSyncControls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('enables Fetch and Push when status allows both', () => {
    render(
      <DefaultBranchSyncControls repo={repo} statusOverride={baseStatus()} />,
    );

    expect(screen.getByRole('button', { name: 'Fetch' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Push to remote' })).toBeEnabled();
    expect(screen.getByText(/ahead 1, behind 0/)).toBeVisible();
  });

  it('disables Push and surfaces pushDisabledReason when canPush is false', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          canPush: false,
          pushDisabledReason: 'git.push_enabled is false in server config',
          aheadCount: 0,
        })}
      />,
    );

    const push = screen.getByRole('button', { name: 'Push to remote' });
    expect(push).toBeDisabled();
    expect(push).toHaveAttribute(
      'title',
      'git.push_enabled is false in server config',
    );
    expect(
      screen.getByText('git.push_enabled is false in server config'),
    ).toBeVisible();
    expect(screen.getByRole('button', { name: 'Fetch' })).toBeEnabled();
  });

  it('disables Fetch when canFetch is false and shows reason', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          canFetch: false,
          canPush: false,
          fetchDisabledReason: 'Set a forge token in Settings → Repositories',
          pushDisabledReason: 'Set a forge token in Settings → Repositories',
        })}
      />,
    );

    const fetchBtn = screen.getByRole('button', { name: 'Fetch' });
    expect(fetchBtn).toBeDisabled();
    expect(fetchBtn).toHaveAttribute(
      'title',
      'Set a forge token in Settings → Repositories',
    );
  });

  it('confirms before pushing', async () => {
    const onPush = vi.fn();
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus()}
        onPush={onPush}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Push to remote' }));
    expect(confirm).toHaveBeenCalled();
    expect(onPush).toHaveBeenCalled();
    confirm.mockRestore();
  });
});
