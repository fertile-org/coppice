/** How full the model's context window is, from a `usage.contextWindow` event. */
export interface ContextWindowReading {
  used: number;
  /** `null` when the connector does not report the window's size. */
  size: number | null;
}

/** The reading in a live run event, or `null` for any other event. */
export function contextWindowFromEvent(
  event: Record<string, unknown>,
): ContextWindowReading | null {
  if (event.type !== 'usage.contextWindow') return null;
  const { used, size } = event;
  if (typeof used !== 'number' || used < 0) return null;
  return {
    used,
    size: typeof size === 'number' && size > 0 ? size : null,
  };
}
