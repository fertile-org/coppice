import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PLAN_COPY } from './copy';
import { PlanReviewActions } from './PlanReviewActions';

const mocks = vi.hoisted(() => ({ apiFetch: vi.fn() }));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../keyboard/useKeyboard', () => ({
  useRegisterShortcut: vi.fn(),
}));

function wrap(node: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
}

const questions = {
  questions: [
    { question: 'Which database?', choices: ['Postgres', 'SQLite'] },
    { question: 'Keep the old endpoint?' },
  ],
  answers: [],
  waiting: true,
};

describe('PlanReviewActions', () => {
  beforeEach(() => {
    mocks.apiFetch.mockReset();
    mocks.apiFetch.mockResolvedValue({ ok: true, status: 200, json: async () => ({}) });
  });

  it('asks the questions instead of offering Approve plan', () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" hasPlan={false} planQuestions={questions} />);
    expect(screen.getByText('Which database?')).toBeVisible();
    expect(screen.getByText('Keep the old endpoint?')).toBeVisible();
    expect(screen.queryByRole('button', { name: PLAN_COPY.approvePlan })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: PLAN_COPY.askForChanges })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send answers' })).toBeDisabled();
  });

  it('disables Approve while a plan run is active', () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" planRunActive />);
    expect(screen.getByRole('button', { name: PLAN_COPY.approvePlan })).toBeDisabled();
  });

  it('warns beside Approve when the plan missed the chosen design, without blocking', () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" designOutOfDate />);
    expect(screen.getByText("The plan wasn't updated to this option.")).toBeVisible();
    expect(screen.getByRole('button', { name: PLAN_COPY.approvePlan })).toBeEnabled();
  });

  it('has no design warning when the plan is up to date', () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" />);
    expect(screen.queryByText("The plan wasn't updated to this option.")).toBeNull();
  });

  it('sends the answers once at least one is given', async () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" hasPlan={false} planQuestions={questions} />);
    fireEvent.click(screen.getByRole('button', { name: 'Postgres' }));
    const send = screen.getByRole('button', { name: 'Send answers' });
    expect(send).toBeEnabled();
    fireEvent.click(send);
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/t1/plan-answers',
        expect.objectContaining({ body: JSON.stringify({ answers: ['Postgres', ''] }) }),
      ),
    );
  });

  it('shows nothing while the plan is still being written', () => {
    const { container } = wrap(
      <PlanReviewActions
        ticketId="t1"
        boardId="b1"
        hasPlan={false}
        planQuestions={{ ...questions, waiting: false }}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('offers Approve plan and Ask for changes once a plan exists', () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" hasPlan />);
    expect(screen.getByRole('button', { name: PLAN_COPY.approvePlan })).toBeVisible();
    expect(screen.getByRole('button', { name: PLAN_COPY.askForChanges })).toBeVisible();
  });
  it('shows the plan right above Approve plan and Ask for changes', () => {
    wrap(
      <PlanReviewActions
        ticketId="t1"
        boardId="b1"
        plan={'## Plan\n\n### Approach\nMove the settings into a drawer.'}
      />,
    );
    const plan = screen.getByTestId('pending-plan');
    expect(plan).toHaveTextContent('Move the settings into a drawer.');
    const approve = screen.getByRole('button', { name: PLAN_COPY.approvePlan });
    expect(plan.compareDocumentPosition(approve) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText(PLAN_COPY.approveAddsToDescription)).toBeInTheDocument();
  });

  it('shows the revision and a collapsed list of requested changes', () => {
    wrap(
      <PlanReviewActions
        ticketId="t1"
        boardId="b1"
        revision={2}
        changeRequests={['Use Postgres.', 'Split the API change.']}
        plan={'## Plan\n\n### Approach\nPostgres.'}
      />,
    );
    expect(screen.getByText('Revision 2')).toBeVisible();
    const requested = screen.getByText(PLAN_COPY.requestedChanges);
    expect(requested.closest('details')).not.toHaveAttribute('open');
    expect(screen.getByText('Use Postgres.')).toBeInTheDocument();
    expect(screen.getByText('Split the API change.')).toBeInTheDocument();
  });
});
