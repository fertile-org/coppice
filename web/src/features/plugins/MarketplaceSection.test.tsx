import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { MarketplaceEntry, MarketplaceSource } from '../../lib/schemas/marketplace';
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

const coppiceSource: MarketplaceSource = {
  id: '00000000-0000-4000-8000-000000000100',
  name: 'coppice',
  kind: 'builtin',
  gitUrl: null,
  gitRef: null,
  commitSha: null,
  refreshedAt: null,
  lastError: null,
  position: 0,
};

const communitySource: MarketplaceSource = {
  id: '00000000-0000-4000-8000-000000000101',
  name: 'acme-plugins',
  kind: 'git',
  gitUrl: 'https://github.com/acme/plugins.git',
  gitRef: null,
  commitSha: 'abc1234',
  refreshedAt: '2026-10-25T10:00:00Z',
  lastError: null,
  position: 1,
};

const githubEntry: MarketplaceEntry = {
  sourceId: coppiceSource.id,
  sourceName: 'coppice',
  name: 'github',
  description: 'Issues, pull requests, reviews and CI from GitHub',
  version: '1.0.0',
  kind: 'github',
  installable: true,
  category: 'code',
  installedPluginId: null,
  updateAvailable: false,
};

const reviewEntry: MarketplaceEntry = {
  sourceId: communitySource.id,
  sourceName: 'acme-plugins',
  name: 'review-kit',
  description: 'Checklists for code review',
  version: '0.3.0',
  kind: 'path',
  installable: true,
  category: 'quality',
  installedPluginId: null,
  updateAvailable: false,
};

const npmEntry: MarketplaceEntry = {
  sourceId: communitySource.id,
  sourceName: 'acme-plugins',
  name: 'npm-only',
  description: 'Published to npm',
  version: '2.0.0',
  kind: 'npm',
  installable: false,
  reason: 'npm sources are not supported',
  category: null,
  installedPluginId: null,
  updateAvailable: false,
};

const installedUpdatable: MarketplaceEntry = {
  sourceId: communitySource.id,
  sourceName: 'acme-plugins',
  name: 'lint-helper',
  description: 'Runs the team linters',
  version: '1.1.0',
  kind: 'path',
  installable: true,
  category: 'quality',
  installedPluginId: '00000000-0000-4000-8000-000000000050',
  updateAvailable: true,
};

const installedPlugin: Plugin = {
  id: '00000000-0000-4000-8000-000000000010',
  pluginDirId: defaultDir.id,
  relPath: 'review-kit',
  name: 'review-kit',
  version: '0.3.0',
  description: 'Checklists for code review',
  source: 'local',
  gitUrl: null,
  gitRef: null,
  gitCommit: null,
  status: 'ok',
  error: null,
  enabled: false,
  skills: [],
  mcpServers: [],
  settings: [],
  unsupported: [],
  marketplace: { name: 'acme-plugins' },
  external: null,
  gitRoot: null,
  agentAccess: 'explicit',
  agentIds: [],
};

let entries: MarketplaceEntry[];
let sources: MarketplaceSource[];
let plugins: Plugin[];
let jobResult: PluginInstall;

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

async function openMarketplace() {
  fireEvent.click(await screen.findByRole('tab', { name: 'Marketplace' }));
  return screen.findByTestId(`marketplace-entry-${githubEntry.sourceId}/github`);
}

function entryCard(entry: MarketplaceEntry): HTMLElement {
  return screen.getByTestId(`marketplace-entry-${entry.sourceId}/${entry.name}`);
}

describe('PluginsPage marketplace tab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    entries = [githubEntry, reviewEntry, npmEntry, installedUpdatable];
    sources = [coppiceSource, communitySource];
    plugins = [];
    jobResult = {
      id: '00000000-0000-4000-8000-000000000020',
      pluginDirId: defaultDir.id,
      kind: 'github',
      gitUrl: 'https://github.com/org/github.git',
      gitRef: null,
      pluginId: null,
      pluginIds: [],
      status: 'running',
      error: null,
    };
    mocks.apiFetch.mockImplementation((path: string, init: RequestInit = {}) => {
      const method = init.method ?? 'GET';
      if (path === '/api/plugin-dirs' && method === 'GET') {
        return Promise.resolve(json([defaultDir]));
      }
      if (path === '/api/plugins' && method === 'GET') {
        return Promise.resolve(json(plugins));
      }
      if (path === '/api/marketplace/entries' && method === 'GET') {
        return Promise.resolve(json(entries));
      }
      if (path === '/api/marketplace/sources' && method === 'GET') {
        return Promise.resolve(json(sources));
      }
      if (path === '/api/marketplace/install' && method === 'POST') {
        const body = JSON.parse(init.body as string) as { entry: string };
        if (body.entry === 'review-kit') {
          return Promise.resolve(json(installedPlugin, 201));
        }
        return Promise.resolve(json(jobResult, 202));
      }
      if (path.startsWith('/api/plugin-installs/')) {
        return Promise.resolve(json(jobResult));
      }
      return Promise.reject(new Error(`unexpected ${method} ${path}`));
    });
  });

  it('opens the Marketplace tab with entries and trust labels', async () => {
    renderPage();

    expect(await screen.findByRole('tab', { name: 'Installed' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await openMarketplace();

    expect(screen.getByRole('heading', { name: 'Coppice marketplace' })).toBeVisible();
    expect(within(entryCard(githubEntry)).getByText('Coppice')).toBeVisible();
    expect(
      within(entryCard(reviewEntry)).getByText('Community: not reviewed by Coppice'),
    ).toBeVisible();
    expect(screen.getByText(/Plugins you can install with one click/)).toBeVisible();
  });

  it('filters by search text and category', async () => {
    renderPage();
    await openMarketplace();

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'checklists' } });
    expect(screen.queryByTestId(`marketplace-entry-${githubEntry.sourceId}/github`)).toBeNull();
    expect(entryCard(reviewEntry)).toBeVisible();

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '' } });
    selectComboboxOption(screen.getByRole('combobox', { name: 'Category' }), 'code');
    expect(entryCard(githubEntry)).toBeVisible();
    expect(screen.queryByTestId(`marketplace-entry-${communitySource.id}/review-kit`)).toBeNull();
  });

  it('shows a GitHub brand icon and a generic icon for other entries', async () => {
    renderPage();
    await openMarketplace();

    expect(within(entryCard(githubEntry)).getByTestId('marketplace-icon-github')).toBeVisible();
    expect(within(entryCard(reviewEntry)).getByTestId('marketplace-icon-fallback')).toBeVisible();
  });

  it('reveals 24 entries at a time and resets the limit for every filter', async () => {
    const extra = Array.from({ length: 29 }, (_, index) => ({
      ...reviewEntry,
      name: `plugin-${String(index).padStart(2, '0')}`,
      description: `Development helper ${index}`,
      category: 'code',
    }));
    entries = [githubEntry, ...extra];
    renderPage();
    await openMarketplace();

    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(24);
    fireEvent.click(screen.getByRole('button', { name: 'Load more' }));
    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(30);

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'Development helper' } });
    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(24);
    fireEvent.click(screen.getByRole('button', { name: 'Load more' }));
    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(29);

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '' } });
    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(24);
    fireEvent.click(screen.getByRole('button', { name: 'Load more' }));
    selectComboboxOption(screen.getByRole('combobox', { name: 'Source' }), 'acme-plugins');
    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(24);

    fireEvent.click(screen.getByRole('button', { name: 'Load more' }));
    selectComboboxOption(screen.getByRole('combobox', { name: 'Category' }), 'code');
    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(24);

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'no such plugin' } });
    expect(screen.queryAllByTestId(/^marketplace-entry-/)).toHaveLength(0);
  });

  it('a path install posts the entry and lands on the installed plugins with a notice', async () => {
    plugins = [installedPlugin];
    renderPage();
    await openMarketplace();

    fireEvent.click(within(entryCard(reviewEntry)).getByRole('button', { name: 'Install' }));

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/marketplace/install',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ sourceId: communitySource.id, entry: 'review-kit' }),
        }),
      ),
    );
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Installed review-kit. Enable it and choose which agents can use it.',
    );
    expect(screen.getByRole('tab', { name: 'Installed' })).toHaveAttribute('aria-selected', 'true');
  });

  it('an object install polls the job and reports the failure', async () => {
    jobResult = { ...jobResult, status: 'failed', error: 'repository not found' };
    renderPage();
    await openMarketplace();

    fireEvent.click(within(entryCard(githubEntry)).getByRole('button', { name: 'Install' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Install failed: repository not found',
    );
    expect(screen.getByRole('tab', { name: 'Marketplace' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(within(entryCard(githubEntry)).getByRole('button', { name: 'Install' })).toBeEnabled();
  });

  it('an object install that succeeds switches to the installed plugins', async () => {
    jobResult = { ...jobResult, status: 'succeeded', pluginIds: [installedPlugin.id] };
    renderPage();
    await openMarketplace();

    fireEvent.click(within(entryCard(githubEntry)).getByRole('button', { name: 'Install' }));

    expect(await screen.findByRole('status')).toHaveTextContent('Installed github.');
  });

  it('a github install names the missing token and highlights the new card', async () => {
    const scrollSpy = vi.spyOn(Element.prototype, 'scrollIntoView');
    const githubPlugin: Plugin = {
      ...installedPlugin,
      id: '00000000-0000-4000-8000-000000000030',
      name: 'github',
      relPath: 'github',
      marketplace: { name: 'coppice' },
      settings: [{ key: 'GITHUB_PERSONAL_ACCESS_TOKEN', configured: false, source: 'missing' }],
    };
    plugins = [githubPlugin];
    jobResult = { ...jobResult, status: 'succeeded', pluginIds: [githubPlugin.id] };
    renderPage();
    await openMarketplace();

    fireEvent.click(within(entryCard(githubEntry)).getByRole('button', { name: 'Install' }));

    const status = await screen.findByRole('status');
    expect(status).toHaveTextContent('Installed github.');
    expect(status).toHaveTextContent(
      'Add GITHUB_PERSONAL_ACCESS_TOKEN in its Settings before agents can use it.',
    );
    const card = await screen.findByTestId(`plugin-card-${githubPlugin.id}`);
    expect(card).toHaveAttribute('data-highlighted', 'true');
    expect(scrollSpy).toHaveBeenCalled();
    scrollSpy.mockRestore();
  });

  it('shows why an entry cannot be installed and offers no install button', async () => {
    renderPage();
    await openMarketplace();

    const card = entryCard(npmEntry);
    expect(within(card).getByText('npm sources are not supported')).toBeVisible();
    expect(within(card).queryByRole('button', { name: 'Install' })).toBeNull();
  });

  it('offers Update when an installed catalog plugin has a newer version', async () => {
    renderPage();
    await openMarketplace();

    const card = entryCard(installedUpdatable);
    fireEvent.click(within(card).getByRole('button', { name: 'Update available' }));

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/marketplace/plugins/${installedUpdatable.installedPluginId}/update`,
        expect.objectContaining({ method: 'POST' }),
      ),
    );
  });

  it('requires a name and git URL before adding a marketplace', async () => {
    renderPage();
    await openMarketplace();

    fireEvent.click(screen.getByRole('button', { name: 'Add marketplace' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Name and git URL are required.',
    );
    expect(mocks.apiFetch).not.toHaveBeenCalledWith(
      '/api/marketplace/sources',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('cannot remove the Coppice source but can remove a community one', async () => {
    renderPage();
    await openMarketplace();

    expect(screen.queryByRole('button', { name: 'Remove coppice' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Remove acme-plugins' }));

    const dialog = await screen.findByRole('dialog', {
      name: 'Remove marketplace "acme-plugins"?',
    });
    expect(within(dialog).getByText(/lose their Update button/)).toBeVisible();
  });
});
