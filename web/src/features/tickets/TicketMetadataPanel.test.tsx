import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { ToastProvider } from '../../components/ToastProvider';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { Ticket } from '../board/useTickets';
import { PLAN_COPY } from '../plan/copy';
import { TicketMetadataPanel } from './TicketMetadataPanel';

const approveSplits = vi.fn();
const assignAgent = vi.fn();

vi.mock('./TicketGitActions', () => ({
  TicketGitActions: () => null,
}));

vi.mock('../keyboard/useKeyboard', () => ({
  useRegisterShortcut: () => {},
}));

vi.mock('../repos/useRepos', () => ({
  useRepos: () => ({ data: [] }),
}));

vi.mock('./useAgentRuns', () => ({
  useAgentRuns: () => ({ data: [] }),
}));

vi.mock('./useTicket', () => ({
  useAgents: () => ({ data: [] }),
  useApproveSplits: () => ({ mutateAsync: approveSplits, isPending: false }),
  useAssignAgent: () => ({ mutateAsync: assignAgent, isPending: false }),
  useDismissSplits: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useTicketGitInfo: () => ({ data: undefined }),
  useUpdateTicket: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useUpdateTicketStatus: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

function makeTicket(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: '00000000-0000-0000-0000-000000000001',
    boardId: '00000000-0000-0000-0000-000000000002',
    title: 'Parent ticket',
    description: '',
    status: 'in_progress',
    createdBy: 'user',
    createdAt: '2026-10-22T00:00:00.000Z',
    updatedAt: '2026-10-22T00:00:00.000Z',
    lastActivityAt: '2026-10-22T00:00:00.000Z',
    pendingSplitRecommendation: {
      recommendedByAgentId: '00000000-0000-0000-0000-0000000000aa',
      recommendedAt: '2026-10-22T00:00:00.000Z',
      splits: [
        { title: 'Child one', description: 'First' },
        { title: 'Child two', description: 'Second' },
      ],
      fromPlan: false,
    },
    ...overrides,
  };
}

function renderPanel(ticket: Ticket = makeTicket()) {
  return render(
    <MemoryRouter>
      <ToastProvider>
        <ConfirmProvider>
          <TicketMetadataPanel ticket={ticket} />
        </ConfirmProvider>
      </ToastProvider>
    </MemoryRouter>,
  );
}

const PM_ID = '00000000-0000-0000-0000-000000000010';

describe('TicketMetadataPanel suggested PM', () => {
  beforeEach(() => {
    assignAgent.mockReset();
  });

  it('assigns the suggested PM without Save', async () => {
    assignAgent.mockResolvedValue(
      makeTicket({
        status: 'backlog',
        assigneeAgentId: PM_ID,
        suggestedAssignee: null,
        pendingSplitRecommendation: null,
      }),
    );
    renderPanel(
      makeTicket({
        status: 'backlog',
        pendingSplitRecommendation: null,
        suggestedAssignee: { id: PM_ID, name: 'PM' },
      }),
    );

    expect(screen.getByText(PLAN_COPY.suggestedPm)).toBeVisible();
    expect(screen.getByText(PLAN_COPY.suggestedPmRole)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.assignPm }));

    await waitFor(() => expect(assignAgent).toHaveBeenCalledWith(PM_ID));
    expect(screen.queryByRole('button', { name: PLAN_COPY.assignPm })).toBeNull();
    expect(await screen.findByText(PLAN_COPY.assignedToPm)).toBeVisible();
  });
});

describe('TicketMetadataPanel split confirm', () => {
  beforeEach(() => {
    approveSplits.mockReset();
    approveSplits.mockResolvedValue(undefined);
  });

  it('cancels split approval without creating children', async () => {
    renderPanel();

    fireEvent.click(screen.getByRole('button', { name: 'Approve splits' }));
    const dialog = await screen.findByRole('dialog', {
      name: 'Create 2 child tickets from this split recommendation? The parent ticket will remain unchanged.',
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));

    expect(approveSplits).not.toHaveBeenCalled();
  });

  it('creates child tickets after confirm', async () => {
    renderPanel();

    fireEvent.click(screen.getByRole('button', { name: 'Approve splits' }));
    const dialog = await screen.findByRole('dialog', {
      name: 'Create 2 child tickets from this split recommendation? The parent ticket will remain unchanged.',
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));

    await waitFor(() => expect(approveSplits).toHaveBeenCalled());
  });
});
