import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setCsrfToken } from '../../lib/api';
import { pluginSchema, type Plugin, type PluginTestResult } from '../../lib/schemas/plugin';
import { PluginCard } from './PluginCard';

const plugin: Plugin = {
  id: '00000000-0000-4000-8000-000000000010',
  pluginDirId: '00000000-0000-4000-8000-000000000001',
  relPath: 'mcp-fake',
  name: 'mcp-fake',
  version: '0.1.0',
  description: '',
  source: 'local',
  gitUrl: null,
  gitRef: null,
  gitCommit: null,
  status: 'ok',
  error: null,
  enabled: true,
  skills: [],
  mcpServers: [
    { name: 'fake', kind: 'stdio', health: 'unhealthy' },
    { name: 'remote', kind: 'http', health: 'ready' },
  ],
  settings: [
    { key: 'API_TOKEN', configured: true },
    { key: 'ROOT', configured: false },
  ],
  unsupported: [],
};

const testResult: PluginTestResult = {
  servers: [
    {
      name: 'fake',
      kind: 'stdio',
      status: 'ok',
      tools: [
        {
          name: 'echo',
          exposedName: 'mcp-fake__echo',
          description: 'Echo input',
          readOnly: true,
        },
      ],
    },
    { name: 'remote', kind: 'http', status: 'error', error: 'missing setting "X"', tools: [] },
  ],
};

const STDIO_WARNING =
  "This plugin starts local MCP servers that run with the Coppice server's privileges until sandboxing lands (M11). Enable anyway?";

const fetchMock = vi.fn();

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function renderCard(target: Plugin = plugin) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <PluginCard plugin={target} />
    </QueryClientProvider>,
  );
}

function callFor(path: string) {
  const call = fetchMock.mock.calls.find(([p]) => p === path) as
    | [string, RequestInit]
    | undefined;
  expect(call).toBeDefined();
  return call![1];
}

describe('PluginCard', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
    setCsrfToken('csrf-token');
    fetchMock.mockImplementation((path: string, init: RequestInit = {}) => {
      const method = init.method ?? 'GET';
      if (path === `/api/plugins/${plugin.id}/settings` && method === 'PUT') {
        return Promise.resolve(json(plugin));
      }
      if (path === `/api/plugins/${plugin.id}/test` && method === 'POST') {
        return Promise.resolve(json(testResult));
      }
      if (path === '/api/plugins' && method === 'GET') {
        return Promise.resolve(json([plugin]));
      }
      return Promise.reject(new Error(`unexpected ${method} ${path}`));
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('renders settings with configured badges', () => {
    renderCard();

    const token = screen.getByLabelText('API_TOKEN');
    const root = screen.getByLabelText('ROOT');
    expect(token).toHaveAttribute('type', 'password');
    expect(root).toHaveAttribute('type', 'password');
    expect(token).toHaveValue('');
    const tokenRow = screen.getByTestId('plugin-setting-API_TOKEN');
    expect(within(tokenRow).getByText('Configured')).toBeVisible();
    const rootRow = screen.getByTestId('plugin-setting-ROOT');
    expect(within(rootRow).queryByText('Configured')).not.toBeInTheDocument();
  });

  it('saves a setting', async () => {
    renderCard();

    fireEvent.change(screen.getByLabelText('ROOT'), { target: { value: 'v' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save settings' }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/plugins/${plugin.id}/settings`,
        expect.anything(),
      ),
    );
    const init = callFor(`/api/plugins/${plugin.id}/settings`);
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body as string)).toEqual({ values: { ROOT: 'v' } });
    expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('csrf-token');
  });

  it('clears a setting', async () => {
    renderCard();

    fireEvent.click(screen.getByRole('button', { name: 'Clear API_TOKEN' }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/plugins/${plugin.id}/settings`,
        expect.anything(),
      ),
    );
    const init = callFor(`/api/plugins/${plugin.id}/settings`);
    expect(JSON.parse(init.body as string)).toEqual({ values: { API_TOKEN: '' } });
  });

  it('shows the server error when saving settings fails', async () => {
    fetchMock.mockImplementation(() =>
      Promise.resolve(json({ error: 'unknown setting "ROOT"' }, 400)),
    );
    renderCard();

    fireEvent.change(screen.getByLabelText('ROOT'), { target: { value: 'v' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save settings' }));

    expect(await screen.findByText('unknown setting "ROOT"')).toBeVisible();
  });

  it('test button shows tools and errors', async () => {
    renderCard();

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));

    const results = await screen.findByTestId('plugin-test-results');
    const echo = within(results).getByText('mcp-fake__echo');
    expect(echo).toBeVisible();
    expect(within(echo.closest('li')!).getByText('read-only')).toBeVisible();
    expect(within(results).getByText('missing setting "X"')).toBeVisible();
    const init = callFor(`/api/plugins/${plugin.id}/test`);
    expect(init.method).toBe('POST');
    expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('csrf-token');
  });

  it('testing a disabled stdio plugin asks first; cancel sends nothing', () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    renderCard({ ...plugin, enabled: false });

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));

    expect(confirm).toHaveBeenCalledWith(STDIO_WARNING);
    expect(fetchMock).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it('testing a disabled stdio plugin posts after confirmation', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    renderCard({ ...plugin, enabled: false });

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));

    expect(await screen.findByTestId('plugin-test-results')).toBeVisible();
    expect(confirm).toHaveBeenCalledWith(STDIO_WARNING);
    expect(callFor(`/api/plugins/${plugin.id}/test`).method).toBe('POST');
    confirm.mockRestore();
  });

  it('testing an enabled plugin does not confirm', async () => {
    const confirm = vi.spyOn(window, 'confirm');
    renderCard();

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));

    expect(await screen.findByTestId('plugin-test-results')).toBeVisible();
    expect(confirm).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it('shows unsupported servers and test conflicts', async () => {
    fetchMock.mockImplementationOnce(() =>
      Promise.resolve(
        json({ servers: [{ name: 'legacy', kind: 'sse', status: 'unsupported', tools: [] }] }),
      ),
    );
    renderCard();

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));
    const results = await screen.findByTestId('plugin-test-results');
    expect(within(results).getByText('not supported')).toBeVisible();

    fetchMock.mockImplementationOnce(() =>
      Promise.resolve(json({ error: 'plugin is not ok' }, 409)),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Test' }));
    expect(await screen.findByText('plugin is not ok')).toBeVisible();
  });

  it('shows server health', () => {
    renderCard();

    const row = screen.getByTestId('plugin-mcp-server-fake');
    expect(within(row).getByText('fake')).toBeVisible();
    expect(within(row).getByText('unhealthy')).toBeVisible();
  });

  it('unknown health falls back', () => {
    const parsed = pluginSchema.parse({
      ...plugin,
      mcpServers: [{ name: 'fake', kind: 'websocket', health: 'weird' }],
    });
    expect(parsed.mcpServers[0].health).toBe('stopped');
    expect(parsed.mcpServers[0].kind).toBe('unknown');
  });

  it('settings default to empty when absent', () => {
    const raw: Partial<Plugin> = { ...plugin };
    delete raw.settings;
    expect(pluginSchema.parse(raw).settings).toEqual([]);
  });
});
