import { ApiError } from '../../lib/api';

/** Plugin endpoints report failures as `{ "error": msg }`. */
export function pluginErrorMessage(err: unknown, fallback: string): string {
  if (!(err instanceof ApiError)) return fallback;
  try {
    const parsed = JSON.parse(err.body) as { error?: unknown };
    if (typeof parsed.error === 'string' && parsed.error.trim()) {
      return parsed.error.trim();
    }
  } catch {
    // Non-JSON body
  }
  return fallback;
}
