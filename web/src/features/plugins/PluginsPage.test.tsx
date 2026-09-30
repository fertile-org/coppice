import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Plugin, PluginDir, PluginInstall } from '../../lib/schemas/plugin';
import { PluginsPage } from './PluginsPage';

const mocks = vi.hoisted(() => ({
  apiFetch: vi.fn(),
}));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../auth/useSession', () => ({
  useSession: () => ({ user: { role: 'admin' }, loading: false }),
}));

const defaultDir: PluginDir = {
  id: '00000000-0000-4000-8000-000000000001',
  path: '/data/plugins',
  position: 0,
  isDefault: true,
};

const extraDir: PluginDir = {
  id: '00000000-0000-4000-8000-000000000002',
  path: '/srv/team-plugins',
  position: 1,
  isDefault: false,
};

const samplePlugin: Plugin = {
  id: '00000000-0000-4000-8000-000000000010',
  pluginDirId: defaultDir.id,
  relPath: 'sample-plugin',
  name: 'sample-plugin',
  version: '1.2.0',
  description: 'A sample plugin',
  source: 'git',
  gitUrl: 'https://github.com/org/sample-plugin.git',
  gitRef: null,
  gitCommit: 'abcdef1234567890',
  status: 'ok',
  error: null,
  enabled: false,
  skills: [
    {
      name: 'review',
      description: 'Review code',
      relPath: 'skills/review/SKILL.md',
      error: null,
    },
  ],
  mcpServers: [{ name: 'docs', kind: 'stdio' }],
  unsupported: ['commands', 'hooks'],
};

const shadowedPlugin: Plugin = {
  ...samplePlugin,
  id: '00000000-0000-4000-8000-000000000011',
  pluginDirId: extraDir.id,
  name: 'shadowed-plugin',
  source: 'local',
  gitUrl: null,
  gitCommit: null,
  status: 'shadowed',
  enabled: false,
  error: 'shadowed by /data/plugins/shadowed-plugin',
  unsupported: [],
};

let dirs: PluginDir[];
let plugins: Plugin[];
let installPoll: PluginInstall;

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <PluginsPage />
    </QueryClientProvider>,
  );
}

describe('PluginsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    dirs = [defaultDir, extraDir];
    plugins = [samplePlugin, shadowedPlugin];
    installPoll = {
      id: '00000000-0000-4000-8000-000000000020',
      pluginDirId: extraDir.id,
      kind: 'install',
      gitUrl: 'https://github.com/org/missing.git',
      gitRef: 'v1',
      pluginId: null,
      status: 'failed',
      error: 'repository not found',
    };
    mocks.apiFetch.mockImplementation((path: string, init: RequestInit = {}) => {
      const method = init.method ?? 'GET';
      if (path === '/api/plugin-dirs' && method === 'GET') {
        return Promise.resolve(json(dirs));
      }
      if (path === '/api/plugins' && method === 'GET') {
        return Promise.resolve(json(plugins));
      }
      if (path.startsWith('/api/plugins/') && method === 'PATCH') {
        const body = JSON.parse(init.body as string) as { enabled: boolean };
        const target = plugins.find((p) => path === `/api/plugins/${p.id}`)!;
        return Promise.resolve(json({ ...target, enabled: body.enabled }));
      }
      if (path === '/api/plugins/install' && method === 'POST') {
        return Promise.resolve(json({ ...installPoll, status: 'running', error: null }, 202));
      }
      if (path.startsWith('/api/plugin-installs/')) {
        return Promise.resolve(json(installPoll));
      }
      return Promise.reject(new Error(`unexpected ${method} ${path}`));
    });
  });

  it('renders plugin cards with status and unsupported parts', async () => {
    renderPage();

    const card = await screen.findByTestId(`plugin-card-${samplePlugin.id}`);
    expect(within(card).getByText('sample-plugin')).toBeVisible();
    expect(within(card).getByText('1.2.0')).toBeVisible();
    expect(within(card).getByText('Not supported yet: commands, hooks')).toBeVisible();
    expect(within(card).getByText('ok')).toBeVisible();
    expect(within(card).getByText('abcdef1')).toBeVisible();
  });

  it('enable toggle is disabled for shadowed plugins', async () => {
    renderPage();

    expect(
      await screen.findByRole('switch', { name: 'Enable shadowed-plugin' }),
    ).toBeDisabled();
    expect(screen.getByRole('switch', { name: 'Enable sample-plugin' })).toBeEnabled();
  });

  it('an enabled shadowed plugin can still be disabled', async () => {
    plugins = [samplePlugin, { ...shadowedPlugin, enabled: true }];
    renderPage();

    const toggle = await screen.findByRole('switch', { name: 'Disable shadowed-plugin' });
    expect(toggle).toBeEnabled();
    fireEvent.click(toggle);

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/plugins/${shadowedPlugin.id}`,
        expect.objectContaining({
          method: 'PATCH',
          body: JSON.stringify({ enabled: false }),
        }),
      ),
    );
  });

  it('toggling enable sends PATCH', async () => {
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Enable sample-plugin' }));

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/plugins/${samplePlugin.id}`,
        expect.objectContaining({
          method: 'PATCH',
          body: JSON.stringify({ enabled: true }),
        }),
      ),
    );
  });

  it('install posts git url and shows failure', async () => {
    renderPage();

    fireEvent.change(await screen.findByLabelText('Git URL'), {
      target: { value: 'https://github.com/org/missing.git' },
    });
    fireEvent.change(screen.getByLabelText(/^Ref/), { target: { value: 'v1' } });
    fireEvent.change(screen.getByLabelText('Target directory'), {
      target: { value: extraDir.id },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/plugins/install',
        expect.objectContaining({ method: 'POST' }),
      ),
    );
    const call = mocks.apiFetch.mock.calls.find(
      ([path]) => path === '/api/plugins/install',
    ) as [string, RequestInit];
    expect(JSON.parse(call[1].body as string)).toEqual({
      gitUrl: 'https://github.com/org/missing.git',
      ref: 'v1',
      pluginDirId: extraDir.id,
    });

    expect(await screen.findByText(/repository not found/)).toBeVisible();
  });

  it('default directory cannot be removed', async () => {
    renderPage();

    const defaultRow = await screen.findByTestId(`plugin-dir-${defaultDir.id}`);
    expect(within(defaultRow).queryByRole('button', { name: /Remove/ })).not.toBeInTheDocument();
    const extraRow = screen.getByTestId(`plugin-dir-${extraDir.id}`);
    expect(within(extraRow).getByRole('button', { name: /Remove/ })).toBeVisible();
  });
});
