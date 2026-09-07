import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Repo } from '../../lib/schemas/repo';
import { RepositoriesPage } from './RepositoriesPage';

const mocks = vi.hoisted(() => ({
  role: 'admin' as 'admin' | 'member',
  repos: [] as Repo[],
  create: vi.fn(),
  update: vi.fn(),
  remove: vi.fn(),
  verify: vi.fn(),
  setToken: vi.fn(),
  clearToken: vi.fn(),
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
  createdAt: '2026-09-04T00:00:00Z',
  updatedAt: '2026-09-04T00:00:00Z',
};

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
  useSetForgeToken: () => mutation(mocks.setToken),
  useClearForgeToken: () => mutation(mocks.clearToken),
  useDefaultBranchSync: () => ({ data: undefined, isLoading: false }),
  useFetchDefaultBranch: () => mutation(vi.fn()),
  usePushDefaultBranch: () => mutation(vi.fn()),
}));

vi.mock('../../components/ToastProvider', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn() }),
}));

describe('RepositoriesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.role = 'admin';
    mocks.repos = [sampleRepo];
    mocks.create.mockResolvedValue(sampleRepo);
    mocks.update.mockResolvedValue(sampleRepo);
    mocks.remove.mockResolvedValue(undefined);
    mocks.verify.mockResolvedValue(sampleRepo);
    mocks.setToken.mockResolvedValue(sampleRepo);
    mocks.clearToken.mockResolvedValue(sampleRepo);
  });

  it('shows a list-only page for admins without a side form or token selector', () => {
    render(<RepositoriesPage />);

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

  it('opens a create drawer from Add repository and closes on Escape and backdrop', () => {
    render(<RepositoriesPage />);

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

    fireEvent.click(screen.getByTestId('repo-drawer-backdrop'));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens an edit drawer with forge token, verify, remove, and sync controls', () => {
    render(<RepositoriesPage />);

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    const dialog = screen.getByRole('dialog', { name: 'Edit repository' });
    expect(dialog).toBeVisible();
    expect(within(dialog).getByDisplayValue('coppice')).toBeVisible();
    expect(within(dialog).getByDisplayValue('/repos/coppice')).toBeVisible();
    expect(within(dialog).getByLabelText('Token')).toBeVisible();
    expect(
      within(dialog).getByRole('button', { name: 'Save token' }),
    ).toBeVisible();
    expect(within(dialog).getByRole('button', { name: 'Verify' })).toBeVisible();
    expect(within(dialog).getByRole('button', { name: 'Remove' })).toBeVisible();
    expect(within(dialog).getByTestId('default-branch-sync')).toBeVisible();
    expect(within(dialog).queryByRole('combobox')).not.toBeInTheDocument();
  });

  it('hides admin controls for non-admins but keeps View code', () => {
    mocks.role = 'member';
    render(<RepositoriesPage />);

    expect(screen.getByText('coppice')).toBeVisible();
    expect(screen.getByRole('button', { name: 'View code' })).toBeVisible();
    expect(
      screen.queryByRole('button', { name: 'Add repository' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Edit' })).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Token')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Verify' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Remove' }),
    ).not.toBeInTheDocument();
  });
});
