import '@testing-library/jest-dom/vitest';
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { TicketUsage, TokenCounts, UsageGroup } from '../../lib/schemas/usage';
import { partialRunsNote } from '../runs/tokenUsage';
import { TicketUsageSummary } from './TicketUsageSummary';

const WORKER = '00000000-0000-4000-8000-000000000001';
const PLANNER = '00000000-0000-4000-8000-000000000002';

const out = (output: number | null): TokenCounts => ({
  input: null,
  cacheRead: null,
  cacheWrite: null,
  output,
  reasoning: null,
});

function group(key: string, overrides: Partial<UsageGroup>): UsageGroup {
  return {
    key,
    runs: 1,
    runsWithUsage: 1,
    runsNotReported: 0,
    partialRuns: 0,
    tokens: out(0),
    subagentTokens: null,
    ...overrides,
  };
}

function usage(overrides: Partial<TicketUsage>): TicketUsage {
  return {
    runs: [],
    own: group('ticket', { runs: 3, runsWithUsage: 2, runsNotReported: 1, tokens: out(150_000) }),
    subtickets: null,
    byAgent: [
      group(WORKER, { runs: 2, runsNotReported: 1, tokens: out(110_000) }),
      group(PLANNER, { tokens: out(40_000) }),
    ],
    byJobType: [],
    ...overrides,
  };
}

const names: Record<string, string> = { [WORKER]: 'Worker', [PLANNER]: 'Planner' };

describe('TicketUsageSummary', () => {
  it('shows the ticket total, runs that reported nothing, and agents', () => {
    render(<TicketUsageSummary usage={usage({})} agentName={(id) => names[id]} />);
    const section = within(screen.getByRole('region', { name: 'Token usage' }));
    expect(section.getByText('This ticket')).toBeVisible();
    expect(section.getByText('150k')).toBeVisible();
    expect(section.getByText('1 run reported no tokens.')).toBeVisible();
    const agents = within(section.getByRole('list', { name: 'Tokens by agent' }));
    expect(agents.getByText('Worker')).toBeVisible();
    expect(agents.getByText('×2')).toBeVisible();
    expect(agents.getByText('(1 not reported)')).toBeVisible();
    expect(agents.getByText('110k')).toBeVisible();
    expect(section.queryByText(/Including/)).not.toBeInTheDocument();
  });

  it('adds subtickets as a second total', () => {
    render(
      <TicketUsageSummary
        usage={usage({
          subtickets: {
            ticketCount: 2,
            runsWithUsage: 3,
            partialRuns: 0,
            tokens: out(50_000),
            subagentTokens: null,
          },
        })}
        agentName={(id) => names[id]}
      />,
    );
    expect(screen.getByText('Including 2 subtickets')).toBeVisible();
    expect(screen.getByText('200k')).toBeVisible();
  });

  it('counts subagents without a breakdown and notes partial runs', () => {
    render(
      <TicketUsageSummary
        usage={usage({
          own: group('ticket', {
            partialRuns: 1,
            tokens: out(5_000),
            subagentTokens: 200_000,
          }),
          subtickets: {
            ticketCount: 1,
            runsWithUsage: 1,
            partialRuns: 1,
            tokens: out(1_000),
            subagentTokens: null,
          },
        })}
        agentName={(id) => names[id]}
      />,
    );
    expect(screen.getByText('205k')).toBeVisible();
    expect(screen.getByText('5k out · 200k subagents')).toBeVisible();
    expect(screen.getByText('206k')).toBeVisible();
    expect(screen.getByText(partialRunsNote(2))).toBeVisible();
    expect(screen.queryByText(/reported no tokens/)).not.toBeInTheDocument();
  });

  it('says not reported when a total has no tokens', () => {
    render(
      <TicketUsageSummary
        usage={usage({
          own: group('ticket', { runs: 2, runsWithUsage: 1, tokens: out(null) }),
          subtickets: {
            ticketCount: 1,
            runsWithUsage: 1,
            partialRuns: 0,
            tokens: out(50_000),
            subagentTokens: null,
          },
        })}
        agentName={(id) => names[id]}
      />,
    );
    expect(screen.getByText('not reported')).toBeVisible();
  });

  it('renders nothing when no run reported usage', () => {
    const { container } = render(
      <TicketUsageSummary
        usage={usage({
          own: group('ticket', { runsWithUsage: 0, runsNotReported: 1, tokens: out(null) }),
        })}
        agentName={(id) => names[id]}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
