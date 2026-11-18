import { formatTokens, tokenDetail, usageTotal } from '../runs/tokenUsage';
import { useChatSessionUsage } from './useChatSessionUsage';

/**
 * "Tokens 12k" for a chat session. Nothing while no turn is tracked (a
 * connector Coppice cannot read); "not reported" when tracked turns reported
 * nothing.
 */
export function ChatSessionTokens({ sessionId }: { sessionId: string }) {
  const { data } = useChatSessionUsage(sessionId);
  if (!data || data.runs === 0) return null;
  const amount = { tokens: data.tokens, subagentTokens: data.subagentTokens };
  const total = data.runsWithUsage > 0 ? usageTotal(amount) : null;
  return (
    <p
      className="font-body text-xs text-text-secondary"
      data-testid="chat-session-tokens"
      title={total == null ? undefined : tokenDetail(amount.tokens, amount.subagentTokens)}
    >
      Tokens {total == null ? 'not reported' : formatTokens(total)}
    </p>
  );
}
