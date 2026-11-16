import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { WorktreeAdminEntry } from '../../lib/schemas/worktree';
import { WorktreesTab } from './WorktreesTab';

const mocks = vi.hoisted(() => ({
  rows: [] as WorktreeAdminEntry[],
  remove: vi.fn(),
}));

function sample(overrides: Partial<WorktreeAdminEntry> = {}): WorktreeAdminEntry {
  return {
    folderName: 'TICKET-aaaaaaaa-coppice',
    path: '/tmp/TICKET-aaaaaaaa-coppice',
    repoId: '00000000-0000-4000-8000-000000000001',
    repoName: 'coppice',
    ticketId: 'aaaaaaaa-0000-4000-8000-000000000001',
    ticketTitle: 'Done ticket',
    ticketStatus: 'done',
    archived: false,
    merged: false,
    hasUncommittedChanges: false,
    hasActiveRun: false,
    lastUsedAt: '2026-01-01T00:00:00Z',
    sizeBytes: 5_000_000,
    finished: true,
    ...overrides,
  };
}

vi.mock('./useWorktrees', () => ({
  useWorktrees: () => ({
    data: mocks.rows,
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
  }),
  useRemoveWorktrees: () => ({
    mutateAsync: mocks.remove,
    isPending: false,
  }),
}));

vi.mock('../../components/ToastProvider', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn() }),
}));

function renderTab() {
  return render(
    <ConfirmProvider>
      <WorktreesTab />
    </ConfirmProvider>,
  );
}

describe('WorktreesTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.rows = [
      sample({
        folderName: 'TICKET-bbbbbbbb-coppice',
        ticketTitle: 'Big finished',
        sizeBytes: 10_000_000,
        finished: true,
      }),
      sample({
        folderName: 'TICKET-cccccccc-coppice',
        ticketTitle: 'Measuring…',
        sizeBytes: null,
        finished: true,
      }),
      sample({
        folderName: 'TICKET-dddddddd-coppice',
        ticketTitle: 'Running',
        ticketStatus: 'in_progress',
        hasActiveRun: true,
        finished: false,
        sizeBytes: 2_000_000,
      }),
      sample({
        folderName: 'TICKET-eeeeeeee-coppice',
        ticketTitle: 'Dirty done',
        hasUncommittedChanges: true,
        finished: false,
        sizeBytes: 3_000_000,
      }),
    ];
    mocks.remove.mockResolvedValue([
      { folderName: 'TICKET-bbbbbbbb-coppice', status: 'removed' },
    ]);
  });

  it('sorts known sizes first and shows measuring rows last', () => {
    renderTab();
    const titles = screen.getAllByRole('row').slice(1).map((row) => row.textContent ?? '');
    expect(titles[0]).toContain('Big finished');
    expect(titles[titles.length - 1]).toContain('Measuring…');
    expect(screen.getByText('…')).toBeVisible();
  });

  it('Select finished skips active and dirty rows', () => {
    renderTab();
    fireEvent.click(screen.getByRole('button', { name: 'Select finished' }));
    expect(screen.getByLabelText('Select TICKET-bbbbbbbb-coppice')).toBeChecked();
    expect(screen.getByLabelText('Select TICKET-cccccccc-coppice')).toBeChecked();
    expect(screen.getByLabelText('Select TICKET-dddddddd-coppice')).not.toBeChecked();
    expect(screen.getByLabelText('Select TICKET-eeeeeeee-coppice')).not.toBeChecked();
    expect(screen.getByLabelText('Select TICKET-dddddddd-coppice')).toBeDisabled();
  });

  it('confirm dialog warns about uncommitted changes', async () => {
    renderTab();
    fireEvent.click(screen.getByLabelText('Select TICKET-eeeeeeee-coppice'));
    fireEvent.click(screen.getByRole('button', { name: 'Remove selected' }));
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent(/uncommitted changes/i);
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(mocks.remove).not.toHaveBeenCalled();
  });

  it('shows per-item failures after a bulk remove', async () => {
    mocks.remove.mockResolvedValue([
      { folderName: 'TICKET-bbbbbbbb-coppice', status: 'removed' },
      {
        folderName: 'TICKET-cccccccc-coppice',
        status: 'active_run',
        message: 'an agent is working in this worktree',
      },
    ]);
    renderTab();
    fireEvent.click(screen.getByRole('button', { name: 'Select finished' }));
    fireEvent.click(screen.getByRole('button', { name: 'Remove selected' }));
    const dialog = await screen.findByRole('dialog');
    fireEvent.click(within(dialog).getByRole('button', { name: /Remove worktrees/i }));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/TICKET-cccccccc-coppice/);
      expect(screen.getByRole('alert')).toHaveTextContent(/agent is working/i);
    });
  });
});
