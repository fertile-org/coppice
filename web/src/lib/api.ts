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
