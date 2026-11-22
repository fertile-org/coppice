import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Plugin, PluginDir, PluginInstall } from '../../lib/schemas/plugin';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import { selectComboboxOption } from '../../test/combobox';
import { PluginsPage } from './PluginsPage';

const mocks = vi.hoisted(() => ({
  apiFetch: vi.fn(),
  isDesktopShell: vi.fn(() => false),
}));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../../lib/desktop', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/desktop')>()),
  isDesktopShell: () => mocks.isDesktopShell(),
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
      enabled: true,
    },
  ],
  mcpServers: [{ name: 'docs', kind: 'stdio', health: 'stopped' }],
  settings: [],
  unsupported: ['commands', 'hooks'],
  marketplace: null,
  external: null,
  gitRoot: null,
  builderMade: false,
  publishTarget: null,
  agentAccess: 'explicit',
  agentIds: [],
};

const STDIO_WARNING =
  "Local MCP servers run on your computer with your computer's privileges. Enable anyway?";

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
      <MemoryRouter>
        <ConfirmProvider>
          <PluginsPage />
        </ConfirmProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

async function openCustom() {
  fireEvent.click(await screen.findByRole('tab', { name: 'Custom' }));
}

describe('PluginsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.isDesktopShell.mockReturnValue(false);
    localStorage.clear();
    dirs = [defaultDir, extraDir];
    plugins = [samplePlugin, shadowedPlugin];
    installPoll = {
      id: '00000000-0000-4000-8000-000000000020',
      pluginDirId: extraDir.id,
      kind: 'install',
      gitUrl: 'https://github.com/org/missing.git',
      gitRef: 'v1',
      pluginId: null,
      pluginIds: [],
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
      if (path === '/api/marketplace/sources' && method === 'GET') {
        return Promise.resolve(json([]));
      }
      if (path === '/api/agents' && method === 'GET') {
        return Promise.resolve(json({ items: [] }));
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

  it('shows Build a plugin only in the desktop shell', async () => {
    mocks.isDesktopShell.mockReturnValue(false);
    renderPage();
    await screen.findByRole('heading', { name: 'Plugins' });
    expect(screen.queryByRole('button', { name: 'Build a plugin' })).not.toBeInTheDocument();
    cleanup();
    mocks.isDesktopShell.mockReturnValue(true);
    renderPage();
    expect(await screen.findByRole('button', { name: 'Build a plugin' })).toBeInTheDocument();
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

  it.each([
    ['stdio cancel', samplePlugin, 'Cancel' as const, false],
    ['stdio confirm', samplePlugin, 'Confirm' as const, true],
    [
      'http only',
      {
        ...samplePlugin,
        mcpServers: [{ name: 'docs', kind: 'http' as const, health: 'stopped' as const }],
      },
      null,
      true,
    ],
  ])('enable confirmation: %s', async (_label, target, confirmAction, patches) => {
    plugins = [target, shadowedPlugin];
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Enable sample-plugin' }));
    if (confirmAction) {
      const dialog = await screen.findByRole('dialog', { name: STDIO_WARNING });
      fireEvent.click(within(dialog).getByRole('button', { name: confirmAction }));
    }

    if (patches) {
      await waitFor(() =>
        expect(mocks.apiFetch).toHaveBeenCalledWith(
          `/api/plugins/${samplePlugin.id}`,
          expect.objectContaining({
            method: 'PATCH',
            body: JSON.stringify({ enabled: true }),
          }),
        ),
      );
      if (!confirmAction) {
        expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
      }
    } else {
      expect(mocks.apiFetch).not.toHaveBeenCalledWith(
        `/api/plugins/${samplePlugin.id}`,
        expect.objectContaining({ method: 'PATCH' }),
      );
    }
  });

  it('install posts git url and shows failure', async () => {
    renderPage();
    await openCustom();

    fireEvent.change(await screen.findByLabelText('Git URL'), {
      target: { value: 'https://github.com/org/missing.git' },
    });
    fireEvent.change(screen.getByLabelText(/^Ref/), { target: { value: 'v1' } });
    selectComboboxOption(screen.getByLabelText('Target directory'), extraDir.path);
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

  it('Install from git on an external card opens and focuses the Custom form', async () => {
    const url = 'https://github.com/acme/r.git';
    plugins = [
      {
        ...samplePlugin,
        id: '00000000-0000-4000-8000-000000000012',
        name: 'remote-plugin',
        source: 'local',
        status: 'external',
        skills: [],
        mcpServers: [],
        unsupported: [],
        external: { kind: 'github', url },
      },
    ];
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: /remote-plugin/ }));
    fireEvent.click(await screen.findByRole('button', { name: 'Install from git' }));

    expect(screen.getByRole('tab', { name: 'Custom' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    const input = screen.getByLabelText('Git URL');
    expect(input).toHaveValue(url);
    await waitFor(() => expect(input).toHaveFocus());
  });

  it('install success lists every plugin from the clone', async () => {
    const alpha: Plugin = {
      ...samplePlugin,
      id: '00000000-0000-4000-8000-0000000000a1',
      name: 'alpha',
      gitRoot: '/data/plugins/marketplace-repo',
    };
    const beta: Plugin = {
      ...alpha,
      id: '00000000-0000-4000-8000-0000000000a2',
      name: 'beta-skills',
    };
    installPoll = {
      ...installPoll,
      gitUrl: 'https://github.com/acme/marketplace-repo.git',
      gitRef: null,
      pluginId: alpha.id,
      pluginIds: [alpha.id, beta.id],
      status: 'succeeded',
      error: null,
    };
    renderPage();
    await openCustom();

    fireEvent.change(await screen.findByLabelText('Git URL'), {
      target: { value: 'https://github.com/acme/marketplace-repo.git' },
    });
    plugins = [alpha, beta];
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));

    await waitFor(() => expect(screen.getByText(/alpha, beta-skills/)).toBeInTheDocument());
    fireEvent.click(await screen.findByRole('tab', { name: 'Installed' }));
    await waitFor(() => {
      expect(screen.getByTestId(`plugin-row-${alpha.id}`)).toBeVisible();
      expect(screen.getByTestId(`plugin-row-${beta.id}`)).toBeVisible();
    });
  });

  it('default directory cannot be removed', async () => {
    renderPage();
    await openCustom();

    const defaultRow = await screen.findByTestId(`plugin-dir-${defaultDir.id}`);
    expect(within(defaultRow).queryByRole('button', { name: /Remove/ })).not.toBeInTheDocument();
    const extraRow = screen.getByTestId(`plugin-dir-${extraDir.id}`);
    expect(within(extraRow).getByRole('button', { name: /Remove/ })).toBeVisible();
  });

  it('canceling remove plugin directory leaves the directory in place', async () => {
    renderPage();
    await openCustom();

    const extraRow = await screen.findByTestId(`plugin-dir-${extraDir.id}`);
    fireEvent.click(within(extraRow).getByRole('button', { name: /Remove/ }));
    const dialog = await screen.findByRole('dialog', {
      name: `Remove plugin directory "${extraDir.path}"?`,
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));

    expect(mocks.apiFetch).not.toHaveBeenCalledWith(
      `/api/plugin-dirs/${extraDir.id}`,
      expect.objectContaining({ method: 'DELETE' }),
    );
  });

  it('clicking the switch does not open the drawer and still asks to enable', async () => {
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Enable sample-plugin' }));
    expect(await screen.findByRole('dialog', { name: STDIO_WARNING })).toBeVisible();
    expect(screen.queryByRole('dialog', { name: 'Plugin details' })).not.toBeInTheDocument();
  });

  it('search and filters narrow the list and counts match the plugins shown', async () => {
    renderPage();

    expect(await screen.findByRole('button', { name: 'All 2' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Enabled 0' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Disabled 2' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Needs attention 0' })).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Enabled 0' }));
    expect(screen.queryByTestId(`plugin-row-${samplePlugin.id}`)).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Disabled 2' }));
    expect(screen.getByTestId(`plugin-row-${samplePlugin.id}`)).toBeVisible();
    expect(screen.getByTestId(`plugin-row-${shadowedPlugin.id}`)).toBeVisible();

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search installed plugins' }), {
      target: { value: 'shadowed' },
    });
    expect(screen.getByRole('button', { name: 'All 1' })).toBeVisible();
    expect(screen.getByTestId(`plugin-row-${shadowedPlugin.id}`)).toBeVisible();
    expect(screen.queryByTestId(`plugin-row-${samplePlugin.id}`)).not.toBeInTheDocument();
  });

  it('needs attention lists invalid, missing setting, re-entry and unhealthy MCP, not shadowed', async () => {
    const invalid: Plugin = {
      ...samplePlugin,
      id: '00000000-0000-4000-8000-000000000021',
      name: 'broken',
      status: 'invalid',
      error: 'plugin name is reserved',
      unsupported: [],
      mcpServers: [],
    };
    const missingSetting: Plugin = {
      ...samplePlugin,
      id: '00000000-0000-4000-8000-000000000022',
      name: 'github',
      settings: [{ key: 'GITHUB_TOKEN', configured: false, source: 'missing' }],
      unsupported: [],
      mcpServers: [],
    };
    const reentry: Plugin = {
      ...samplePlugin,
      id: '00000000-0000-4000-8000-000000000023',
      name: 'secrets',
      settings: [
        { key: 'API_TOKEN', configured: false, source: 'missing', needsReentry: true },
      ],
      unsupported: [],
      mcpServers: [],
    };
    const unhealthy: Plugin = {
      ...samplePlugin,
      id: '00000000-0000-4000-8000-000000000024',
      name: 'mcp-fake',
      mcpServers: [{ name: 'fake', kind: 'stdio', health: 'unhealthy' }],
      unsupported: [],
    };
    plugins = [invalid, missingSetting, reentry, unhealthy, shadowedPlugin, samplePlugin];
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: 'Needs attention 4' }));
    const rows = screen.getAllByTestId(/^plugin-row-/);
    expect(rows.map((row) => row.getAttribute('data-testid'))).toEqual([
      `plugin-row-${invalid.id}`,
      `plugin-row-${missingSetting.id}`,
      `plugin-row-${unhealthy.id}`,
      `plugin-row-${reentry.id}`,
    ]);
    expect(screen.queryByTestId(`plugin-row-${shadowedPlugin.id}`)).not.toBeInTheDocument();
  });

  it('closing the drawer during Update keeps polling and shows a failure on the row', async () => {
    const job: PluginInstall = {
      id: '00000000-0000-4000-8000-000000000099',
      pluginDirId: defaultDir.id,
      kind: 'update',
      gitUrl: samplePlugin.gitUrl!,
      gitRef: null,
      pluginId: samplePlugin.id,
      pluginIds: [samplePlugin.id],
      status: 'failed',
      error: 'fetch failed',
    };
    mocks.apiFetch.mockImplementation((path: string, init: RequestInit = {}) => {
      const method = init.method ?? 'GET';
      if (path === '/api/plugin-dirs' && method === 'GET') return Promise.resolve(json(dirs));
      if (path === '/api/plugins' && method === 'GET') return Promise.resolve(json(plugins));
      if (path === '/api/agents' && method === 'GET') return Promise.resolve(json({ items: [] }));
      if (path === `/api/plugins/${samplePlugin.id}/update` && method === 'POST') {
        return Promise.resolve(json({ ...job, status: 'running', error: null }, 202));
      }
      if (path === `/api/plugin-installs/${job.id}`) return Promise.resolve(json(job));
      return Promise.reject(new Error(`unexpected ${method} ${path}`));
    });
    renderPage();

    fireEvent.click(
      within(await screen.findByTestId(`plugin-row-${samplePlugin.id}`)).getByRole('button', {
        name: /sample-plugin/,
      }),
    );
    fireEvent.click(
      within(await screen.findByRole('dialog', { name: 'Plugin details' })).getByRole(
        'button',
        { name: 'Update' },
      ),
    );
    fireEvent.click(within(screen.getByRole('dialog', { name: 'Plugin details' })).getByRole('button', { name: 'Close' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('Update failed: fetch failed');
    expect(screen.queryByRole('dialog', { name: 'Plugin details' })).not.toBeInTheDocument();
  });

  it('confirming remove plugin directory deletes it', async () => {
    mocks.apiFetch.mockImplementation((path: string, init: RequestInit = {}) => {
      const method = init.method ?? 'GET';
      if (path === '/api/plugin-dirs' && method === 'GET') {
        return Promise.resolve(json(dirs));
      }
      if (path === '/api/plugins' && method === 'GET') {
        return Promise.resolve(json(plugins));
      }
      if (path === '/api/agents' && method === 'GET') {
        return Promise.resolve(json({ items: [] }));
      }
      if (path === `/api/plugin-dirs/${extraDir.id}` && method === 'DELETE') {
        return Promise.resolve(json(null));
      }
      return Promise.reject(new Error(`unexpected ${method} ${path}`));
    });
    renderPage();
    await openCustom();

    const extraRow = await screen.findByTestId(`plugin-dir-${extraDir.id}`);
    fireEvent.click(within(extraRow).getByRole('button', { name: /Remove/ }));
    const dialog = await screen.findByRole('dialog', {
      name: `Remove plugin directory "${extraDir.path}"?`,
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/plugin-dirs/${extraDir.id}`,
        expect.objectContaining({ method: 'DELETE' }),
      ),
    );
  });
});
