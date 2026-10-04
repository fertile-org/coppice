import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Plugin, PluginDir, PluginInstall } from '../../lib/schemas/plugin';
import { selectComboboxOption } from '../../test/combobox';
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
      enabled: true,
    },
  ],
  mcpServers: [{ name: 'docs', kind: 'stdio', health: 'stopped' }],
  settings: [],
  unsupported: ['commands', 'hooks'],
  marketplace: null,
  external: null,
  gitRoot: null,
};

const STDIO_WARNING =
  "This plugin starts local MCP servers that run with the Coppice server's privileges until sandboxing lands (M12). Enable anyway?";

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
        <PluginsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('PluginsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
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

  it('enabling plugin with stdio server asks for confirmation', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Enable sample-plugin' }));

    expect(confirm).toHaveBeenCalledWith(STDIO_WARNING);
    expect(mocks.apiFetch).not.toHaveBeenCalledWith(
      `/api/plugins/${samplePlugin.id}`,
      expect.objectContaining({ method: 'PATCH' }),
    );
    confirm.mockRestore();
  });

  it('enabling plugin with only http servers does not confirm', async () => {
    plugins = [
      { ...samplePlugin, mcpServers: [{ name: 'docs', kind: 'http', health: 'stopped' }] },
    ];
    const confirm = vi.spyOn(window, 'confirm');
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Enable sample-plugin' }));

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/plugins/${samplePlugin.id}`,
        expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ enabled: true }) }),
      ),
    );
    expect(confirm).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it('toggling enable sends PATCH', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
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
    expect(confirm).toHaveBeenCalledWith(STDIO_WARNING);
    confirm.mockRestore();
  });

  it('install posts git url and shows failure', async () => {
    renderPage();

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

  it('guide is open when no plugins are installed and explains the flow', async () => {
    plugins = [];
    renderPage();

    const toggle = await screen.findByRole('button', { name: 'How plugins work' });
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    const guide = screen.getByTestId('plugins-guide');
    expect(within(guide).getByText('Skills')).toBeVisible();
    expect(within(guide).getByText('Tools')).toBeVisible();
    expect(within(guide).getByText('Settings')).toBeVisible();
    expect(within(guide).getByRole('link', { name: 'Agents' })).toHaveAttribute(
      'href',
      '/agents',
    );
    expect(screen.getByText(/examples\/plugins/, { selector: 'p *' })).toBeVisible();
  });

  it('guide starts collapsed when plugins exist and remembers the choice', async () => {
    const view = renderPage();

    const toggle = await screen.findByRole('button', { name: 'How plugins work' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByTestId('plugins-guide')).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(screen.getByTestId('plugins-guide')).toBeVisible();

    view.unmount();
    renderPage();
    expect(await screen.findByTestId('plugins-guide')).toBeVisible();
  });

  it('installed plugins come before the add sections', async () => {
    renderPage();

    const headings = (await screen.findAllByRole('heading', { level: 2 })).map(
      (h) => h.textContent,
    );
    expect(headings).toEqual(['Installed plugins', 'Add plugins']);
  });

  it('Install from git on an external card fills the install form', async () => {
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

    fireEvent.click(await screen.findByRole('button', { name: 'Install from git' }));

    expect(screen.getByLabelText('Git URL')).toHaveValue(url);
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

    fireEvent.change(await screen.findByLabelText('Git URL'), {
      target: { value: 'https://github.com/acme/marketplace-repo.git' },
    });
    plugins = [alpha, beta];
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));

    expect(
      await screen.findByText('Installed marketplace-repo: 2 plugins (alpha, beta-skills)'),
    ).toBeVisible();
    const update = within(screen.getByTestId(`plugin-card-${alpha.id}`)).getByRole(
      'button',
      { name: 'Update' },
    );
    expect(update).toHaveAttribute('title', 'Updates all 2 plugins from this repository');
  });

  it('single-plugin install keeps the plain success message', async () => {
    installPoll = {
      ...installPoll,
      gitUrl: 'https://github.com/org/sample-plugin.git',
      pluginId: samplePlugin.id,
      pluginIds: [samplePlugin.id],
      status: 'succeeded',
      error: null,
    };
    renderPage();

    fireEvent.change(await screen.findByLabelText('Git URL'), {
      target: { value: 'https://github.com/org/sample-plugin.git' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));

    expect(
      await screen.findByText('Installed https://github.com/org/sample-plugin.git.'),
    ).toBeVisible();
  });

  it('default directory cannot be removed', async () => {
    renderPage();

    const defaultRow = await screen.findByTestId(`plugin-dir-${defaultDir.id}`);
    expect(within(defaultRow).queryByRole('button', { name: /Remove/ })).not.toBeInTheDocument();
    const extraRow = screen.getByTestId(`plugin-dir-${extraDir.id}`);
    expect(within(extraRow).getByRole('button', { name: /Remove/ })).toBeVisible();
  });
});
