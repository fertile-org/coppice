import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { MemoryRouter } from 'react-router-dom';
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
    { key: 'API_TOKEN', configured: true, source: 'setting' },
    { key: 'ROOT', configured: false, source: 'missing' },
  ],
  unsupported: [],
  marketplace: null,
  external: null,
  gitRoot: null,
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

function renderCard(
  target: Plugin = plugin,
  props: Partial<Omit<ComponentProps<typeof PluginCard>, 'plugin'>> = {},
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <PluginCard plugin={target} {...props} />
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
      if (path === `/api/plugins/${plugin.id}` && method === 'PATCH') {
        return Promise.resolve(json({ ...plugin, enabled: true }));
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

  it('shows where unset settings come from', () => {
    renderCard({
      ...plugin,
      settings: [
        { key: 'API_TOKEN', configured: true, source: 'setting' },
        { key: 'GITHUB_TOKEN', configured: false, source: 'env' },
        { key: 'PORT', configured: false, source: 'default' },
        { key: 'ROOT', configured: false, source: 'missing' },
      ],
    });

    const row = (key: string) => within(screen.getByTestId(`plugin-setting-${key}`));
    expect(row('API_TOKEN').getByText('Configured')).toBeVisible();
    expect(row('API_TOKEN').queryByText('From server env')).not.toBeInTheDocument();
    expect(row('GITHUB_TOKEN').getByText('From server env')).toBeVisible();
    expect(row('GITHUB_TOKEN').queryByText('Configured')).not.toBeInTheDocument();
    expect(row('PORT').getByText('Default')).toBeVisible();
    expect(row('ROOT').getByText('Missing')).toBeVisible();
  });

  it('unknown setting source falls back to missing', () => {
    const parsed = pluginSchema.parse({
      ...plugin,
      settings: [{ key: 'X', configured: false, source: 'vault' }],
    });
    expect(parsed.settings[0].source).toBe('missing');
  });

  it('enabling a plugin with env-sourced settings names those keys', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
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

    await waitFor(() => expect(callFor(`/api/plugins/${plugin.id}`).method).toBe('PATCH'));
    expect(confirm).toHaveBeenCalledWith(
      "Setting GITHUB_TOKEN is not set here, so the Coppice server's environment value for it will be sent to the plugin. Enable anyway?",
    );
    confirm.mockRestore();
  });

  it('stdio and env risks share one confirmation; cancel sends nothing', () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    renderCard({
      ...plugin,
      enabled: false,
      settings: [
        { key: 'A', configured: false, source: 'env' },
        { key: 'B', configured: false, source: 'env' },
      ],
    });

    fireEvent.click(screen.getByRole('switch', { name: 'Enable mcp-fake' }));

    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm).toHaveBeenCalledWith(
      "This plugin starts local MCP servers that run with the Coppice server's privileges until sandboxing lands (M11). Settings A, B are not set here, so the Coppice server's environment values for them will be sent to the plugin. Enable anyway?",
    );
    expect(fetchMock).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it('testing a disabled http plugin with env-sourced settings asks first', () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    renderCard({
      ...plugin,
      enabled: false,
      mcpServers: [{ name: 'remote', kind: 'http', health: 'stopped' }],
      settings: [{ key: 'GITHUB_TOKEN', configured: false, source: 'env' }],
    });

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));

    expect(confirm).toHaveBeenCalledWith(expect.stringContaining('GITHUB_TOKEN'));
    expect(fetchMock).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it('testing a disabled http plugin without env-sourced settings does not confirm', async () => {
    const confirm = vi.spyOn(window, 'confirm');
    renderCard({
      ...plugin,
      enabled: false,
      mcpServers: [{ name: 'remote', kind: 'http', health: 'stopped' }],
    });

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

  it('status pill and server health explain themselves', () => {
    renderCard({ ...plugin, status: 'shadowed' });

    expect(screen.getByText('shadowed')).toHaveAttribute(
      'title',
      expect.stringMatching(/earlier plugin directory/),
    );
    expect(screen.getByText('unhealthy')).toHaveAttribute(
      'title',
      expect.stringMatching(/Test/),
    );
  });

  it('enabled plugin points to the Agents page', () => {
    renderCard();

    expect(screen.getByRole('link', { name: /Give it to an agent/ })).toHaveAttribute(
      'href',
      `/agents?plugin=${plugin.id}`,
    );
    expect(screen.getByText(/mcp-fake pre-selected/)).toBeVisible();
  });

  it('disabled plugin does not show the attach link', () => {
    renderCard({ ...plugin, enabled: false });

    expect(screen.queryByRole('link', { name: /Give it to an agent/ })).not.toBeInTheDocument();
  });

  it('explains how agents see tools and skills', () => {
    renderCard({
      ...plugin,
      skills: [
        {
          name: 'review',
          description: 'Review code',
          relPath: 'skills/review/SKILL.md',
          error: null,
          enabled: true,
        },
      ],
    });

    expect(screen.getByText(/mcp-fake__<tool>/)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: /Skills/ }));
    expect(screen.getByText(/loads one when it needs it/)).toBeVisible();
  });

  it('settings default to empty when absent', () => {
    const raw: Partial<Plugin> = { ...plugin };
    delete raw.settings;
    expect(pluginSchema.parse(raw).settings).toEqual([]);
  });

  it('new plugin fields default when absent', () => {
    const raw: Record<string, unknown> = {
      ...plugin,
      skills: [{ name: 'a', description: '', relPath: 'skills/a/SKILL.md', error: null }],
    };
    delete raw.marketplace;
    delete raw.external;
    delete raw.gitRoot;
    const parsed = pluginSchema.parse(raw);
    expect(parsed.marketplace).toBeNull();
    expect(parsed.external).toBeNull();
    expect(parsed.gitRoot).toBeNull();
    expect(parsed.skills[0].enabled).toBe(true);
    expect(pluginSchema.parse({ ...plugin, status: 'external' }).status).toBe('external');
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

  it('skills header shows a plain count when all are on', () => {
    renderCard({
      ...plugin,
      skills: [
        { name: 'a', description: '', relPath: 'skills/a/SKILL.md', error: null, enabled: true },
      ],
    });

    expect(screen.getByRole('button', { name: 'Skills (1)' })).toBeVisible();
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
    expect(screen.getByText('external')).toHaveAttribute(
      'title',
      'Lives in another repository; install it separately.',
    );
    expect(screen.queryByRole('switch')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Test' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Install from git' }));
    expect(onInstallFromGit).toHaveBeenCalledWith(url);
  });

  it('external plugin without a url names its source kind', () => {
    renderCard({
      ...plugin,
      status: 'external',
      enabled: false,
      mcpServers: [],
      settings: [],
      external: { kind: 'npm', url: null },
    });

    expect(screen.getByText('Source: npm')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Install from git' })).not.toBeInTheDocument();
  });

  it('marketplace plugins show where they came from', () => {
    renderCard({ ...plugin, marketplace: { name: 'acme-market' } });

    expect(screen.getByText('From marketplace acme-market')).toBeVisible();
  });

  it('update tooltip names sibling count', () => {
    renderCard(
      { ...plugin, source: 'git', gitUrl: 'https://x/r.git', gitRoot: '/data/plugins/r' },
      { siblingCount: 3 },
    );

    expect(screen.getByRole('button', { name: 'Update' })).toHaveAttribute(
      'title',
      'Updates all 3 plugins from this repository',
    );
  });
});
