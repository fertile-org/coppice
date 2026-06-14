let csrfToken: string | null = null;

export function setCsrfToken(token: string) {
  csrfToken = token;
}

export function withCsrf(
  token: string | null,
  headers: Record<string, string> = {},
): Record<string, string> {
  if (token) {
    headers['X-CSRF-Token'] = token;
  }
  return headers;
}

export function parseApiErrorMessage(
  err: unknown,
  fallback = 'Something went wrong.',
): string {
  if (!(err instanceof ApiError)) {
    return fallback;
  }
  const trimmed = err.body.trim();
  if (!trimmed) {
    return fallback;
  }
  try {
    const parsed = JSON.parse(trimmed) as { message?: unknown };
    if (typeof parsed.message === 'string' && parsed.message.trim()) {
      return parsed.message.trim();
    }
  } catch {
    // Plain-text error body
  }
  return trimmed;
}

export function apiErrorToastMessage(message: string, maxLen = 120): string {
  const firstLine = message.split('\n').find((line) => line.trim())?.trim() ?? message;
  if (firstLine.length <= maxLen) {
    return firstLine;
  }
  return `${firstLine.slice(0, maxLen)}…`;
}

export class ApiError extends Error {
  status: number;
  body: string;

  constructor(status: number, body: string) {
    super(`API error ${status}: ${body}`);
    this.name = 'ApiError';
    this.status = status;
    this.body = body;
  }
}

export async function apiFetch(path: string, init: RequestInit = {}) {
  const headers = withCsrf(csrfToken, {
    ...((init.headers as Record<string, string> | undefined) ?? {}),
  });
  const res = await fetch(path, { ...init, headers, credentials: 'include' });
  if (!res.ok) {
    throw new ApiError(res.status, await res.text());
  }
  return res;
}
