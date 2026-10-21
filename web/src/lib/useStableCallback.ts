import { useCallback, useLayoutEffect, useRef } from 'react';

/**
 * A callback with a stable identity that always calls the latest `fn`. Use it
 * for handlers passed to memoized children. Do not call it during render.
 */
export function useStableCallback<Args extends unknown[], Result>(
  fn: (...args: Args) => Result,
): (...args: Args) => Result {
  const ref = useRef(fn);
  useLayoutEffect(() => {
    ref.current = fn;
  });
  return useCallback((...args: Args) => ref.current(...args), []);
}
