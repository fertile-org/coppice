import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { PluginBuild } from '../../lib/schemas/pluginBuild';
import { BUILDER_COPY } from './builderCopy';
import { PluginBuildProgress } from './PluginBuildProgress';

const mocks = vi.hoisted(() => ({
  apiFetch: vi.fn(),
}));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../runs/LiveRunView', () => ({
  LiveRunView: () => <div>Live log</div>,
}));

const BUILD_ID = '00000000-0000-4000-8000-0000000000aa';
const RUN_ID = '00000000-0000-4000-8000-0000000000bb';

function building(overrides: Partial<PluginBuild> = {}): PluginBuild {
  return {
    id: BUILD_ID,
    answers: {},
    plans: [{ revision: 1, body: 'plan' }],
    status: 'building',
    pluginName: 'changelog-helper',
    workspacePath: '/tmp/x',
    publishTarget: null,
    planQuestions: null,
    checkErrors: null,
    formatRetries: 0,
    pluginId: null,
    testResult: null,
    activeRunId: RUN_ID,
    lastRunId: RUN_ID,
    lastRunStatus: 'running',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

function renderProgress(build: PluginBuild) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConfirmProvider>
          <PluginBuildProgress build={build} onDiscarded={vi.fn()} />
        </ConfirmProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('PluginBuildProgress', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.apiFetch.mockResolvedValue(new Response('{}', { status: 200 }));
  });

  it('confirms Stop and posts agent-runs stop, not build cancel', async () => {
    renderProgress(building());
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.stopBuild }));
    const dialog = await screen.findByRole('dialog');
    expect(
      within(dialog).getByRole('heading', { name: BUILDER_COPY.stopBuildTitle }),
    ).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: BUILDER_COPY.stopBuildConfirm }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/agent-runs/${RUN_ID}/stop`,
        expect.objectContaining({ method: 'POST' }),
      ),
    );
    expect(mocks.apiFetch).not.toHaveBeenCalledWith(
      expect.stringContaining('/cancel'),
      expect.anything(),
    );
  });
});
