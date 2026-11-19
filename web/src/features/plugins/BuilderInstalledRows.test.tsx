import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import type { PluginBuild } from '../../lib/schemas/pluginBuild';
import { BUILDER_COPY } from './builderCopy';
import { BuilderInstalledRows } from './BuilderInstalledRows';

const builds: PluginBuild[] = [];

vi.mock('./usePluginBuilds', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./usePluginBuilds')>();
  return {
    ...actual,
    usePluginBuilds: () => ({ data: builds, isLoading: false }),
    useStartPluginBuildRun: () => ({ mutate: vi.fn(), isPending: false }),
    useDiscardPluginBuild: () => ({ mutate: vi.fn(), isPending: false }),
  };
});

function base(overrides: Partial<PluginBuild>): PluginBuild {
  return {
    id: '00000000-0000-4000-8000-0000000000aa',
    answers: {},
    plans: [],
    status: 'building',
    pluginName: 'changelog-helper',
    workspacePath: '/tmp/x',
    publishTarget: null,
    planQuestions: null,
    checkErrors: null,
    formatRetries: 0,
    pluginId: null,
    testResult: null,
    activeRunId: '00000000-0000-4000-8000-0000000000bb',
    lastRunId: null,
    lastRunStatus: null,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

function renderRows() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <BuilderInstalledRows />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('BuilderInstalledRows', () => {
  it('shows Building… with Open and Ready to try with Review', () => {
    builds.length = 0;
    builds.push(
      base({
        id: '00000000-0000-4000-8000-0000000000a1',
        status: 'building',
        activeRunId: '00000000-0000-4000-8000-0000000000b1',
      }),
      base({
        id: '00000000-0000-4000-8000-0000000000a2',
        status: 'ready_to_try',
        pluginName: 'ready-one',
        activeRunId: null,
        pluginId: '00000000-0000-4000-8000-0000000000c2',
      }),
    );
    renderRows();
    expect(screen.getByText(BUILDER_COPY.installedBuilding('changelog-helper'))).toBeInTheDocument();
    expect(screen.getByRole('button', { name: BUILDER_COPY.open })).toBeInTheDocument();
    expect(screen.getByText(BUILDER_COPY.installedReady('ready-one'))).toBeInTheDocument();
    expect(screen.getByRole('button', { name: BUILDER_COPY.review })).toBeInTheDocument();
  });
});
