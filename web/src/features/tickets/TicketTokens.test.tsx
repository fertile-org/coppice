import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TicketUsage, TokenCounts, UsageGroup } from '../../lib/schemas/usage';
import { partialRunsNote } from '../runs/tokenUsage';
import { TicketTokens } from './TicketTokens';

const usageState = vi.hoisted(() => ({ data: undefined as TicketUsage | undefined }));

vi.mock('./useTicketUsage', () => ({
  useTicketUsage: () => ({ data: usageState.data }),
}));

const TICKET = '00000000-0000-4000-8000-0000000000aa';

const tokens = (input: number, cacheRead: number, output: number): TokenCounts => ({
  input,
  cacheRead,
  cacheWrite: 0,
  output,
  reasoning: null,
});

function own(overrides: Partial<UsageGroup> = {}): UsageGroup {
  return {
    key: 'ticket',
    runs: 3,
    runsWithUsage: 2,
    runsNotReported: 0,
    partialRuns: 0,
    tokens: tokens(38_000, 360_000, 14_000),
    subagentTokens: null,
    ...overrides,
  };
}

const subtickets = (
  ticketCount: number,
  runsWithUsage: number,
  counts: TokenCounts,
  partialRuns = 0,
) => ({ ticketCount, runsWithUsage, partialRuns, tokens: counts, subagentTokens: null });

function usage(overrides: Partial<TicketUsage> = {}): TicketUsage {
  return {
    runs: [],
    own: own(),
    subtickets: null,
    byAgent: [],
    byJobType: [],
    ...overrides,
  };
}

describe('TicketTokens', () => {
  beforeEach(() => {
    usageState.data = undefined;
  });

  it('shows the ticket total with its breakdown', () => {
    usageState.data = usage();
    render(<TicketTokens ticketId={TICKET} />);
    expect(screen.getByText('Tokens')).toBeVisible();
    expect(screen.getByText('412k')).toBeVisible();
    expect(screen.getByText('(38k in · 360k cached · 14k out)')).toBeVisible();
    expect(screen.queryByText(/Incl\./)).not.toBeInTheDocument();
  });

  it('adds subtickets as a second line', () => {
    usageState.data = usage({
      subtickets: subtickets(2, 4, tokens(10_000, 80_000, 8_000)),
    });
    render(<TicketTokens ticketId={TICKET} />);
    expect(screen.getByText('This ticket:')).toBeVisible();
    expect(screen.getByText('412k')).toBeVisible();
    expect(screen.getByText('Incl. 2 subtickets:')).toBeVisible();
    expect(screen.getByText('510k')).toBeVisible();
  });

  it('shows subtickets alone when only they reported usage', () => {
    usageState.data = usage({
      own: own({ runs: 1, runsWithUsage: 0, tokens: tokens(0, 0, 0) }),
      subtickets: subtickets(1, 1, tokens(1_000, 0, 500)),
    });
    render(<TicketTokens ticketId={TICKET} />);
    expect(screen.getByText('Incl. 1 subticket:')).toBeVisible();
  });

  it('renders nothing before usage loads or when no run reported any', () => {
    const { container, rerender } = render(<TicketTokens ticketId={TICKET} />);
    expect(container).toBeEmptyDOMElement();

    usageState.data = usage({
      own: own({ runs: 2, runsWithUsage: 0, runsNotReported: 2, tokens: tokens(0, 0, 0) }),
    });
    rerender(<TicketTokens ticketId={TICKET} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('counts subagents without a breakdown and notes partial runs', () => {
    usageState.data = usage({
      own: own({ partialRuns: 1, subagentTokens: 200_000 }),
      subtickets: subtickets(1, 1, tokens(0, 0, 0), 1),
    });
    render(<TicketTokens ticketId={TICKET} />);
    expect(screen.getAllByText('612k')).toHaveLength(2);
    expect(screen.getByText(partialRunsNote(2))).toBeVisible();
  });

  it('opens the runs tab from the link', () => {
    usageState.data = usage();
    const onViewRuns = vi.fn();
    render(<TicketTokens ticketId={TICKET} onViewRuns={onViewRuns} />);
    fireEvent.click(screen.getByRole('button', { name: 'View runs →' }));
    expect(onViewRuns).toHaveBeenCalledTimes(1);
  });
});
