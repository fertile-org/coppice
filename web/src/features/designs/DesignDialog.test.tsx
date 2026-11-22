import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../lib/api';
import type { DesignSet } from '../../lib/schemas/design';
import { DesignDialog } from './DesignDialog';
import { makeDesignSet, makeOption, makePick } from './designFixtures';

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

  it('hides choose actions in plan_running, plan_final, and read_only modes', () => {
    for (const mode of ['plan_running', 'plan_final', 'read_only'] as const) {
      const dialog = open(makeDesignSet({ choice: { mode, planOutOfDate: false } }));
      viewOption(dialog, 'Full-width');
      expect(dialog.queryByRole('button', { name: /^Choose|Try again/ })).toBeNull();
      cleanup();
    }
  });

  it('save_only work-run or chat set: the recommended option can be chosen, and saves', async () => {
    const dialog = open(
      makeDesignSet({ fromPlanRun: false, choice: { mode: 'save_only', planOutOfDate: false } }),
    );
    expect(dialog.queryByText('Work follows this option.')).toBeNull();
    fireEvent.click(dialog.getByRole('button', { name: 'Choose A' }));
    await waitFor(() =>
      expect(apiFetch).toHaveBeenCalledWith(
        expect.stringMatching(/\/choose$/),
        expect.objectContaining({ body: JSON.stringify({ optionId: 'a' }) }),
      ),
    );
  });

  it("save_only work-run or chat set: work follows only the human's choice", () => {
    const full = makePick(makeOption('b', 'Full-width button with Show all', 'B'));
    const dialog = open(
      makeDesignSet({
        fromPlanRun: false,
        chosen: full,
        selected: full,
        choice: { mode: 'save_only', planOutOfDate: false },
      }),
    );
    expect(dialog.getByText('Work follows this option.')).toBeInTheDocument();
    viewOption(dialog, 'Quiet button');
    expect(dialog.queryByText('Work follows this option.')).toBeNull();
    expect(dialog.getByRole('button', { name: 'Choose A' })).toBeInTheDocument();
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

  describe('slug ids and prefixed titles', () => {
    const quiet = makeOption('option-a', 'A: Quiet', 'A', 'Quiet');
    const full = makeOption('option-b', 'B: Full-width', 'B', 'Full-width');
    const slugSet = (overrides: Partial<DesignSet> = {}) =>
      makeDesignSet({
        options: [quiet, full],
        recommended: makePick(quiet),
        selected: makePick(quiet),
        ...overrides,
      });

    it('labels tabs "B: Full-width" and the buttons "Choose B"', async () => {
      const dialog = open(slugSet());
      expect(dialog.getByRole('tab', { name: /^A: Quiet/ })).toBeInTheDocument();
      expect(dialog.getByRole('tab', { name: /^B: Full-width/ })).toBeInTheDocument();
      expect(dialog.queryByText(/OPTION-/i)).toBeNull();
      fireEvent.click(dialog.getByRole('tab', { name: /^B: Full-width/ }));
      fireEvent.click(dialog.getByRole('button', { name: 'Choose B and update plan' }));
      await waitFor(() =>
        expect(apiFetch).toHaveBeenCalledWith(
          expect.stringMatching(/\/choose$/),
          expect.objectContaining({ body: JSON.stringify({ optionId: 'option-b' }) }),
        ),
      );
    });

  });
});
