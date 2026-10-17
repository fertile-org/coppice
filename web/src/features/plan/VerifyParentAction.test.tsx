import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PLAN_COPY } from './copy';
import { VerifyParentAction } from './VerifyParentAction';

const mocks = vi.hoisted(() => ({ apiFetch: vi.fn() }));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [
      { id: 'pm', name: 'PM', enabled: true },
      { id: 'tl', name: 'Tech Lead', enabled: true },
      { id: 'off', name: 'Off', enabled: false },
    ],
  }),
}));

function renderAction() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <VerifyParentAction ticketId="parent" boardId="b1" />
    </QueryClientProvider>,
  );
}

describe('VerifyParentAction', () => {
  beforeEach(() => {
    mocks.apiFetch.mockReset();
    mocks.apiFetch.mockResolvedValue({ ok: true, status: 200, json: async () => ({}) });
  });

  it('verifies with the PM by default', async () => {
    renderAction();
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.verifyParent }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/parent/verify-parent',
        expect.objectContaining({ body: '{}' }),
      ),
    );
  });

  it('lets the human pick another enabled agent', async () => {
    renderAction();
    const select = screen.getByLabelText(PLAN_COPY.verifyAgent);
    expect(screen.queryByRole('option', { name: 'Off' })).not.toBeInTheDocument();
    fireEvent.change(select, { target: { value: 'tl' } });
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.verifyParent }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/parent/verify-parent',
        expect.objectContaining({ body: JSON.stringify({ agentId: 'tl' }) }),
      ),
    );
  });
});
