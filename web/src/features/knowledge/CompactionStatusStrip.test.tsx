import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Connector } from '../../lib/schemas/connector';
import type { CompactionBatch, CompactionStatus } from '../../lib/schemas/knowledge';
import { CompactionStatusStrip } from './CompactionStatusStrip';

const mocks = vi.hoisted(() => ({
  status: null as CompactionStatus | null,
  run: vi.fn(),
  retry: vi.fn(),
  cancel: vi.fn(),
}));

vi.mock('./useCompaction', () => ({
  useCompactionStatus: () => ({ data: mocks.status }),
  useCompactNow: () => ({ mutateAsync: mocks.run, isPending: false }),
  useRetryCompaction: () => ({ mutateAsync: mocks.retry, isPending: false }),
  useCancelCompaction: () => ({ mutateAsync: mocks.cancel, isPending: false }),
}));

vi.mock('../runs/LiveConsole', () => ({
  LiveConsole: ({ runId }: { runId: string | null }) => (
    <div data-testid="live-console">console {runId}</div>
  ),
}));

vi.mock('../runs/ClaudeLiveConsole', () => ({
  ClaudeLiveConsole: ({ runId }: { runId: string | null }) => (
    <div data-testid="live-console">claude console {runId}</div>
  ),
}));

vi.mock('../runs/LiveSession', () => ({
  LiveSession: ({ runId }: { runId: string | null }) => (
    <div data-testid="live-console">session {runId}</div>
  ),
}));

const connectorsState = vi.hoisted(() => ({
  connectors: undefined as Connector[] | undefined,
}));

vi.mock('../agents/useAgents', () => ({
  useConnectors: () => ({ data: connectorsState.connectors }),
}));

const AGENT = {
  id: '00000000-0000-4000-8000-000000000001',
  name: 'Reviewer',
  enabled: true,
  connector: 'mock',
};
const RUN_ID = '00000000-0000-4000-8000-000000000009';

function batch(overrides: Partial<CompactionBatch> = {}): CompactionBatch {
  return {
    id: '00000000-0000-4000-8000-000000000002',
    agentId: AGENT.id,
    agentName: AGENT.name,
    runId: RUN_ID,
    status: 'running',
    trigger: 'manual',
    ticketCount: 4,
    candidateCount: null,
    summary: null,
    errorMessage: null,
    createdAt: new Date(Date.now() - 120_000).toISOString(),
    startedAt: new Date(Date.now() - 60_000).toISOString(),
    endedAt: null,
    ...overrides,
  };
}

function status(overrides: Partial<CompactionStatus>): CompactionStatus {
  return {
    state: 'idle',
    configured: true,
    agent: AGENT,
    queuedCount: 3,
    blockedCount: 0,
    oldestQueuedAt: null,
    activeBatch: null,
    lastBatch: null,
    nextScheduledAt: new Date(Date.now() + 25 * 60_000).toISOString(),
    intervalSecs: 1800,
    batchMaxTickets: 10,
    ...overrides,
  };
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{`${location.pathname}${location.hash}`}</div>;
}

function renderStrip() {
  render(
    <MemoryRouter initialEntries={['/knowledge']}>
      <Routes>
        <Route path="/knowledge" element={<CompactionStatusStrip />} />
        <Route path="/agents" element={<LocationProbe />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  connectorsState.connectors = [
    {
      id: 'mock',
      displayName: 'Mock',
      console: 'plain',
      caps: { readOnlyTools: true, chatResume: true },
    },
  ];
  mocks.run.mockResolvedValue(batch({ status: 'queued' }));
  mocks.retry.mockResolvedValue(batch({ status: 'queued', trigger: 'retry' }));
  mocks.cancel.mockResolvedValue(batch({ status: 'failed', errorMessage: 'cancelled' }));
});

describe('CompactionStatusStrip', () => {
  it('warns when compaction is off and links to the Agents card', () => {
    mocks.status = status({ state: 'not_configured', configured: false, agent: null, queuedCount: 12 });
    renderStrip();
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Knowledge compaction is off. 12 Done tickets are waiting; choose a compaction agent',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Choose agent' }));
    expect(screen.getByTestId('location')).toHaveTextContent('/agents#knowledge-compaction');
  });

  it('shows a paused banner for a disabled agent', () => {
    mocks.status = status({ state: 'agent_disabled', agent: { ...AGENT, enabled: false } });
    renderStrip();
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Compaction is paused — Reviewer is disabled. 3 tickets waiting.',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Open agents' }));
    expect(screen.getByTestId('location')).toHaveTextContent('/agents#knowledge-compaction');
  });

  it('shows idle cadence and compacts on demand', async () => {
    mocks.status = status({
      lastBatch: batch({
        status: 'succeeded',
        candidateCount: 2,
        endedAt: new Date(Date.now() - 3 * 60_000).toISOString(),
      }),
    });
    renderStrip();
    const strip = screen.getByRole('status');
    expect(strip).toHaveTextContent('Compacted by Reviewer every 30 min');
    expect(strip).toHaveTextContent('3 tickets waiting');
    expect(strip).toHaveTextContent('last run 3 minutes ago, 2 candidates');
    expect(strip).toHaveTextContent('next run in 25 minutes');
    fireEvent.click(screen.getByRole('button', { name: 'Compact now' }));
    await waitFor(() => expect(mocks.run).toHaveBeenCalled());
  });

  it('disables Compact now when nothing is waiting', () => {
    mocks.status = status({ queuedCount: 0, nextScheduledAt: null });
    renderStrip();
    expect(screen.getByRole('button', { name: 'Compact now' })).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent('no runs yet');
  });

  it('shows the running batch with live console and cancel', async () => {
    mocks.status = status({ state: 'running', queuedCount: 0, activeBatch: batch() });
    renderStrip();
    expect(screen.getByRole('status')).toHaveTextContent(
      'Reviewer is compacting 4 tickets… started 1 minute ago',
    );
    fireEvent.click(screen.getByRole('button', { name: 'View run' }));
    expect(screen.getByRole('dialog')).toHaveTextContent(`console ${RUN_ID}`);
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(mocks.cancel).toHaveBeenCalled());
  });

  it('uses the structured console when the agent connector is structured', () => {
    connectorsState.connectors = [
      {
        id: 'acme-structured',
        displayName: 'Acme',
        console: 'structured',
        caps: { readOnlyTools: true, chatResume: true },
      },
    ];
    mocks.status = status({
      state: 'running',
      queuedCount: 0,
      agent: { ...AGENT, connector: 'acme-structured' },
      activeBatch: batch(),
    });
    renderStrip();
    fireEvent.click(screen.getByRole('button', { name: 'View run' }));
    expect(screen.getByRole('dialog')).toHaveTextContent(`claude console ${RUN_ID}`);
  });

  it('shows a failure banner with retry', async () => {
    mocks.status = status({
      state: 'failed',
      lastBatch: batch({
        status: 'failed',
        errorMessage: 'connector `mock` knowledge compaction failed: invalid fixture',
      }),
    });
    renderStrip();
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Last compaction failed: connector `mock` knowledge compaction failed: invalid fixture. 4 tickets not compacted.',
    );
    expect(screen.getByRole('button', { name: 'View run' })).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(mocks.retry).toHaveBeenCalled());
  });

  it('treats a cancelled batch as re-runnable rather than retryable', () => {
    mocks.status = status({
      state: 'failed',
      lastBatch: batch({ status: 'failed', errorMessage: 'cancelled' }),
    });
    renderStrip();
    expect(screen.getByRole('alert')).toHaveTextContent('Last compaction was cancelled.');
    expect(screen.queryByRole('button', { name: 'Retry' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Compact now' })).toBeEnabled();
  });
});
