import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { PluginBuild } from '../../lib/schemas/pluginBuild';
import type { Plugin } from '../../lib/schemas/plugin';
import { BUILDER_COPY } from './builderCopy';
import { PluginBuildTryView } from './PluginBuildTryView';

const mocks = vi.hoisted(() => ({
  apiFetch: vi.fn(),
  plugins: [] as Plugin[],
}));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('./usePlugins', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./usePlugins')>();
  return {
    ...actual,
    usePlugins: () => ({ data: mocks.plugins, isLoading: false }),
    useTestPlugin: () => ({
      mutateAsync: vi.fn(),
      isPending: false,
      data: undefined,
      reset: vi.fn(),
    }),
  };
});

const BUILD_ID = '00000000-0000-4000-8000-0000000000aa';
const PLUGIN_ID = '00000000-0000-4000-8000-0000000000cc';

function tryBuild(overrides: Partial<PluginBuild> = {}): PluginBuild {
  return {
    id: BUILD_ID,
    answers: {},
    plans: [
      {
        revision: 1,
        body: 'plan',
        advanced: { pluginName: 'changelog-helper', version: '0.1.0' },
      },
    ],
    status: 'ready_to_try',
    pluginName: 'changelog-helper',
    workspacePath: '/tmp/x',
    publishTarget: null,
    planQuestions: null,
    checkErrors: null,
    formatRetries: 0,
    pluginId: PLUGIN_ID,
    testResult: { servers: [{ name: 's', kind: 'stdio', status: 'error', tools: [] }] },
    activeRunId: null,
    lastRunId: null,
    lastRunStatus: null,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

function plugin(): Plugin {
  return {
    id: PLUGIN_ID,
    pluginDirId: '00000000-0000-4000-8000-0000000000dd',
    relPath: 'changelog-helper',
    name: 'changelog-helper',
    version: '0.1.0',
    description: '',
    source: 'local',
    gitUrl: null,
    gitRef: null,
    gitCommit: null,
    status: 'ok',
    error: null,
    enabled: false,
    skills: [{ name: 'update-changelog', description: '', relPath: 'skills/x', error: null, enabled: true }],
    mcpServers: [],
    settings: [],
    unsupported: [],
    marketplace: null,
    external: null,
    gitRoot: null,
    builderMade: false,
    publishTarget: null,
    agentAccess: 'all',
    agentIds: [],
  };
}

function renderTry(build: PluginBuild) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConfirmProvider>
          <PluginBuildTryView
            build={build}
            onAccepted={vi.fn() as (
              pluginId: string | null,
              name: string,
              publish: unknown,
              publishTarget: string | null,
            ) => void}
            onDiscarded={vi.fn()}
          />
        </ConfirmProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('PluginBuildTryView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.plugins = [plugin()];
    mocks.apiFetch.mockResolvedValue(
      new Response(JSON.stringify(tryBuild()), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
  });

  it('keeps Accept disabled until a passing test', () => {
    renderTry(tryBuild());
    expect(screen.getByRole('button', { name: BUILDER_COPY.accept })).toBeDisabled();
    expect(screen.getByRole('button', { name: BUILDER_COPY.accept })).toHaveAttribute(
      'title',
      BUILDER_COPY.acceptDisabledTitle,
    );
  });

  it('enables Accept when stored test passed', () => {
    renderTry(tryBuild({ testResult: { servers: [] } }));
    expect(screen.getByRole('button', { name: BUILDER_COPY.accept })).toBeEnabled();
  });

  it('confirms Discard and posts discard', async () => {
    const onDiscarded = vi.fn();
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ConfirmProvider>
            <PluginBuildTryView
              build={tryBuild({ testResult: { servers: [] } })}
              onAccepted={vi.fn() as (
              pluginId: string | null,
              name: string,
              publish: unknown,
              publishTarget: string | null,
            ) => void}
              onDiscarded={onDiscarded}
            />
          </ConfirmProvider>
        </MemoryRouter>
      </QueryClientProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.discard }));
    const dialog = await screen.findByRole('dialog');
    expect(
      within(dialog).getByRole('heading', { name: BUILDER_COPY.discardTitle }),
    ).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: BUILDER_COPY.discardConfirm }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/plugin-builds/${BUILD_ID}/discard`,
        expect.objectContaining({ method: 'POST' }),
      ),
    );
  });
});
