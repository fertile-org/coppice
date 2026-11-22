import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Connector } from '../../lib/schemas/connector';
import type { UsageSummary } from '../../lib/schemas/usage';
import { UsagePage } from './UsagePage';
import type { UsageSummaryParams } from './useUsageSummary';

const counts = (input: number | null, cacheRead: number | null, output: number | null) => ({
  input,
  cacheRead,
  cacheWrite: null,
  output,
  reasoning: null,
});

const summary: UsageSummary = {
  totals: { tokens: counts(1500, 20000, 700), unsplit: 0, total: 22200, runs: 3, activeDays: 2 },
  rows: [
    {
      key: 'claude-code/claude-haiku-5-5',
      label: 'claude-haiku-5-5',
      connector: 'claude-code',
      tokens: counts(1000, null, 500),
      unsplit: 0,
      total: 1500,
      runs: 2,
    },
    {
      key: 'turny/t1',
      label: 't1',
      connector: 'turny',
      tokens: counts(500, null, 200),
      unsplit: 0,
      total: 700,
      runs: 1,
    },
    {
      key: 'claude-code/',
      label: null,
      connector: 'claude-code',
      tokens: counts(0, 20000, 0),
      unsplit: 0,
      total: 20000,
      runs: 1,
    },
  ],
  daily: [
    { day: '2026-11-09', byKey: { 'claude-code/claude-haiku-5-5': 1500 } },
    { day: '2026-11-10', byKey: { 'turny/t1': 700, 'claude-code/': 20000 } },
  ],
  coverage: {
    trackedRuns: 3,
    untrackedRuns: 12,
    untrackedByConnector: [{ connector: 'cursor', runs: 12 }],
  },
  perDoneTicket: { tickets: 2, median: 1500, average: 1100 },
};

const allFields = { input: true, cacheRead: true, cacheWrite: true, output: true, reasoning: true };
const connectors: Connector[] = [
  {
    id: 'claude-code',
    displayName: 'Claude Code',
    console: 'structured',
    caps: { readOnlyTools: true, chatResume: true },
    usageCoverage: {
      granularity: 'request',
      model: 'perRequest',
      fields: allFields,
      subagents: 'totalOnly',
      contextWindow: 'usedOnly',
    },
  },
  {
    id: 'turny',
    displayName: 'Turny',
    console: 'structured',
    caps: { readOnlyTools: true, chatResume: true },
    usageCoverage: {
      granularity: 'turn',
      model: 'runOnly',
      fields: { ...allFields, cacheRead: false },
      subagents: 'none',
      contextWindow: 'none',
    },
  },
  {
    id: 'cursor',
    displayName: 'Cursor',
    console: 'structured',
    caps: { readOnlyTools: true, chatResume: true },
    usageCoverage: null,
  },
];

const mocks = vi.hoisted(() => ({
  role: 'admin',
  params: [] as UsageSummaryParams[],
}));

vi.mock('../auth/useSession', () => ({
  useSession: () => ({ user: { id: 'u1', email: 'a@b.c', role: mocks.role }, loading: false }),
}));

vi.mock('../agents/useAgents', () => ({
  useConnectors: () => ({ data: connectors }),
}));

vi.mock('../boards/useBoards', () => ({
  useBoards: () => ({ data: [{ id: 'b1', name: 'Main board' }] }),
}));

vi.mock('./useUsageSummary', () => ({
  useUsageSummary: (params: UsageSummaryParams) => {
    mocks.params.push(params);
    return { data: summary, isError: false, isLoading: false };
  },
}));

function renderPage() {
  return render(
    <MemoryRouter initialEntries={['/settings/usage']}>
      <Routes>
        <Route path="/settings/usage" element={<UsagePage />} />
        <Route path="/boards" element={<p>Boards page</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

describe('UsagePage', () => {
  beforeEach(() => {
    mocks.role = 'admin';
    mocks.params = [];
  });

  it('shows rows that add up to the total tile', () => {
    renderPage();
    const totalTile = screen.getByText('Total tokens').parentElement!;
    expect(within(totalTile).getByText('22k')).toBeInTheDocument();
    const rows = screen.getAllByRole('row').slice(1);
    const exact = rows.map((row) =>
      Number(within(row).getAllByRole('cell')[5].getAttribute('title')!.replace(/\D/g, '')),
    );
    expect(exact.reduce((a, b) => a + b, 0)).toBe(summary.totals.total);
    expect(within(screen.getByRole('table')).getByText('Model not reported')).toBeInTheDocument();
    expect(screen.getByText('1.5k', { selector: 'p' })).toBeInTheDocument();
  });

  it('asks again when a filter changes', () => {
    renderPage();
    expect(mocks.params.at(-1)).toMatchObject({ groupBy: 'model', board: null, connector: null });
    fireEvent.change(screen.getByLabelText('Group by'), { target: { value: 'agent' } });
    expect(mocks.params.at(-1)?.groupBy).toBe('agent');
    fireEvent.change(screen.getByLabelText('Range'), { target: { value: 'all' } });
    expect(mocks.params.at(-1)).toMatchObject({ from: null, to: null });
    fireEvent.change(screen.getByLabelText('Board'), { target: { value: 'b1' } });
    expect(mocks.params.at(-1)?.board).toBe('b1');
    fireEvent.change(screen.getByLabelText('Connector'), { target: { value: 'claude-code' } });
    expect(mocks.params.at(-1)?.connector).toBe('claude-code');
  });

  it('sends members back to the boards', () => {
    mocks.role = 'member';
    renderPage();
    expect(screen.getByText('Boards page')).toBeInTheDocument();
  });
});
