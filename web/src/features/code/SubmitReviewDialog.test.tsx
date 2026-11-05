import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { ApiError } from '../../lib/api';
import { selectComboboxOption } from '../../test/combobox';
import { SubmitReviewDialog } from './SubmitReviewDialog';

const {
  TICKET_ID,
  BOARD_ID,
  ADA_ID,
  BEN_ID,
  PM_ID,
  REPO_ID,
  agents,
  boards,
  ticketState,
  mutateAsync,
} = vi.hoisted(() => {
  const TICKET_ID = '00000000-0000-4000-8000-000000000001';
  const BOARD_ID = '00000000-0000-4000-8000-000000000002';
  const ADA_ID = '00000000-0000-4000-8000-000000000003';
  const BEN_ID = '00000000-0000-4000-8000-000000000004';
  const PM_ID = '00000000-0000-4000-8000-000000000005';
  const REPO_ID = '00000000-0000-4000-8000-000000000006';
  return {
    TICKET_ID,
    BOARD_ID,
    ADA_ID,
    BEN_ID,
    PM_ID,
    REPO_ID,
    agents: [
      { id: ADA_ID, name: 'Ada', role: 'Backend Engineer', enabled: true },
      { id: BEN_ID, name: 'Ben', role: 'Frontend Engineer', enabled: true },
      { id: PM_ID, name: 'Pat', role: 'PM', enabled: true },
    ],
    boards: [] as { id: string; name: string }[],
    ticketState: {
      assigneeAgentId: ADA_ID as string | undefined,
    },
    mutateAsync: vi.fn(),
  };
});

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({ data: agents }),
}));

vi.mock('../boards/useBoards', () => ({
  useBoards: () => ({ data: boards }),
}));

vi.mock('../tickets/useTicket', () => ({
  useTicket: () => ({
    data: {
      id: TICKET_ID,
      boardId: BOARD_ID,
      assigneeAgentId: ticketState.assigneeAgentId,
    },
  }),
}));

vi.mock('./useCodeReview', () => ({
  useSubmitCodeReview: () => ({ mutateAsync, isPending: false }),
}));

function renderDialog() {
  render(
    <MemoryRouter>
      <ToastProvider>
        <SubmitReviewDialog
          open
          onClose={() => {}}
          repoId={REPO_ID}
          repoName="coppice"
          worktreePath="/tmp/worktree"
          baseBranch="main"
          headBranch="agent/ticket"
          headSha="abc123"
          ticketId={TICKET_ID}
          inlineComments={[]}
          onSubmitted={() => {}}
        />
      </ToastProvider>
    </MemoryRouter>,
  );
}

function fillSummary() {
  fireEvent.change(screen.getByLabelText('Summary'), {
    target: { value: 'Needs another pass.' },
  });
}

describe('SubmitReviewDialog', () => {
  beforeEach(() => {
    ticketState.assigneeAgentId = ADA_ID;
    mutateAsync.mockReset();
    mutateAsync.mockResolvedValue({
      ticketId: TICKET_ID,
      commentId: '00000000-0000-4000-8000-000000000099',
      ticketCreated: false,
    });
  });

  it('shows an engineer picker for move and reassign, and sends reassignAgentId', async () => {
    renderDialog();
    fillSummary();

    selectComboboxOption(
      screen.getByRole('combobox', { name: 'Workflow action' }),
      'Move to In Progress',
    );
    const engineer = screen.getByRole('combobox', { name: 'Engineer' });
    expect(engineer).toHaveTextContent('Ada · Backend Engineer');

    fireEvent.click(screen.getByRole('button', { name: 'Submit review' }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        workflowAction: 'move_to_in_progress',
        reassignAgentId: ADA_ID,
      }),
    );
  });

  it('sends the chosen engineer when reassigning', async () => {
    renderDialog();
    fillSummary();

    selectComboboxOption(
      screen.getByRole('combobox', { name: 'Workflow action' }),
      'Reassign engineer',
    );
    selectComboboxOption(
      screen.getByRole('combobox', { name: 'Engineer' }),
      'Ben · Frontend Engineer',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Submit review' }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        workflowAction: 'reassign_engineer',
        reassignAgentId: BEN_ID,
      }),
    );
  });

  it('prefills the first engineer when the assignee is not an engineer', () => {
    ticketState.assigneeAgentId = PM_ID;
    renderDialog();

    selectComboboxOption(
      screen.getByRole('combobox', { name: 'Workflow action' }),
      'Move to In Progress',
    );
    expect(screen.getByRole('combobox', { name: 'Engineer' })).toHaveTextContent(
      'Ada · Backend Engineer',
    );
  });

  it('shows the server error when submit fails', async () => {
    mutateAsync.mockRejectedValue(
      new ApiError(
        400,
        JSON.stringify({
          error:
            'Approve the plan, or turn on Skip planning, before moving this ticket to In Progress.',
        }),
      ),
    );
    renderDialog();
    fillSummary();
    selectComboboxOption(
      screen.getByRole('combobox', { name: 'Workflow action' }),
      'Move to In Progress',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Submit review' }));

    expect(
      await screen.findByRole('alert'),
    ).toHaveTextContent(
      'Approve the plan, or turn on Skip planning, before moving this ticket to In Progress.',
    );
  });
});
