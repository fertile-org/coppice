import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import { PluginBuilderPage } from './PluginBuilderPage';
import { PluginBuildPlanReview } from './PluginBuildPlanReview';
import { BUILDER_COPY } from './builderCopy';
import { PLAN_COPY } from '../plan/copy';
import type { PluginBuild } from '../../lib/schemas/pluginBuild';

const mocks = vi.hoisted(() => ({
  apiFetch: vi.fn(),
  isDesktopShell: vi.fn(() => true),
}));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../../lib/desktop', () => ({
  isDesktopShell: () => mocks.isDesktopShell(),
}));

vi.mock('../auth/useSession', () => ({
  useSession: () => ({ user: { role: 'admin' }, loading: false }),
}));

const BUILD_ID = '00000000-0000-4000-8000-0000000000aa';

function draftBuild(overrides: Partial<PluginBuild> = {}): PluginBuild {
  return {
    id: BUILD_ID,
    answers: {},
    plans: [],
    status: 'drafting',
    pluginName: null,
    workspacePath: '/tmp/plugin-builds/' + BUILD_ID,
    publishTarget: null,
    planQuestions: null,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function renderBuilder(initial = `/settings/plugins/build?id=${BUILD_ID}`) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[initial]}>
        <ConfirmProvider>
          <Routes>
            <Route path="/settings/plugins/build" element={<PluginBuilderPage />} />
            <Route path="/settings/plugins" element={<div>Plugins home</div>} />
          </Routes>
        </ConfirmProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function renderPlan(build: PluginBuild) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConfirmProvider>
          <PluginBuildPlanReview build={build} />
        </ConfirmProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('PluginBuilderPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.isDesktopShell.mockReturnValue(true);
    mocks.apiFetch.mockImplementation((path: string, init: RequestInit = {}) => {
      const method = init.method ?? 'GET';
      if (path === `/api/plugin-builds/${BUILD_ID}` && method === 'GET') {
        return Promise.resolve(json(draftBuild()));
      }
      if (path === `/api/plugin-builds/${BUILD_ID}` && method === 'PATCH') {
        const body = JSON.parse(init.body as string);
        return Promise.resolve(json(draftBuild({ answers: body.answers ?? {} })));
      }
      if (path === `/api/plugin-builds/${BUILD_ID}/plan-runs` && method === 'POST') {
        return Promise.resolve(
          json({
            build: draftBuild({ status: 'planning' }),
            runId: '00000000-0000-4000-8000-0000000000bb',
          }),
        );
      }
      if (path === '/api/plugin-builds' && method === 'POST') {
        return Promise.resolve(json(draftBuild()));
      }
      return Promise.reject(new Error(`unexpected ${method} ${path}`));
    });
  });

  it('redirects to Plugins when not in the desktop shell', async () => {
    mocks.isDesktopShell.mockReturnValue(false);
    renderBuilder();
    expect(await screen.findByText('Plugins home')).toBeInTheDocument();
  });

  it('retains answers on Back through step 5 and See the plan', async () => {
    renderBuilder();
    expect(
      await screen.findByRole('heading', { name: BUILDER_COPY.q1Title }),
    ).toBeInTheDocument();

    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: 'Keep changelog current' },
    });
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.next }));

    expect(
      await screen.findByRole('heading', { name: BUILDER_COPY.q2Title }),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: 'Read commits\nWrite summary' },
    });
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.next }));

    expect(
      await screen.findByRole('heading', { name: BUILDER_COPY.q3Title }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.q3Git }));
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.next }));

    expect(
      await screen.findByRole('heading', { name: BUILDER_COPY.q4Title }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.q4None }));
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.next }));

    expect(
      await screen.findByRole('heading', { name: BUILDER_COPY.q5Title }),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: BUILDER_COPY.seeThePlan })).toBeEnabled();

    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.back }));
    expect(screen.getByRole('button', { name: BUILDER_COPY.q4None })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.back }));
    expect(screen.getByRole('button', { name: BUILDER_COPY.q3Git })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.back }));
    expect(screen.getByRole('textbox')).toHaveValue('Read commits\nWrite summary');
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.back }));
    expect(screen.getByRole('textbox')).toHaveValue('Keep changelog current');

    // Forward again to See the plan
    fireEvent.click(screen.getByRole('button', { name: BUILDER_COPY.next }));
    fireEvent.click(await screen.findByRole('button', { name: BUILDER_COPY.next }));
    fireEvent.click(await screen.findByRole('button', { name: BUILDER_COPY.next }));
    fireEvent.click(await screen.findByRole('button', { name: BUILDER_COPY.next }));
    fireEvent.click(await screen.findByRole('button', { name: BUILDER_COPY.seeThePlan }));

    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        `/api/plugin-builds/${BUILD_ID}/plan-runs`,
        expect.objectContaining({ method: 'POST' }),
      ),
    );
    expect(await screen.findByText(BUILDER_COPY.writingPlan)).toBeInTheDocument();
  });

  it('prompts Leave/Stay via useConfirm on Esc when answers exist', async () => {
    renderBuilder();
    await screen.findByRole('heading', { name: BUILDER_COPY.q1Title });
    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: 'Something' },
    });
    fireEvent.keyDown(window, { key: 'Escape' });
    const dialog = await screen.findByRole('dialog');
    expect(
      within(dialog).getByRole('heading', { name: BUILDER_COPY.leaveTitle }),
    ).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: BUILDER_COPY.leaveStay }));
    expect(screen.getByRole('heading', { name: BUILDER_COPY.q1Title })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'Escape' });
    const dialog2 = await screen.findByRole('dialog');
    fireEvent.click(within(dialog2).getByRole('button', { name: BUILDER_COPY.leaveConfirm }));
    expect(await screen.findByText('Plugins home')).toBeInTheDocument();
  });
});

describe('PluginBuildPlanReview', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.apiFetch.mockResolvedValue(json({}));
  });

  it('shows Ask for changes and Approve plan without a skip-review checkbox', () => {
    renderPlan(
      draftBuild({
        status: 'plan_ready',
        plans: [
          {
            revision: 1,
            body: '## changelog-helper\n\nKeeps CHANGELOG.md up to date.',
            advanced: { pluginName: 'changelog-helper', version: '0.1.0' },
          },
        ],
      }),
    );
    expect(screen.getByRole('button', { name: PLAN_COPY.askForChanges })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: PLAN_COPY.approvePlan })).toBeInTheDocument();
    expect(screen.queryByLabelText(PLAN_COPY.skipHumanReview)).not.toBeInTheDocument();
    expect(screen.queryByText(PLAN_COPY.skipHumanReview)).not.toBeInTheDocument();
    expect(screen.getByText(BUILDER_COPY.approveStartsBuild)).toBeInTheDocument();
    expect(screen.getByText(BUILDER_COPY.advanced)).toBeInTheDocument();
  });
});
