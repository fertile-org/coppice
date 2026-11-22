import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import { setCsrfToken } from '../../lib/api';
import type { Plugin, PluginTestResult } from '../../lib/schemas/plugin';
import { PluginDetails } from './PluginDetails';
import { PluginRow } from './PluginRow';

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
    { key: 'API_TOKEN', configured: true, source: 'setting' },
    { key: 'ROOT', configured: false, source: 'missing' },
  ],
  unsupported: [],
  marketplace: null,
  external: null,
  gitRoot: null,
  builderMade: false,
  publishTarget: null,
  agentAccess: 'explicit',
  agentIds: [],
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
  "Local MCP servers run on your computer with your computer's privileges. Enable anyway?";

const fetchMock = vi.fn();

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function renderCard(
  target: Plugin = plugin,
  props: {
    onInstallFromGit?: (url: string) => void;
    siblingCount?: number;
    highlighted?: boolean;
  } = {},
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConfirmProvider>
          <ul>
            <PluginRow
              plugin={target}
              selected={false}
              highlighted={props.highlighted ?? false}
              actionError={null}
              onOpen={() => undefined}
            />
          </ul>
          <PluginDetails
            plugin={target}
            onInstallFromGit={props.onInstallFromGit}
            siblingCount={props.siblingCount}
            updating={false}
            onUpdate={() => undefined}
          />
        </ConfirmProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function renderDetails(target: Plugin = plugin) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConfirmProvider>
          <PluginDetails
            plugin={target}
            updating={false}
            onUpdate={() => undefined}
          />
        </ConfirmProvider>
      </MemoryRouter>
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

describe('PluginDetails', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
    setCsrfToken('csrf-token');
    fetchMock.mockImplementation((path: string, init: RequestInit = {}) => {
      const method = init.method ?? 'GET';
      if (path === `/api/plugins/${plugin.id}/settings` && method === 'PUT') {
        return Promise.resolve(json(plugin));
      }
      if (path === `/api/plugins/${plugin.id}` && method === 'PATCH') {
        return Promise.resolve(json({ ...plugin, enabled: true }));
      }
      if (path === `/api/plugins/${plugin.id}/test` && method === 'POST') {
        return Promise.resolve(json(testResult));
      }
      if (path === '/api/plugins' && method === 'GET') {
        return Promise.resolve(json([plugin]));
      }
      if (path === '/api/agents' && method === 'GET') {
        return Promise.resolve(json({ items: [] }));
      }
      return Promise.reject(new Error(`unexpected ${method} ${path}`));
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('asks for a plugin token again when the backup left it out', () => {
    renderCard({
      ...plugin,
      settings: [
        {
          key: 'API_TOKEN',
          configured: false,
          source: 'missing',
          needsReentry: true,
        },
      ],
    });

    const field = screen.getByLabelText('API_TOKEN');
    expect(field).toHaveAttribute(
      'placeholder',
      "Paste this token again. Backups don't include saved tokens.",
    );
    const row = screen.getByTestId('plugin-setting-API_TOKEN');
    expect(within(row).queryByText('Configured')).not.toBeInTheDocument();
    expect(within(row).queryByText('Missing')).not.toBeInTheDocument();
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

  it('Test button warns on stdio enablement and posts when confirmed or already enabled', async () => {
    renderCard({ ...plugin, enabled: false });
    fireEvent.click(screen.getByRole('button', { name: 'Test' }));
    const dialog = await screen.findByRole('dialog', { name: STDIO_WARNING });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(fetchMock).not.toHaveBeenCalledWith(
      `/api/plugins/${plugin.id}/test`,
      expect.anything(),
    );

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));
    fireEvent.click(
      within(await screen.findByRole('dialog', { name: STDIO_WARNING })).getByRole('button', {
        name: 'Confirm',
      }),
    );
    expect(await screen.findByTestId('plugin-test-results')).toBeVisible();
    expect(callFor(`/api/plugins/${plugin.id}/test`).method).toBe('POST');
  });

  it('enabling a plugin with env-sourced settings names those keys', async () => {
    renderCard({
      ...plugin,
      enabled: false,
      mcpServers: [{ name: 'remote', kind: 'http', health: 'stopped' }],
      settings: [
        { key: 'API_TOKEN', configured: true, source: 'setting' },
        { key: 'GITHUB_TOKEN', configured: false, source: 'env' },
      ],
    });

    fireEvent.click(screen.getByRole('switch', { name: 'Enable mcp-fake' }));
    const dialog = await screen.findByRole('dialog', {
      name: "Setting GITHUB_TOKEN is not set here, so your computer's environment value for it will be sent to the plugin. Enable anyway?",
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));

    await waitFor(() => expect(callFor(`/api/plugins/${plugin.id}`).method).toBe('PATCH'));
  });

  it('stdio and env risks share one confirmation; cancel sends nothing', async () => {
    renderCard({
      ...plugin,
      enabled: false,
      settings: [
        { key: 'A', configured: false, source: 'env' },
        { key: 'B', configured: false, source: 'env' },
      ],
    });

    fireEvent.click(screen.getByRole('switch', { name: 'Enable mcp-fake' }));
    const dialog = await screen.findByRole('dialog', {
      name: "Local MCP servers run on your computer with your computer's privileges. Settings A, B are not set here, so your computer's environment values for them will be sent to the plugin. Enable anyway?",
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(fetchMock).not.toHaveBeenCalledWith(
      `/api/plugins/${plugin.id}`,
      expect.anything(),
    );
  });

  it('shows unsupported servers and test conflicts', async () => {
    renderCard();
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith('/api/agents', expect.anything()));
    fetchMock.mockImplementationOnce(() =>
      Promise.resolve(
        json({ servers: [{ name: 'legacy', kind: 'sse', status: 'unsupported', tools: [] }] }),
      ),
    );

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));
    const results = await screen.findByTestId('plugin-test-results');
    expect(within(results).getByText('not supported')).toBeVisible();

    fetchMock.mockImplementationOnce(() =>
      Promise.resolve(json({ error: 'plugin is not ok' }, 409)),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Test' }));
    expect(await screen.findByText('plugin is not ok')).toBeVisible();
  });

  it('shows the plugin load error in full in the details, not only the row', () => {
    const message =
      'plugin name `coppice` is reserved and the manifest could not be read from disk';
    renderDetails({ ...plugin, status: 'invalid', error: message, mcpServers: [], settings: [] });

    expect(screen.getByText(message)).toBeVisible();
    expect(screen.queryByTestId(`plugin-row-${plugin.id}`)).not.toBeInTheDocument();
  });

  it('summarizes agent access and updates explicit assignment', async () => {
    renderCard({
      ...plugin,
      agentIds: [
        '00000000-0000-4000-8000-0000000000aa',
        '00000000-0000-4000-8000-0000000000bb',
      ],
    });
    expect(screen.getByTestId('plugin-agent-access-summary')).toHaveTextContent('2 agents');
    cleanup();

    const ada = '00000000-0000-4000-8000-0000000000aa';
    fetchMock.mockImplementation((path: string, init: RequestInit = {}) => {
      const method = init.method ?? 'GET';
      if (path === '/api/agents' && method === 'GET') {
        return Promise.resolve(
          json({
            items: [
              {
                id: ada,
                name: 'Ada',
                role: 'Engineer',
                responsibilities: [],
                systemPrompt: '',
                connector: 'mock',
                health: 'healthy',
                enabled: true,
                createdAt: '2026-10-01T00:00:00Z',
                updatedAt: '2026-10-01T00:00:00Z',
              },
            ],
          }),
        );
      }
      if (path === `/api/plugins/${plugin.id}/agents` && method === 'PUT') {
        return Promise.resolve(json({ mode: 'explicit', agentIds: [ada] }));
      }
      return Promise.reject(new Error(`unexpected ${method} ${path}`));
    });
    renderCard({ ...plugin, agentAccess: 'all', agentIds: [] });

    expect(screen.getByText("Changes apply from each agent's next run.")).toBeVisible();
    await waitFor(() =>
      expect(screen.getByRole('radio', { name: 'Choose agents' })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole('radio', { name: 'Choose agents' }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/plugins/${plugin.id}/agents`,
        expect.anything(),
      ),
    );
    const init = callFor(`/api/plugins/${plugin.id}/agents`);
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body as string)).toEqual({ mode: 'explicit', agentIds: [ada] });
  });

  it('skill switches show the on count and PUT the change', async () => {
    const withSkills: Plugin = {
      ...plugin,
      skills: [
        { name: 'a', description: '', relPath: 'skills/a/SKILL.md', error: null, enabled: true },
        { name: 'b', description: '', relPath: 'skills/b/SKILL.md', error: null, enabled: false },
      ],
    };
    fetchMock.mockImplementation((path: string, init: RequestInit = {}) => {
      if (path === `/api/plugins/${plugin.id}/skills/b` && init.method === 'PUT') {
        return Promise.resolve(json(withSkills));
      }
      if (path === '/api/plugins') return Promise.resolve(json([withSkills]));
      return Promise.reject(new Error(`unexpected ${init.method ?? 'GET'} ${path}`));
    });
    renderCard(withSkills);

    fireEvent.click(screen.getByRole('button', { name: 'Skills (1 of 2 on)' }));
    expect(screen.getByRole('switch', { name: 'Disable a' })).toBeVisible();
    fireEvent.click(screen.getByRole('switch', { name: 'Enable b' }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/plugins/${plugin.id}/skills/b`,
        expect.anything(),
      ),
    );
    const init = callFor(`/api/plugins/${plugin.id}/skills/b`);
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body as string)).toEqual({ enabled: true });
    expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('csrf-token');
  });

  it('external plugin card shows the source and offers install', () => {
    const onInstallFromGit = vi.fn();
    const url = 'https://github.com/acme/r.git';
    renderCard(
      {
        ...plugin,
        status: 'external',
        enabled: false,
        mcpServers: [],
        settings: [],
        external: { kind: 'github', url },
      },
      { onInstallFromGit },
    );

    expect(screen.getByText(`Lives in another repository: ${url}`)).toBeVisible();
    expect(screen.queryByRole('switch')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Test' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Install from git' }));
    expect(onInstallFromGit).toHaveBeenCalledWith(url);
  });
});
