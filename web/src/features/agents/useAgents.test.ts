import { afterEach, describe, expect, it, vi } from 'vitest';
import { fetchConnectors } from './useAgents';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', () => ({ apiFetch }));

afterEach(() => {
  apiFetch.mockReset();
});

describe('fetchConnectors', () => {
  it('parses connector descriptors from /api/connectors', async () => {
    apiFetch.mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [
            {
              id: 'opencode',
              displayName: 'OpenCode',
              console: 'openCodeSession',
              caps: { readOnlyTools: false, chatResume: true },
            },
            {
              id: 'mock',
              displayName: 'Mock',
              console: 'plain',
              caps: { readOnlyTools: true, chatResume: true },
            },
          ],
        }),
      ),
    );

    const items = await fetchConnectors();

    expect(apiFetch).toHaveBeenCalledWith('/api/connectors');
    expect(items.map((c) => c.id)).toEqual(['opencode', 'mock']);
    expect(items[0]).toEqual({
      id: 'opencode',
      displayName: 'OpenCode',
      console: 'openCodeSession',
      caps: { readOnlyTools: false, chatResume: true },
    });
  });

  it('rejects an unknown console kind', async () => {
    apiFetch.mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [
            {
              id: 'x',
              displayName: 'X',
              console: 'fancy',
              caps: { readOnlyTools: false, chatResume: false },
            },
          ],
        }),
      ),
    );

    await expect(fetchConnectors()).rejects.toThrow();
  });
});
