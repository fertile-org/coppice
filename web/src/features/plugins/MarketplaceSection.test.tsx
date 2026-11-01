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
import { MARKETPLACE_ENTRIES_QUERY_KEY } from './useMarketplace';

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
  refreshedAt: '2026-10-31T10:00:00Z',
  lastError: null,
  position: 1,
};

const unfetchedSource: MarketplaceSource = {
  ...communitySource,
  id: '00000000-0000-4000-8000-000000000102',
  name: 'anthropic-test',
  refreshedAt: null,
  commitSha: null,
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
let addedFetchError: string | null;
let entryErrors: Record<string, string>;
let entryDelay: Promise<void> | null;
let sourcesDelay: Promise<void> | null;
let refreshDelay: Promise<void> | null;
let postRefreshEntryDelay: Promise<void> | null;
const refreshedIds = new Set<string>();

function deferred() {
  let resolve: () => void = () => undefined;
  const promise = new Promise<void>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

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
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConfirmProvider>
          <PluginsPage />
        </ConfirmProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return { client };
}

async function openMarketplace() {
  fireEvent.click(await screen.findByRole('tab', { name: 'Marketplace' }));
  return screen.findByTestId(`marketplace-entry-${githubEntry.sourceId}/github`);
}

async function openSources() {
  fireEvent.click(screen.getByRole('button', { name: 'Manage sources' }));
  return screen.findByRole('dialog', { name: 'Marketplace sources' });
}

function entryCard(entry: MarketplaceEntry): HTMLElement {
  return screen.getByTestId(`marketplace-entry-${entry.sourceId}/${entry.name}`);
}

describe('PluginsPage marketplace tab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    entries = [githubEntry, reviewEntry, npmEntry, installedUpdatable];
    sources = [
      { ...coppiceSource, refreshedAt: '2026-10-31T10:00:00Z' },
      communitySource,
    ];
    plugins = [];
    addedFetchError = null;
    entryErrors = {};
    entryDelay = null;
    sourcesDelay = null;
    refreshDelay = null;
    postRefreshEntryDelay = null;
    refreshedIds.clear();
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
      if (path.startsWith('/api/marketplace/entries') && method === 'GET') {
        const url = new URL(path, 'http://localhost');
        const source = url.searchParams.get('source');
        if (source && entryErrors[source]) {
          return Promise.reject(new Error(entryErrors[source]));
        }
        const listed = source ? entries.filter((entry) => entry.sourceId === source) : entries;
        if (entryDelay) {
          return entryDelay.then(() => json(listed));
        }
        if (source && postRefreshEntryDelay) {
          if (!refreshedIds.has(source)) {
            return Promise.resolve(json([]));
          }
          return postRefreshEntryDelay.then(() => json(listed));
        }
        return Promise.resolve(json(listed));
      }
      if (path === '/api/marketplace/sources' && method === 'GET') {
        if (sourcesDelay) {
          return sourcesDelay.then(() => json(sources));
        }
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
      if (path === '/api/marketplace/sources' && method === 'POST') {
        const body = JSON.parse(init.body as string) as { name: string; gitUrl: string };
        const added: MarketplaceSource = {
          ...unfetchedSource,
          id: '00000000-0000-4000-8000-000000000103',
          name: body.name,
          gitUrl: body.gitUrl,
          lastError: addedFetchError,
        };
        sources = [...sources, added];
        return Promise.resolve(json(added, 201));
      }
      if (path.match(/^\/api\/marketplace\/sources\/.+\/refresh$/) && method === 'POST') {
        const finish = async () => {
          if (refreshDelay) await refreshDelay;
          const id = path.split('/')[4];
          const source = sources.find((item) => item.id === id);
          if (source?.lastError) return Promise.reject(new Error(source.lastError));
          refreshedIds.add(id);
          const refreshed = { ...source!, refreshedAt: '2026-10-28T10:00:00Z' };
          sources = sources.map((item) => (item.id === id ? refreshed : item));
          return json(refreshed);
        };
        return finish();
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

    expect(screen.getByRole('heading', { name: 'Plugin marketplace' })).toBeVisible();
    expect(within(entryCard(githubEntry)).getByText('Coppice')).toBeVisible();
    expect(
      within(entryCard(reviewEntry)).getByText('Community: not reviewed by Coppice'),
    ).toBeVisible();
    expect(screen.getByText(/Install with one click/)).toBeVisible();
    expect(screen.queryByTestId(`marketplace-source-${communitySource.id}`)).toBeNull();
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
    fireEvent.click(screen.getByTestId('marketplace-source-chip-acme-plugins'));
    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(24);

    fireEvent.click(screen.getByRole('button', { name: 'Load more' }));
    selectComboboxOption(screen.getByRole('combobox', { name: 'Category' }), 'code');
    expect(screen.getAllByTestId(/^marketplace-entry-/)).toHaveLength(24);

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'no such plugin' } });
    expect(screen.queryAllByTestId(/^marketplace-entry-/)).toHaveLength(0);
  });

  it('fetches each never-loaded source once when Marketplace opens', async () => {
    sources = [
      { ...coppiceSource, refreshedAt: '2026-10-31T10:00:00Z' },
      unfetchedSource,
    ];
    renderPage();
    await openMarketplace();

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/marketplace/sources/${unfetchedSource.id}/refresh`,
        expect.objectContaining({ method: 'POST' }),
      ),
    );
    await waitFor(() =>
      expect(screen.queryByTestId('marketplace-skeleton')).toBeNull(),
    );
    const calls = mocks.apiFetch.mock.calls.filter(
      ([path, init]) =>
        path === `/api/marketplace/sources/${unfetchedSource.id}/refresh` &&
        (init?.method ?? 'GET') === 'POST',
    );
    expect(calls).toHaveLength(1);
    expect(entryCard(githubEntry)).toBeVisible();
  });

  it('keeps healthy entries visible when a first fetch fails', async () => {
    sources = [
      { ...coppiceSource, refreshedAt: '2026-10-31T10:00:00Z' },
      { ...unfetchedSource, lastError: 'network offline' },
    ];
    renderPage();
    await openMarketplace();

    expect(entryCard(githubEntry)).toBeVisible();
    expect(screen.getByText(/anthropic-test couldn’t load/)).toBeVisible();
    expect(screen.getByTestId('marketplace-source-chip-anthropic-test')).toBeVisible();
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
    const dialog = await openSources();

    fireEvent.click(within(dialog).getByRole('button', { name: 'Add marketplace' }));

    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
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
    const sourcesDialog = await openSources();

    expect(within(sourcesDialog).queryByRole('button', { name: 'Remove coppice' })).toBeNull();
    fireEvent.click(within(sourcesDialog).getByRole('button', { name: 'Remove acme-plugins' }));

    const dialog = await screen.findByRole('dialog', {
      name: 'Remove marketplace "acme-plugins"?',
    });
    expect(within(dialog).getByText(/lose their Update button/)).toBeVisible();
  });

  function addSourceFromDialog(dialog: HTMLElement) {
    fireEvent.change(within(dialog).getByLabelText('Name'), { target: { value: 'team-tools' } });
    fireEvent.change(within(dialog).getByLabelText('Git URL'), {
      target: { value: 'https://github.com/team/tools.git' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Add marketplace' }));
  }

  function refreshCalls(id: string) {
    return mocks.apiFetch.mock.calls.filter(
      ([path, init]) =>
        path === `/api/marketplace/sources/${id}/refresh` && (init?.method ?? 'GET') === 'POST',
    );
  }

  it('adding a source registers it and fetches its catalog once', async () => {
    renderPage();
    await openMarketplace();
    const dialog = await openSources();

    addSourceFromDialog(dialog);

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/marketplace/sources',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ name: 'team-tools', gitUrl: 'https://github.com/team/tools.git' }),
        }),
      ),
    );
    const source = await within(dialog).findByTestId(
      'marketplace-source-00000000-0000-4000-8000-000000000103',
    );
    await waitFor(() => expect(within(source).getByText(/^Refreshed /)).toBeVisible());
    expect(refreshCalls('00000000-0000-4000-8000-000000000103')).toHaveLength(1);
    expect(within(dialog).queryByRole('alert')).toBeNull();
  });

  it('explains when a source is added but its catalog cannot be fetched', async () => {
    addedFetchError = 'repository not found';
    renderPage();
    await openMarketplace();
    const dialog = await openSources();

    addSourceFromDialog(dialog);

    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
      'Added team-tools, but its catalog could not be fetched.',
    );
    const source = await within(dialog).findByTestId(
      'marketplace-source-00000000-0000-4000-8000-000000000103',
    );
    expect(within(source).getByRole('button', { name: 'Retry team-tools' })).toBeVisible();
    expect(refreshCalls('00000000-0000-4000-8000-000000000103')).toHaveLength(1);
  });

  it('refreshes a source from the dialog and opens it from the failure warning', async () => {
    sources = [
      { ...coppiceSource, refreshedAt: '2026-10-31T10:00:00Z' },
      communitySource,
      { ...unfetchedSource, refreshedAt: '2026-10-25T10:00:00Z', lastError: 'network offline' },
    ];
    renderPage();
    await openMarketplace();

    fireEvent.click(screen.getByRole('button', { name: 'Manage sources' }));
    const dialog = await screen.findByRole('dialog', { name: 'Marketplace sources' });
    expect(within(dialog).getByText('Last refresh failed: network offline')).toBeVisible();
    expect(within(dialog).getByRole('button', { name: 'Retry anthropic-test' })).toBeVisible();

    fireEvent.click(within(dialog).getByRole('button', { name: 'Refresh acme-plugins' }));
    await waitFor(() => expect(refreshCalls(communitySource.id)).toHaveLength(1));
  });

  it('shows skeletons and per-source spinners before sources settle', async () => {
    let release: () => void = () => undefined;
    entryDelay = new Promise((resolve) => {
      release = resolve;
    });
    renderPage();
    fireEvent.click(await screen.findByRole('tab', { name: 'Marketplace' }));

    expect(await screen.findAllByTestId('marketplace-skeleton')).not.toHaveLength(0);
    expect(screen.getByLabelText('Loading coppice')).toBeVisible();
    expect(screen.getByLabelText('Loading acme-plugins')).toBeVisible();
    expect(screen.queryByText('No plugins in the marketplace yet.')).toBeNull();

    release();
    expect(await screen.findByTestId(`marketplace-entry-${githubEntry.sourceId}/github`)).toBeVisible();
    expect(screen.queryByTestId('marketplace-skeleton')).toBeNull();
  });

  it('retries a failed source without hiding the others', async () => {
    entryErrors[communitySource.id] = 'catalog missing';
    renderPage();
    await openMarketplace();

    expect(entryCard(githubEntry)).toBeVisible();
    expect(screen.getByText(/acme-plugins couldn’t load/)).toBeVisible();
    const failed = screen.getByTestId('marketplace-source-chip-acme-plugins');
    expect(within(failed).getByLabelText('Retry acme-plugins')).toBeVisible();

    const before = mocks.apiFetch.mock.calls.filter(([path]) =>
      String(path).includes(`source=${communitySource.id}`),
    ).length;
    fireEvent.click(failed);
    await waitFor(() =>
      expect(
        mocks.apiFetch.mock.calls.filter(([path]) =>
          String(path).includes(`source=${communitySource.id}`),
        ).length,
      ).toBeGreaterThan(before),
    );
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'github' } });
    expect(entryCard(githubEntry)).toBeVisible();
  });

  it('ranks name matches ahead of description matches', async () => {
    const exact: MarketplaceEntry = {
      ...reviewEntry,
      name: 'review',
      description: 'Exact name',
    };
    const prefix: MarketplaceEntry = {
      ...reviewEntry,
      name: 'review-kit',
      description: 'Prefix name',
    };
    const contains: MarketplaceEntry = {
      ...reviewEntry,
      name: 'code-review',
      description: 'Contains name',
    };
    const described: MarketplaceEntry = {
      ...githubEntry,
      name: 'github',
      description: 'Issues, pull requests, reviews and CI',
    };
    entries = [described, contains, prefix, exact];
    renderPage();
    await openMarketplace();

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'review' } });
    const cards = screen.getAllByTestId(/^marketplace-entry-/);
    expect(cards.map((card) => card.getAttribute('data-testid'))).toEqual([
      `marketplace-entry-${exact.sourceId}/review`,
      `marketplace-entry-${prefix.sourceId}/review-kit`,
      `marketplace-entry-${contains.sourceId}/code-review`,
      `marketplace-entry-${described.sourceId}/github`,
    ]);
  });

  it('shows why an entry is already installed from another source', async () => {
    entries = [
      githubEntry,
      {
        ...reviewEntry,
        installable: false,
        reason: 'already installed as github',
      },
    ];
    renderPage();
    await openMarketplace();

    const card = entryCard(reviewEntry);
    expect(within(card).getByText('already installed as github')).toBeVisible();
    expect(within(card).queryByRole('button', { name: 'Install' })).toBeNull();
  });

  function entriesCalls() {
    return mocks.apiFetch.mock.calls.filter(
      ([path, init]) =>
        String(path).startsWith('/api/marketplace/entries') && (init?.method ?? 'GET') === 'GET',
    );
  }

  function entriesCallsFor(id: string) {
    return entriesCalls().filter(([path]) => String(path).includes(`source=${id}`));
  }

  it('a refresh of one source refetches only that source’s entries', async () => {
    renderPage();
    await openMarketplace();

    const coppiceBefore = entriesCallsFor(coppiceSource.id).length;
    const communityBefore = entriesCallsFor(communitySource.id).length;
    expect(coppiceBefore).toBeGreaterThan(0);
    expect(communityBefore).toBeGreaterThan(0);

    const dialog = await openSources();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Refresh acme-plugins' }));
    await waitFor(() => expect(refreshCalls(communitySource.id)).toHaveLength(1));
    await waitFor(() => expect(entriesCallsFor(communitySource.id).length).toBe(communityBefore + 1));
    expect(entriesCallsFor(coppiceSource.id)).toHaveLength(coppiceBefore);
  });

  it('a background refetch of loaded entries shows no skeletons or spinners', async () => {
    const { client } = renderPage();
    await openMarketplace();
    expect(screen.queryByTestId('marketplace-skeleton')).toBeNull();

    const hold = deferred();
    entryDelay = hold.promise;
    void client.invalidateQueries({ queryKey: MARKETPLACE_ENTRIES_QUERY_KEY });
    await waitFor(() => expect(entriesCalls().length).toBeGreaterThan(2));

    expect(screen.queryByTestId('marketplace-skeleton')).toBeNull();
    expect(screen.queryByLabelText('Loading coppice')).toBeNull();
    expect(screen.queryByLabelText('Loading acme-plugins')).toBeNull();
    expect(screen.queryByText(/Fetching \d+ sources/)).toBeNull();
    expect(entryCard(githubEntry)).toBeVisible();

    hold.resolve();
    expect(entryCard(githubEntry)).toBeVisible();
  });

  it('a retry shows the spinner while the refresh is in flight', async () => {
    sources = [
      { ...coppiceSource, refreshedAt: '2026-10-31T10:00:00Z' },
      { ...communitySource, lastError: 'catalog missing' },
    ];
    const hold = deferred();
    refreshDelay = hold.promise;
    renderPage();
    await openMarketplace();

    const failed = screen.getByTestId('marketplace-source-chip-acme-plugins');
    expect(within(failed).getByLabelText('Retry acme-plugins')).toBeVisible();

    fireEvent.click(failed);
    expect(await screen.findByLabelText('Loading acme-plugins')).toBeVisible();
    expect(within(failed).queryByLabelText('Retry acme-plugins')).toBeNull();
    expect(screen.getByText(/Fetching 1 source/)).toBeVisible();
    expect(screen.queryByText(/acme-plugins couldn’t load/)).toBeNull();

    hold.resolve();
  });

  it('does not flash no plugins between a first refresh settling and its entries arriving', async () => {
    const refreshHold = deferred();
    const afterHold = deferred();
    refreshDelay = refreshHold.promise;
    postRefreshEntryDelay = afterHold.promise;
    sources = [unfetchedSource];
    entries = [{ ...reviewEntry, sourceId: unfetchedSource.id, sourceName: unfetchedSource.name }];
    renderPage();
    fireEvent.click(await screen.findByRole('tab', { name: 'Marketplace' }));

    expect(await screen.findAllByTestId('marketplace-skeleton')).not.toHaveLength(0);
    expect(screen.queryByText('No plugins in the marketplace yet.')).toBeNull();

    refreshHold.resolve();
    await waitFor(() => expect(entriesCallsFor(unfetchedSource.id).length).toBeGreaterThan(1));

    expect(screen.queryByText('No plugins in the marketplace yet.')).toBeNull();
    expect(screen.queryAllByTestId('marketplace-skeleton').length).toBeGreaterThan(0);

    afterHold.resolve();
    expect(
      await screen.findByTestId(`marketplace-entry-${unfetchedSource.id}/review-kit`),
    ).toBeVisible();
  });

  it('shows skeletons while sources load instead of a loading sentence', async () => {
    const hold = deferred();
    sourcesDelay = hold.promise;
    renderPage();
    fireEvent.click(await screen.findByRole('tab', { name: 'Marketplace' }));

    expect(screen.queryByText('Loading marketplace…')).toBeNull();
    expect(await screen.findAllByTestId('marketplace-skeleton')).not.toHaveLength(0);

    hold.resolve();
    expect(await screen.findByTestId(`marketplace-entry-${githubEntry.sourceId}/github`)).toBeVisible();
  });
});
