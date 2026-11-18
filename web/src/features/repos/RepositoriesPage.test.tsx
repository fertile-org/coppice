import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { Repo } from '../../lib/schemas/repo';
import { RepositoriesPage } from './RepositoriesPage';
import type { DefaultBranchSyncStatus } from './useRepos';

const mocks = vi.hoisted(() => ({
  role: 'admin' as 'admin' | 'member',
  repos: [] as Repo[],
  syncStatus: undefined as DefaultBranchSyncStatus | undefined,
  syncEnabledCalls: [] as Array<{ repoId: string; enabled: boolean }>,
  create: vi.fn(),
  update: vi.fn(),
  remove: vi.fn(),
  verify: vi.fn(),
  setToken: vi.fn(),
  clearToken: vi.fn(),
  fetch: vi.fn(),
  pull: vi.fn(),
  push: vi.fn(),
  sync: vi.fn(),
}));

const sampleRepo: Repo = {
  id: '00000000-0000-4000-8000-000000000001',
  name: 'coppice',
  localPath: '/repos/coppice',
  remoteUrl: 'https://github.com/org/coppice.git',
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

function baseSyncStatus(
  overrides: Partial<DefaultBranchSyncStatus> = {},
): DefaultBranchSyncStatus {
  return {
    defaultBranch: 'main',
    localSha: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    remoteSha: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
    aheadCount: 0,
    behindCount: 2,
    workingTreeClean: true,
    pushEnabled: true,
    forgeTokenConfigured: true,
    canFetch: true,
    canPush: false,
    canPull: true,
    fetchDisabledReason: null,
    pushDisabledReason: 'Local default branch is not ahead of remote',
    pullDisabledReason: null,
    ...overrides,
  };
}

function mutation(mutateAsync: ReturnType<typeof vi.fn>) {
  return { mutateAsync, isPending: false };
}

vi.mock('../auth/useSession', () => ({
  useSession: () => ({
    user: { role: mocks.role },
    loading: false,
  }),
}));

vi.mock('./useRepos', () => ({
  useRepos: () => ({
    data: mocks.repos,
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
  }),
  useCreateRepo: () => mutation(mocks.create),
  useUpdateRepo: () => mutation(mocks.update),
  useDeleteRepo: () => mutation(mocks.remove),
  useVerifyRepo: () => mutation(mocks.verify),
  useInspectRepoPath: () => mutation(vi.fn()),
  useSetForgeToken: () => mutation(mocks.setToken),
  useDefaultBranchSync: (repoId: string, enabled: boolean) => {
    mocks.syncEnabledCalls.push({ repoId, enabled });
    return { data: enabled ? mocks.syncStatus : undefined, isLoading: false };
  },
  useFetchDefaultBranch: () => mutation(mocks.fetch),
  usePushDefaultBranch: () => mutation(mocks.push),
  usePullDefaultBranch: () => mutation(mocks.pull),
  useSyncDefaultBranch: () => mutation(mocks.sync),
}));

vi.mock('./useWorktrees', () => ({
  useWorktrees: () => ({
    data: [],
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
  }),
  useRemoveWorktrees: () => mutation(vi.fn()),
}));

vi.mock('../../components/ToastProvider', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn() }),
}));

function renderPage() {
  return render(
    <MemoryRouter>
      <ConfirmProvider>
        <RepositoriesPage />
      </ConfirmProvider>
    </MemoryRouter>,
  );
}

describe('RepositoriesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.role = 'admin';
    mocks.repos = [sampleRepo];
    mocks.syncStatus = baseSyncStatus();
    mocks.syncEnabledCalls = [];
    mocks.create.mockResolvedValue(sampleRepo);
    mocks.update.mockResolvedValue(sampleRepo);
    mocks.remove.mockResolvedValue(undefined);
    mocks.verify.mockResolvedValue(sampleRepo);
    mocks.setToken.mockResolvedValue(sampleRepo);
    mocks.clearToken.mockResolvedValue(sampleRepo);
    mocks.fetch.mockResolvedValue(baseSyncStatus({ canPull: true }));
    mocks.pull.mockResolvedValue({
      defaultBranch: 'main',
      message: 'Pulled main',
      status: baseSyncStatus({ aheadCount: 0, behindCount: 0, canPull: false }),
    });
    mocks.push.mockResolvedValue(undefined);
    mocks.sync.mockResolvedValue({
      action: 'pulled',
      defaultBranch: 'main',
      message: 'Default branch pulled',
      status: baseSyncStatus({ aheadCount: 0, behindCount: 0, canPull: false }),
    });
  });

  it('shows a list-only page for admins without a side form or token selector', () => {
    renderPage();

    expect(screen.getByRole('heading', { name: 'Repositories' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Add repository' })).toBeVisible();
    expect(screen.getByText('coppice')).toBeVisible();
    expect(screen.getByRole('button', { name: 'View code' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Edit' })).toBeVisible();

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Local path')).not.toBeInTheDocument();
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Verify' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Remove' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId('default-branch-sync')).not.toBeInTheDocument();
  });

  it('opens a create drawer from Add repository and closes on Escape and backdrop', async () => {
    renderPage();

    fireEvent.click(screen.getByRole('button', { name: 'Add repository' }));

    const dialog = screen.getByRole('dialog', { name: 'Add repository' });
    expect(dialog).toBeVisible();
    expect(within(dialog).getByLabelText('Name')).toBeVisible();
    expect(within(dialog).getByLabelText('Local path')).toBeVisible();
    expect(
      within(dialog).getByRole('button', { name: 'Add repository' }),
    ).toBeVisible();

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Add repository' }));
    expect(screen.getByRole('dialog', { name: 'Add repository' })).toBeVisible();

    const backdrop = screen.getByTestId('repo-drawer-backdrop');
    await waitFor(() => {
      fireEvent.pointerDown(backdrop);
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
  });

  it('opens an edit drawer with verify, remove, and sync controls (no forge token)', () => {
    renderPage();

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    const dialog = screen.getByRole('dialog', { name: 'Edit repository' });
    expect(dialog).toBeVisible();
    expect(within(dialog).getByDisplayValue('coppice')).toBeVisible();
    expect(within(dialog).getByDisplayValue('/repos/coppice')).toBeVisible();
    expect(within(dialog).queryByLabelText('Token')).not.toBeInTheDocument();
    expect(
      within(dialog).queryByRole('button', { name: 'Save token' }),
    ).not.toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: 'Verify' })).toBeVisible();
    expect(within(dialog).getByRole('button', { name: 'Remove' })).toBeVisible();
    expect(within(dialog).getByTestId('default-branch-sync')).toBeVisible();
    expect(within(dialog).queryByRole('combobox')).not.toBeInTheDocument();
  });

  it('asks for a forge token again only after import marks it', () => {
    mocks.repos = [{ ...sampleRepo, forgeTokenNeedsReentry: true }];
    renderPage();

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    const dialog = screen.getByRole('dialog', { name: 'Edit repository' });
    const field = within(dialog).getByLabelText('Token');
    expect(field).toHaveAttribute(
      'placeholder',
      "Paste this token again. Backups don't include saved tokens.",
    );
    expect(
      within(dialog).getByRole('button', { name: 'Save token' }),
    ).toBeVisible();
  });

  it('hides admin controls for non-admins but keeps View code', () => {
    mocks.role = 'member';
    renderPage();

    expect(screen.getByText('coppice')).toBeVisible();
    expect(screen.getByRole('button', { name: 'View code' })).toBeVisible();
    expect(
      screen.queryByRole('button', { name: 'Add repository' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Edit' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Sync' })).not.toBeInTheDocument();
    expect(screen.queryByTestId('repo-git-status')).not.toBeInTheDocument();
    expect(screen.getByText('—')).toBeVisible();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Token')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Verify' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Remove' }),
    ).not.toBeInTheDocument();
    expect(mocks.syncEnabledCalls.every((c) => !c.enabled)).toBe(true);
  });

  it('shows GIT status and Sync for ready admin rows without Push', () => {
    renderPage();

    expect(screen.getByRole('columnheader', { name: 'Git' })).toBeVisible();
    expect(screen.getByTestId('repo-git-status')).toHaveTextContent(
      'ahead 0, behind 2',
    );
    const sync = screen.getByRole('button', { name: 'Sync' });
    expect(sync).toBeEnabled();
    expect(
      screen.queryByRole('button', { name: 'Push to remote' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Fetch' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Pull' })).not.toBeInTheDocument();
  });

  it('disables Sync when the working tree is dirty', () => {
    mocks.syncStatus = baseSyncStatus({
      workingTreeClean: false,
      canFetch: true,
      canPull: false,
    });
    renderPage();

    expect(screen.getByTestId('repo-git-status')).toHaveTextContent(
      'working tree dirty',
    );
    expect(screen.getByRole('button', { name: 'Sync' })).toBeDisabled();
  });

  it('Sync calls the sync endpoint', async () => {
    renderPage();

    fireEvent.click(screen.getByRole('button', { name: 'Sync' }));

    await waitFor(() => expect(mocks.sync).toHaveBeenCalled());
    expect(mocks.fetch).not.toHaveBeenCalled();
    expect(mocks.pull).not.toHaveBeenCalled();
  });

  it('keeps Fetch / Pull / Push in the edit drawer', () => {
    renderPage();

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    const dialog = screen.getByRole('dialog', { name: 'Edit repository' });
    const sync = within(dialog).getByTestId('default-branch-sync');
    expect(within(sync).getByRole('button', { name: 'Fetch' })).toBeVisible();
    expect(within(sync).getByRole('button', { name: 'Pull' })).toBeVisible();
    expect(
      within(sync).getByRole('button', { name: 'Push to remote' }),
    ).toBeVisible();
  });

  it('canceling remove repository leaves it in place', async () => {
    renderPage();

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    fireEvent.click(within(screen.getByRole('dialog', { name: 'Edit repository' })).getByRole('button', { name: 'Remove' }));
    const dialog = await screen.findByRole('dialog', {
      name: 'Remove repository "coppice"?',
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));

    expect(mocks.remove).not.toHaveBeenCalled();
  });

  it('confirming remove repository deletes it', async () => {
    renderPage();

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    fireEvent.click(within(screen.getByRole('dialog', { name: 'Edit repository' })).getByRole('button', { name: 'Remove' }));
    const dialog = await screen.findByRole('dialog', {
      name: 'Remove repository "coppice"?',
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));

    await waitFor(() => expect(mocks.remove).toHaveBeenCalledWith(sampleRepo.id));
  });
});
