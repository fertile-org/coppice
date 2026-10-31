import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { DesignSet } from '../../lib/schemas/design';
import { DesignPlanCard } from './DesignPlanCard';
import { makeDesignSet } from './designFixtures';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

function serve(sets: DesignSet[]) {
  apiFetch.mockImplementation((path: string) =>
    String(path).endsWith('/designs')
      ? Promise.resolve(new Response(JSON.stringify(sets), { status: 200 }))
      : Promise.resolve(new Response('missing', { status: 404 })),
  );
}

function renderCard() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <DesignPlanCard ticketId="t1" />
    </QueryClientProvider>,
  );
}

describe('DesignPlanCard', () => {
  beforeEach(() => {
    apiFetch.mockReset();
  });

  it('shows the card text', async () => {
    serve([makeDesignSet()]);
    renderCard();
    expect(await screen.findByText('Click to open design options')).toBeInTheDocument();
    expect(
      screen.getByText(
        '3 options · Using A: Quiet button after the last card (recommended)',
      ),
    ).toBeInTheDocument();
  });

  it('says your choice when a human chose', async () => {
    const full = { id: 'b', title: 'Full-width button with Show all' };
    serve([makeDesignSet({ chosen: full, selected: full })]);
    renderCard();
    expect(
      await screen.findByText('3 options · Using B: Full-width button with Show all (your choice)'),
    ).toBeInTheDocument();
  });

  it('shows only the option count for a set with no selection', async () => {
    serve([makeDesignSet({ selected: null, recommended: null })]);
    renderCard();
    expect(await screen.findByText('3 options')).toBeInTheDocument();
  });

  it('prefers the plan-run set and ignores superseded sets', async () => {
    serve([
      makeDesignSet({ id: '33333333-3333-4333-8333-333333333333', title: 'Old', superseded: true }),
      makeDesignSet({ id: '44444444-4444-4444-8444-444444444444', title: 'Work run', fromPlanRun: false, options: [{ id: 'a', title: 'X', entry: 'a/index.html' }], selected: null }),
      makeDesignSet({ id: '55555555-5555-4555-8555-555555555555', title: 'Plan run' }),
    ]);
    renderCard();
    expect(await screen.findByText(/^3 options/)).toBeInTheDocument();
    expect(screen.getAllByText('Click to open design options')).toHaveLength(1);
  });

  it('renders nothing without a set', async () => {
    serve([makeDesignSet({ superseded: true })]);
    const { container } = renderCard();
    await vi.waitFor(() => expect(apiFetch).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it('opens the popup', async () => {
    serve([makeDesignSet()]);
    renderCard();
    fireEvent.click(await screen.findByRole('button', { name: 'Open' }));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
  });
});
