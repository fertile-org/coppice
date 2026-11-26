import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import { useKeyboardEnvironment } from '../keyboard/useKeyboard';
import { makeDesignSet } from '../designs/designFixtures';
import { PLAN_COPY } from './copy';
import { PlanReviewActions } from './PlanReviewActions';

const mocks = vi.hoisted(() => ({ apiFetch: vi.fn() }));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

function OpenDrawer() {
  useKeyboardEnvironment({ drawerOpen: true, modalOpen: false });
  return null;
}

function wrap(node: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
}

function renderWithKeys(node: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <MemoryRouter initialEntries={['/boards/board-1']}>
      <KeyboardProvider>
        <QueryClientProvider client={client}>
          <OpenDrawer />
          {node}
        </QueryClientProvider>
      </KeyboardProvider>
    </MemoryRouter>,
  );
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
    mocks.apiFetch.mockImplementation((path: string) => {
      if (String(path).endsWith('/designs')) {
        return Promise.resolve({ ok: true, status: 200, json: async () => [] });
      }
      return Promise.resolve({ ok: true, status: 200, json: async () => ({}) });
    });
  });

  it('disables Approve while a plan run is active', () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" planRunActive />);
    expect(screen.getByRole('button', { name: PLAN_COPY.approvePlan })).toBeDisabled();
  });

  it('accepts every suggested answer in one click', async () => {
    wrap(
      <PlanReviewActions
        ticketId="t1"
        boardId="b1"
        hasPlan={false}
        planQuestions={{
          questions: [
            { question: 'Which database?', choices: ['Postgres', 'SQLite'], recommended: 'Postgres' },
            { question: 'Keep the old endpoint?', recommended: 'No' },
          ],
          answers: [],
          waiting: true,
          round: 2,
        }}
      />,
    );
    expect(screen.getByRole('heading', { name: PLAN_COPY.moreQuestions })).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.acceptAllRecommended }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/t1/plan-answers',
        expect.objectContaining({
          body: JSON.stringify({ answers: ['Postgres', 'No'] }),
        }),
      ),
    );
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

  it('pre-ticks Skip human review from the planner suggestion and approves with the box value', async () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" suggestSkipHumanReview />);
    const box = screen.getByRole('checkbox', { name: new RegExp(PLAN_COPY.skipHumanReview) });
    expect(box).toBeChecked();
    expect(screen.getByText(PLAN_COPY.suggestedByPlanner)).toBeVisible();
    fireEvent.click(box);
    expect(box).not.toBeChecked();
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.approvePlan }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/t1/approve-plan',
        expect.objectContaining({ body: JSON.stringify({ skipHumanReview: false }) }),
      ),
    );
  });

  it('locks Approve while a note is unsent, and turns it back on when the box is cleared', () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" />);
    const box = screen.getByRole('textbox', { name: PLAN_COPY.askPlaceholder });
    const approve = screen.getByRole('button', { name: PLAN_COPY.approvePlan });
    const ask = screen.getByRole('button', { name: PLAN_COPY.askForChanges });

    fireEvent.change(box, { target: { value: 'please split this' } });
    expect(approve).toBeDisabled();
    expect(ask).toBeEnabled();
    expect(screen.getByText(PLAN_COPY.approveNeedsEmptyNote)).toBeVisible();

    fireEvent.change(box, { target: { value: '   ' } });
    expect(approve).toBeEnabled();
    expect(screen.queryByText(PLAN_COPY.approveNeedsEmptyNote)).toBeNull();
  });

  it('does not approve with r p while the box has text, and approves once it is empty', async () => {
    renderWithKeys(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" />);
    const box = screen.getByRole('textbox', { name: PLAN_COPY.askPlaceholder });

    const ask = screen.getByRole('button', { name: PLAN_COPY.askForChanges });
    fireEvent.change(box, { target: { value: 'please split this' } });
    ask.focus();
    fireEvent.keyDown(ask, { key: 'r' });
    fireEvent.keyDown(ask, { key: 'p' });
    expect(mocks.apiFetch.mock.calls.some(([path]) => String(path).includes('approve-plan'))).toBe(
      false,
    );

    fireEvent.change(box, { target: { value: '' } });
    fireEvent.keyDown(document, { key: 'r' });
    fireEvent.keyDown(document, { key: 'p' });
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/t1/approve-plan',
        expect.objectContaining({ method: 'POST' }),
      ),
    );
  });

  it('sends the note from Ask for changes and still approves from an empty box', async () => {
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" />);
    const box = screen.getByRole('textbox', { name: PLAN_COPY.askPlaceholder });
    fireEvent.change(box, { target: { value: 'Split the API change.' } });
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.askForChanges }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/t1/plan-changes',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ comment: 'Split the API change.' }),
        }),
      ),
    );

    mocks.apiFetch.mockClear();
    fireEvent.change(box, { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.approvePlan }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/t1/approve-plan',
        expect.objectContaining({ method: 'POST' }),
      ),
    );
  });

  it('labels Approve with recommended defaults and sends acceptRecommended', async () => {
    const set = makeDesignSet({ chosen: null });
    mocks.apiFetch.mockImplementation((path: string) => {
      if (String(path).endsWith('/designs')) {
        return Promise.resolve({ ok: true, status: 200, json: async () => [set] });
      }
      return Promise.resolve({ ok: true, status: 200, json: async () => ({}) });
    });
    wrap(<PlanReviewActions ticketId="t1" boardId="b1" plan="## Plan" />);
    const approve = await screen.findByRole('button', {
      name: PLAN_COPY.approveWithRecommended(1),
    });
    fireEvent.click(approve);
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/t1/approve-plan',
        expect.objectContaining({
          body: JSON.stringify({ skipHumanReview: false, acceptRecommended: true }),
        }),
      ),
    );
  });
});
