import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { ReactElement } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
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
  mergeAfterAccept: false,
  pushAfterMerge: false,
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

function renderControls(ui: ReactElement) {
  return render(<ConfirmProvider>{ui}</ConfirmProvider>);
}

describe('DefaultBranchSyncControls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('enables Fetch and Push when status allows both', () => {
    renderControls(
      <DefaultBranchSyncControls repo={repo} statusOverride={baseStatus()} />,
    );

    expect(screen.getByRole('button', { name: 'Fetch' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Push to remote' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Pull' })).toBeDisabled();
    expect(screen.getByText(/ahead 1, behind 0/)).toBeVisible();
  });

  it('enables Pull from server canPull and disables Push when behind', () => {
    renderControls(
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
    renderControls(
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

  it('dedupes repeated disabled reasons in the summary', () => {
    renderControls(
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
    renderControls(
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

  it('confirms before push and pull', async () => {
    for (const [button, status, handler] of [
      ['Push to remote', { aheadCount: 1, behindCount: 0 }, 'onPush'],
      [
        'Pull',
        {
          aheadCount: 0,
          behindCount: 1,
          canPush: false,
          canPull: true,
          pullDisabledReason: null,
        },
        'onPull',
      ],
    ] as const) {
      const onPush = vi.fn();
      const onPull = vi.fn();
      const view = renderControls(
        <DefaultBranchSyncControls
          repo={repo}
          statusOverride={baseStatus(status)}
          onPush={onPush}
          onPull={onPull}
        />,
      );
      fireEvent.click(screen.getByRole('button', { name: button }));
      fireEvent.click(await within(screen.getByRole('dialog')).findByRole('button', { name: 'Confirm' }));
      await waitFor(() =>
        handler === 'onPush' ? expect(onPush).toHaveBeenCalled() : expect(onPull).toHaveBeenCalled(),
      );
      view.unmount();
    }
  });
});
