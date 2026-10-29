import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Plugin, PluginDir, PluginInstall } from '../../lib/schemas/plugin';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
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
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Enable sample-plugin' }));
    const dialog = await screen.findByRole('dialog', { name: STDIO_WARNING });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(mocks.apiFetch).not.toHaveBeenCalledWith(
      `/api/plugins/${samplePlugin.id}`,
      expect.objectContaining({ method: 'PATCH' }),
    );
  });

  it('enabling plugin with only http servers does not confirm', async () => {
    plugins = [
      { ...samplePlugin, mcpServers: [{ name: 'docs', kind: 'http', health: 'stopped' }] },
    ];
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Enable sample-plugin' }));

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/plugins/${samplePlugin.id}`,
        expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ enabled: true }) }),
      ),
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('toggling enable sends PATCH', async () => {
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Enable sample-plugin' }));
    const dialog = await screen.findByRole('dialog', { name: STDIO_WARNING });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));

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

  it('separates installed and custom controls and links the plugin guide', async () => {
    renderPage();

    expect(await screen.findByRole('tab', { name: 'Installed' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByRole('tab', { name: 'Marketplace' })).toBeVisible();
    expect(screen.getByRole('tab', { name: 'Custom' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'How plugins work' })).toBeNull();
    expect(screen.queryByRole('heading', { name: 'Add plugins' })).toBeNull();
    expect(screen.getByRole('link', { name: 'Plugin guide' })).toHaveAttribute(
      'href',
      'https://getcoppice.app/docs/plugins',
    );

    await openCustom();
    expect(screen.getByRole('heading', { name: 'Custom plugins' })).toBeVisible();
    expect(screen.getByRole('link', { name: 'View required plugin format' })).toHaveAttribute(
      'href',
      'https://getcoppice.app/docs/plugins#create-a-plugin',
    );
    expect(screen.getByLabelText('Git URL')).toBeVisible();
    expect(screen.getByLabelText('Directory path')).toBeVisible();
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

    expect(
      await screen.findByText('Installed marketplace-repo: 2 plugins (alpha, beta-skills)'),
    ).toBeVisible();
    fireEvent.click(screen.getByRole('tab', { name: 'Installed' }));
    const update = within(await screen.findByTestId(`plugin-card-${alpha.id}`)).getByRole(
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
    await openCustom();

    fireEvent.change(await screen.findByLabelText('Git URL'), {
      target: { value: 'https://github.com/org/sample-plugin.git' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));

    expect(
      await screen.findByText('Installed https://github.com/org/sample-plugin.git.'),
    ).toBeVisible();
  });

  it('empty state links GitHub examples and describes a desktop git install', async () => {
    plugins = [];
    renderPage();

    const examples = await screen.findByRole('link', { name: 'examples/plugins' });
    expect(examples).toHaveAttribute(
      'href',
      'https://github.com/fertile-org/coppice/tree/main/examples/plugins',
    );
    expect(examples).toHaveAttribute('target', '_blank');

    await openCustom();
    expect(screen.getByText(/with your computer's git credentials/)).toBeVisible();
    expect(screen.getByLabelText('Directory path')).toHaveAttribute(
      'placeholder',
      '~/coppice-plugins',
    );
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
