import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { UsageGroup } from '../../lib/schemas/usage';
import { ChatSessionTokens } from './ChatSessionTokens';

const mocks = vi.hoisted(() => ({ data: null as UsageGroup | null }));

vi.mock('./useChatSessionUsage', () => ({
  useChatSessionUsage: () => ({ data: mocks.data }),
}));

const group = (overrides: Partial<UsageGroup>): UsageGroup => ({
  key: 'chat_session',
  runs: 2,
  runsWithUsage: 2,
  runsNotReported: 0,
  partialRuns: 0,
  tokens: { input: 8, cacheRead: 12000, cacheWrite: null, output: 12, reasoning: null },
  subagentTokens: null,
  ...overrides,
});

describe('ChatSessionTokens', () => {
  beforeEach(() => {
    mocks.data = null;
  });

  it("shows the session's total", () => {
    mocks.data = group({});
    render(<ChatSessionTokens sessionId="s1" />);
    expect(screen.getByTestId('chat-session-tokens')).toHaveTextContent('Tokens 12k');
  });

  it('says not reported when tracked turns reported nothing', () => {
    mocks.data = group({
      runsWithUsage: 0,
      runsNotReported: 2,
      tokens: { input: null, cacheRead: null, cacheWrite: null, output: null, reasoning: null },
    });
    render(<ChatSessionTokens sessionId="s1" />);
    expect(screen.getByTestId('chat-session-tokens')).toHaveTextContent('Tokens not reported');
  });

  it('shows nothing when no turn is tracked', () => {
    mocks.data = group({ runs: 0, runsWithUsage: 0 });
    render(<ChatSessionTokens sessionId="s1" />);
    expect(screen.queryByTestId('chat-session-tokens')).not.toBeInTheDocument();
  });
});
