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
    usePullDefaultBranch: () => ({
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
    canPull: false,
    fetchDisabledReason: null,
    pushDisabledReason: null,
    pullDisabledReason: 'Local default branch is not behind remote',
    ...overrides,
  };
}

const DIRTY_PULL =
  'Main repository has uncommitted changes — commit or stash before pulling';
const DIRTY_PUSH =
  'Main repository has uncommitted changes — commit or stash before pushing';
const NO_REMOTE = 'Set repository remote URL in Settings → Repositories';

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
    expect(screen.getByRole('button', { name: 'Pull' })).toBeDisabled();
    expect(screen.getByText(/ahead 1, behind 0/)).toBeVisible();
  });

  it('enables Pull from server canPull and disables Push when behind', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          aheadCount: 0,
          behindCount: 2,
          canPush: false,
          canPull: true,
          pushDisabledReason:
            'Local default branch is behind or diverged from remote — use Pull when strictly behind, or resolve divergence outside Coppice',
          pullDisabledReason: null,
        })}
      />,
    );

    expect(screen.getByRole('button', { name: 'Pull' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Push to remote' })).toBeDisabled();
    expect(
      screen.getByText(/use Pull when strictly behind/),
    ).toBeVisible();
  });

  it('shows one uncommitted-changes line for a dirty tree', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          workingTreeClean: false,
          canPull: false,
          canPush: false,
          pullDisabledReason: DIRTY_PULL,
          pushDisabledReason: DIRTY_PUSH,
        })}
      />,
    );

    const pull = screen.getByRole('button', { name: 'Pull' });
    expect(pull).toBeDisabled();
    expect(pull).toHaveAttribute('title', DIRTY_PULL);
    expect(screen.getByRole('button', { name: 'Push to remote' })).toHaveAttribute(
      'title',
      DIRTY_PUSH,
    );
    expect(screen.getAllByText(/uncommitted changes/)).toHaveLength(1);
    expect(screen.getByText(/working tree dirty/)).toBeVisible();
  });

  it('keeps the push-disabled reason next to the dirty-tree line', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          workingTreeClean: false,
          pushEnabled: false,
          canPull: false,
          canPush: false,
          pullDisabledReason: DIRTY_PULL,
          pushDisabledReason: 'git.push_enabled is false in server config',
        })}
      />,
    );

    expect(screen.getAllByText(/uncommitted changes/)).toHaveLength(1);
    expect(
      screen.getByText('git.push_enabled is false in server config'),
    ).toBeVisible();
  });

  it('does not repeat not-behind / not-ahead when in sync', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          aheadCount: 0,
          behindCount: 0,
          canPush: false,
          pushDisabledReason: 'Local default branch is not ahead of remote',
        })}
      />,
    );

    expect(screen.getByText('in sync with remote')).toBeVisible();
    expect(screen.queryByText(/not behind remote/)).toBeNull();
    expect(screen.queryByText(/not ahead of remote/)).toBeNull();
    expect(screen.getByRole('button', { name: 'Pull' })).toHaveAttribute(
      'title',
      'Local default branch is not behind remote',
    );
  });

  it('shows "Fetch remote first" once when there is no remote ref', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          remoteSha: null,
          aheadCount: null,
          behindCount: null,
          canPush: false,
          canPull: false,
          pushDisabledReason: 'Fetch remote first',
          pullDisabledReason: 'Fetch remote first',
        })}
      />,
    );

    expect(screen.getAllByText('Fetch remote first')).toHaveLength(1);
  });

  it('shows the missing remote URL reason once', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          remoteSha: null,
          aheadCount: null,
          behindCount: null,
          canFetch: false,
          canPush: false,
          canPull: false,
          fetchDisabledReason: NO_REMOTE,
          pushDisabledReason: NO_REMOTE,
          pullDisabledReason: NO_REMOTE,
        })}
      />,
    );

    expect(screen.getAllByText(NO_REMOTE)).toHaveLength(1);
  });

  it('shows both hints when diverged', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          aheadCount: 1,
          behindCount: 1,
          canPush: false,
          canPull: false,
          pullDisabledReason:
            'Local default branch has diverged from remote — refuse pull without merge or rebase',
          pushDisabledReason:
            'Local default branch is behind or diverged from remote — use Pull when strictly behind, or resolve divergence outside Coppice',
        })}
      />,
    );

    expect(screen.getByText(/has diverged from remote/)).toBeVisible();
    expect(screen.getByText(/use Pull when strictly behind/)).toBeVisible();
  });

  it('disables Push and surfaces pushDisabledReason when canPush is false', () => {
    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          pushEnabled: false,
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
          canPull: false,
          fetchDisabledReason: 'Set repository remote URL in Settings → Repositories',
          pushDisabledReason: 'Set repository remote URL in Settings → Repositories',
          pullDisabledReason: 'Set repository remote URL in Settings → Repositories',
        })}
      />,
    );

    const fetchBtn = screen.getByRole('button', { name: 'Fetch' });
    expect(fetchBtn).toBeDisabled();
    expect(fetchBtn).toHaveAttribute(
      'title',
      'Set repository remote URL in Settings → Repositories',
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

  it('confirms before pulling', async () => {
    const onPull = vi.fn();
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(
      <DefaultBranchSyncControls
        repo={repo}
        statusOverride={baseStatus({
          aheadCount: 0,
          behindCount: 1,
          canPush: false,
          canPull: true,
          pullDisabledReason: null,
        })}
        onPull={onPull}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Pull' }));
    expect(confirm).toHaveBeenCalled();
    expect(onPull).toHaveBeenCalled();
    confirm.mockRestore();
  });
});
