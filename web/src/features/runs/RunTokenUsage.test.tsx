import '@testing-library/jest-dom/vitest';
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RunTokenUsage } from './RunTokenUsage';
import { USAGE_COPY } from './tokenUsage';

const { useRunUsage } = vi.hoisted(() => ({ useRunUsage: vi.fn() }));

vi.mock('./useRunUsage', () => ({ useRunUsage }));

const RUN_ID = '00000000-0000-4000-8000-000000000010';

const tokens = (input: number | null, cacheRead: number | null, output: number | null) => ({
  input,
  cacheRead,
  cacheWrite: null,
  output,
  reasoning: null,
});

describe('RunTokenUsage', () => {
  it('shows the run total, each request, and each subagent', () => {
    useRunUsage.mockReturnValue({
      data: {
        totals: {
          status: 'reported',
          tokens: tokens(18, 2200, 120),
          subagentTokens: null,
          requestCount: 2,
          subagentCount: 2,
        },
        requests: [
          { id: 'r1', granularity: 'request', model: 'claude-haiku-5-5', tokens: tokens(10, 1000, 50) },
          { id: 'r2', granularity: 'request', model: null, tokens: tokens(5, 1200, 30) },
        ],
        subagents: [
          {
            id: 's1',
            agentType: 'general-purpose',
            description: 'Reply with number 4',
            model: 'claude-haiku-5-5',
            status: 'completed',
            tokens: tokens(3, null, 40),
            totalTokens: 12847,
            toolUses: 1,
            durationMs: 900,
          },
          {
            id: 's2',
            agentType: null,
            description: null,
            model: null,
            status: null,
            tokens: tokens(null, null, null),
            totalTokens: 5000,
            toolUses: null,
            durationMs: null,
          },
        ],
      },
      isLoading: false,
      isError: false,
    });

    render(<RunTokenUsage runId={RUN_ID} enabled live />);

    expect(useRunUsage).toHaveBeenCalledWith(RUN_ID, true, 3000);
    expect(screen.getByText('18 in · 2.2k cached · 120 out')).toBeVisible();
    expect(screen.queryByText(USAGE_COPY.partialLive)).not.toBeInTheDocument();

    const requests = screen.getAllByRole('listitem', { name: /request/i });
    expect(requests).toHaveLength(2);
    expect(within(requests[0]).getByText('claude-haiku-5-5')).toBeVisible();
    expect(within(requests[1]).getByText('5 in · 1.2k cached · 30 out')).toBeVisible();

    const first = within(screen.getByRole('listitem', { name: 'Subagent general-purpose' }));
    expect(first.getByText('completed')).toBeVisible();
    expect(first.getByText('Reply with number 4')).toBeVisible();
    expect(first.getByText('3 in · 40 out')).toBeVisible();
    expect(first.getByText('1 tool use')).toBeVisible();
    // Only the CLI's total is known for the second one.
    expect(
      within(screen.getByRole('listitem', { name: 'Subagent s2' })).getByText('5k total'),
    ).toBeVisible();
  });

  it('explains a partial total that ended, with subagents counted apart', () => {
    useRunUsage.mockReturnValue({
      data: {
        totals: {
          status: 'partial',
          tokens: tokens(1, null, 1),
          subagentTokens: 200_000,
          requestCount: 1,
          subagentCount: 1,
        },
        requests: [],
        subagents: [],
      },
      isLoading: false,
      isError: false,
    });
    const { rerender } = render(<RunTokenUsage runId={RUN_ID} enabled />);
    expect(useRunUsage).toHaveBeenLastCalledWith(RUN_ID, true, false);
    expect(screen.getByText('1 in · 1 out · 200k subagents')).toBeVisible();
    expect(screen.getByText(USAGE_COPY.partialEnded)).toBeVisible();

    rerender(<RunTokenUsage runId={RUN_ID} enabled live />);
    expect(screen.getByText(USAGE_COPY.partialLive)).toBeVisible();
  });

  it('tells a tracked run that reported nothing from an untracked one', () => {
    const nothing = {
      totals: {
        status: 'none',
        tokens: tokens(null, null, null),
        subagentTokens: null,
        requestCount: 0,
        subagentCount: 0,
      },
      requests: [],
      subagents: [],
    };
    useRunUsage.mockReturnValue({ data: nothing, isLoading: false, isError: false });
    const { rerender } = render(<RunTokenUsage runId={RUN_ID} enabled live />);
    expect(screen.getByText(USAGE_COPY.nothingYet)).toBeVisible();
    expect(screen.queryByText('Run total')).not.toBeInTheDocument();

    rerender(<RunTokenUsage runId={RUN_ID} enabled />);
    expect(screen.getByText(USAGE_COPY.notReported)).toBeVisible();

    useRunUsage.mockReturnValue({
      data: { totals: null, requests: [], subagents: [] },
      isLoading: false,
      isError: false,
    });
    rerender(<RunTokenUsage runId={RUN_ID} enabled />);
    expect(screen.getByText(USAGE_COPY.noUsage)).toBeVisible();
  });
});
