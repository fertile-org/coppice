import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Plugin } from '../../lib/schemas/plugin';
import { AgentsPage } from './AgentsPage';
import type { Agent } from './useAgents';

const mocks = vi.hoisted(() => ({
  plugins: [] as Plugin[],
  assigned: [] as string[],
  setPlugins: vi.fn(),
  updateAgent: vi.fn(),
}));

vi.mock('../plugins/usePlugins', () => ({
  usePlugins: () => ({ data: mocks.plugins, isLoading: false }),
}));

vi.mock('../settings/useKnowledgeSettings', () => ({
  useKnowledgeSettings: () => ({ data: null }),
}));

vi.mock('./KnowledgeCompactionCard', () => ({
  KnowledgeCompactionCard: () => null,
}));

const agent: Agent = {
  id: '00000000-0000-4000-8000-0000000000a1',
  name: 'Builder',
  role: 'Developer',
  skills: [],
  responsibilities: [],
  systemPrompt: '',
  connector: 'mock',
  health: 'healthy',
  enabled: true,
  createdAt: '2026-10-01T00:00:00Z',
  updatedAt: '2026-10-01T00:00:00Z',
};

vi.mock('./useAgents', () => ({
  useAgents: () => ({ data: [agent], isLoading: false, isError: false, refetch: vi.fn() }),
  useAgentPresets: () => ({ data: [], isLoading: false }),
  useConnectors: () => ({ data: [{ id: 'mock' }] }),
  useUpdateAgentMutation: () => ({ mutateAsync: vi.fn() }),
  useCreateAgent: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useUpdateAgent: () => ({ mutateAsync: mocks.updateAgent, isPending: false }),
  useAgentPlugins: () => ({ data: mocks.assigned, error: null, isSuccess: true }),
  useSetAgentPlugins: () => ({ mutateAsync: mocks.setPlugins, isPending: false }),
  useModelProviders: () => ({ data: [], isLoading: false }),
  useModels: () => ({ data: [], isLoading: false }),
  fetchAgentPlugins: vi.fn(),
}));

function plugin(overrides: Partial<Plugin>): Plugin {
  return {
    id: '00000000-0000-4000-8000-000000000010',
    pluginDirId: '00000000-0000-4000-8000-000000000001',
    relPath: 'plugin',
    name: 'plugin',
    version: '1.0.0',
    description: '',
    source: 'local',
    gitUrl: null,
    gitRef: null,
    gitCommit: null,
    status: 'ok',
    error: null,
    enabled: true,
    skills: [],
    mcpServers: [],
    settings: [],
    unsupported: [],
    ...overrides,
  };
}

const hello = plugin({ id: '00000000-0000-4000-8000-0000000000b1', name: 'hello-coppice' });
const other = plugin({ id: '00000000-0000-4000-8000-0000000000b2', name: 'other-plugin' });

function SearchProbe() {
  const location = useLocation();
  return <span data-testid="search">{location.search}</span>;
}

function renderPage(url: string) {
  return render(
    <MemoryRouter initialEntries={[url]}>
      <AgentsPage />
      <SearchProbe />
    </MemoryRouter>,
  );
}

describe('AgentsPage rows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.plugins = [];
    mocks.assigned = [];
  });

  it('clicking a row opens the edit dialog', async () => {
    renderPage('/agents');

    fireEvent.click(screen.getByText('Developer'));

    expect(await screen.findByRole('dialog')).toHaveTextContent('Edit agent');
  });

  it('Enter on a focused row opens the edit dialog', async () => {
    renderPage('/agents');

    fireEvent.keyDown(screen.getByRole('row', { name: /Builder/ }), { key: 'Enter' });

    expect(await screen.findByRole('dialog')).toBeVisible();
  });

  it('the enable toggle does not open the dialog', () => {
    renderPage('/agents');

    fireEvent.click(screen.getByRole('button', { name: 'Disable' }));

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});

describe('AgentsPage plugin deep link', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.plugins = [hello, other];
    mocks.assigned = [other.id];
    mocks.updateAgent.mockResolvedValue({});
    mocks.setPlugins.mockResolvedValue({});
  });

  it('shows no banner without a plugin param', () => {
    renderPage('/agents');

    expect(screen.queryByTestId('plugin-deeplink-banner')).not.toBeInTheDocument();
  });

  it('banner names the plugin and Done clears it', () => {
    renderPage(`/agents?plugin=${hello.id}`);

    const banner = screen.getByTestId('plugin-deeplink-banner');
    expect(within(banner).getByText('hello-coppice')).toBeVisible();
    fireEvent.click(within(banner).getByRole('button', { name: 'Done' }));

    expect(screen.queryByTestId('plugin-deeplink-banner')).not.toBeInTheDocument();
    expect(screen.getByTestId('search')).toHaveTextContent('');
  });

  it('Edit pre-selects the plugin on top of existing ones and saves both', async () => {
    renderPage(`/agents?plugin=${hello.id}`);

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('checkbox', { name: /hello-coppice/ })).toBeChecked();
    expect(within(dialog).getByRole('checkbox', { name: /other-plugin/ })).toBeChecked();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save changes' }));

    await waitFor(() =>
      expect(mocks.setPlugins).toHaveBeenCalledWith({
        agentId: agent.id,
        pluginIds: [other.id, hello.id],
      }),
    );
  });

  it('a plugin that is not enabled shows a warning and is not pre-selected', async () => {
    mocks.plugins = [{ ...hello, enabled: false }, other];
    renderPage(`/agents?plugin=${hello.id}`);

    expect(screen.getByTestId('plugin-deeplink-banner')).toHaveTextContent(
      /not enabled or not available/,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).queryByRole('checkbox', { name: /hello-coppice/ })).toBeNull();
  });
});
