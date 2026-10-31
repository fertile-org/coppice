import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../lib/api';
import type { DesignSet } from '../../lib/schemas/design';
import { DesignDialog } from './DesignDialog';
import { makeDesignSet } from './designFixtures';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

function open(design: DesignSet) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <DesignDialog design={design} scope="t1" onClose={() => {}} />
    </QueryClientProvider>,
  );
  return within(screen.getByRole('dialog'));
}

function viewOption(dialog: ReturnType<typeof open>, title: string) {
  fireEvent.click(dialog.getByRole('tab', { name: new RegExp(title) }));
}

describe('DesignDialog footer', () => {
  beforeEach(() => {
    apiFetch.mockReset();
    apiFetch.mockImplementation((path: string) =>
      String(path).endsWith('/choose')
        ? Promise.resolve(new Response(null, { status: 204 }))
        : Promise.resolve(new Response('missing', { status: 404 })),
    );
  });

  it('selected option, plan up to date: "Work follows this option." and no button', () => {
    const dialog = open(makeDesignSet());
    expect(dialog.getByText('Work follows this option.')).toBeInTheDocument();
    expect(dialog.queryByRole('button', { name: /^Choose|Try again/ })).toBeNull();
  });

  it('out of date: "The plan wasn\'t updated to this option." with Try again', async () => {
    const dialog = open(
      makeDesignSet({ choice: { mode: 'update_plan', planOutOfDate: true } }),
    );
    expect(dialog.getByText("The plan wasn't updated to this option.")).toBeInTheDocument();
    fireEvent.click(dialog.getByRole('button', { name: 'Try again' }));
    await waitFor(() =>
      expect(apiFetch).toHaveBeenCalledWith(
        '/api/designs/11111111-1111-4111-8111-111111111111/choose',
        expect.objectContaining({ method: 'POST', body: JSON.stringify({ optionId: 'a' }) }),
      ),
    );
  });

  it('update_plan on another option: "Choose B and update plan"', async () => {
    const dialog = open(makeDesignSet());
    viewOption(dialog, 'Full-width');
    fireEvent.click(dialog.getByRole('button', { name: 'Choose B and update plan' }));
    await waitFor(() =>
      expect(apiFetch).toHaveBeenCalledWith(
        expect.stringMatching(/\/choose$/),
        expect.objectContaining({ body: JSON.stringify({ optionId: 'b' }) }),
      ),
    );
  });

  it('plan_running: "The plan is being updated." with no button', () => {
    const dialog = open(
      makeDesignSet({ choice: { mode: 'plan_running', planOutOfDate: false } }),
    );
    viewOption(dialog, 'Full-width');
    expect(dialog.getByText('The plan is being updated.')).toBeInTheDocument();
    expect(dialog.queryByRole('button', { name: /^Choose|Try again/ })).toBeNull();
  });

  it('plan_final: approved-plan text and no button', () => {
    const dialog = open(makeDesignSet({ choice: { mode: 'plan_final', planOutOfDate: false } }));
    viewOption(dialog, 'Full-width');
    expect(
      dialog.getByText(
        'The plan is approved, so this design is final. To change it, add a comment on the ticket.',
      ),
    ).toBeInTheDocument();
    expect(dialog.queryByRole('button', { name: /^Choose|Try again/ })).toBeNull();
  });

  it('save_only: "Choose B", also with nothing selected', () => {
    const dialog = open(
      makeDesignSet({ selected: null, choice: { mode: 'save_only', planOutOfDate: false } }),
    );
    expect(dialog.getByRole('button', { name: 'Choose A' })).toBeInTheDocument();
    viewOption(dialog, 'Full-width');
    expect(dialog.getByRole('button', { name: 'Choose B' })).toBeInTheDocument();
  });

  it('save_only with a selection: the selected option has no action', () => {
    const dialog = open(makeDesignSet({ choice: { mode: 'save_only', planOutOfDate: false } }));
    expect(dialog.getByText('Work follows this option.')).toBeInTheDocument();
    viewOption(dialog, 'Footer');
    expect(dialog.getByRole('button', { name: 'Choose C' })).toBeInTheDocument();
  });

  it('read_only: no action', () => {
    const dialog = open(makeDesignSet({ choice: { mode: 'read_only', planOutOfDate: false } }));
    viewOption(dialog, 'Full-width');
    expect(dialog.queryByRole('button', { name: /^Choose|Try again/ })).toBeNull();
    expect(dialog.queryByText('Work follows this option.')).toBeNull();
  });

  it('shows the Recommended badge and the chosen marker', () => {
    const dialog = open(makeDesignSet({ chosen: { id: 'b', title: 'Full-width button with Show all' } }));
    const tabA = dialog.getByRole('tab', { name: /Quiet button/ });
    const tabB = dialog.getByRole('tab', { name: /Full-width/ });
    expect(within(tabA).getByText('Recommended')).toBeInTheDocument();
    expect(within(tabB).queryByText('Recommended')).toBeNull();
    expect(within(tabB).getByText('Chosen')).toBeInTheDocument();
  });

  it('shows the server error message when a pick is refused', async () => {
    apiFetch.mockRejectedValue(
      new ApiError(409, JSON.stringify({ message: 'The plan is already being updated.' })),
    );
    const dialog = open(makeDesignSet());
    viewOption(dialog, 'Full-width');
    fireEvent.click(dialog.getByRole('button', { name: 'Choose B and update plan' }));
    expect(await dialog.findByText('The plan is already being updated.')).toBeInTheDocument();
  });
});
