import type { ContextWindowReading } from './contextWindow';
import { formatTokens } from './tokenUsage';

/** "Context 84k / 200k" with a bar, or "Context 84k" without a window size. */
export function ContextWindowMeter({ used, size }: ContextWindowReading) {
  const label =
    size == null
      ? `Context ${formatTokens(used)}`
      : `Context ${formatTokens(used)} / ${formatTokens(size)}`;
  const exact =
    size == null
      ? `${used.toLocaleString()} tokens in the context window`
      : `${used.toLocaleString()} of ${size.toLocaleString()} tokens in the context window`;
  return (
    <div className="flex items-center gap-2" title={exact}>
      <span className="font-body text-xs text-text-secondary">{label}</span>
      {size == null ? null : (
        <div
          className="h-1 w-24 overflow-hidden rounded-full bg-border"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={size}
          aria-valuenow={Math.min(used, size)}
          aria-label="Context window"
        >
          <div
            className="h-full bg-accent"
            style={{ width: `${Math.min(100, (used / size) * 100)}%` }}
          />
        </div>
      )}
    </div>
  );
}
