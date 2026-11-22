import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { SessionContext, type SessionContextValue } from '../auth/useSession';
import { ThemeProvider } from '../theme/ThemeProvider';
import { INIT_COPY } from './copy';
import { InitWizard } from './InitWizard';
import { InitWizardOffer } from './InitWizardOffer';
const previewReady = {
  presetId: 'starter',
  status: 'ready',
  connectors: [{ id: 'claude-code', displayName: 'Claude Code' }],
  members: [
    { presetKey: 'pm', name: 'PM', role: 'PM' },
    { presetKey: 'tech_lead', name: 'Technical Lead', role: 'Technical Lead' },
    { presetKey: 'backend_engineer', name: 'Backend Engineer', role: 'Backend Engineer' },
    { presetKey: 'frontend_engineer', name: 'Frontend Engineer', role: 'Frontend Engineer' },
    { presetKey: 'qc', name: 'QC', role: 'QC' },
  ],
};

const inspectedReady = {
  suggestedName: 'demo',
  remoteUrl: 'https://example.com/demo.git',
  defaultBranch: 'main',
  verificationStatus: 'ready',
  verificationError: null,
};

function jsonResponse(body: unknown, status = 200) {
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    }),
  );
}

function renderWizard() {
  const onSkip = vi.fn();
  const onCreated = vi.fn();
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <InitWizard open onSkip={onSkip} onCreated={onCreated} />
    </QueryClientProvider>,
  );
  return { onSkip, onCreated };
}

describe('InitWizard', () => {
  const fetchMock = vi.fn();

  beforeEach(() => {
    localStorage.clear();
    fetchMock.mockReset();
    fetchMock.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/repos/inspect-path') return jsonResponse(inspectedReady);
      if (path === '/api/init' && init?.method === 'POST') {
        return jsonResponse({
          boardId: '00000000-0000-4000-8000-0000000000b1',
          repositoryId: '00000000-0000-4000-8000-0000000000b2',
          connectorId: 'claude-code',
          connectorDisplayName: 'Claude Code',
          connectorTurnedOn: true,
          agents: [],
        });
      }
      if (path === '/api/init') return jsonResponse(previewReady);
      if (path === '/api/connectors') {
        return jsonResponse({
          items: [{ id: 'claude-code', displayName: 'Claude Code', readiness: 'ready' }],
        });
      }
      return jsonResponse([]);
    });
    vi.stubGlobal('fetch', fetchMock);
    window.coppiceDesktop = {
      pickDirectory: () => Promise.resolve('/tmp/demo'),
    };
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    delete window.coppiceDesktop;
  });

  it('skips without creating anything', async () => {
    const { onSkip } = renderWizard();
    expect(await screen.findByRole('heading', { name: INIT_COPY.title })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: INIT_COPY.skip }));
    expect(onSkip).toHaveBeenCalledOnce();
    expect(postCount()).toBe(0);
  });

  it('does not create until the last step is confirmed', async () => {
    const { onCreated } = renderWizard();
    fireEvent.click(await screen.findByRole('button', { name: INIT_COPY.browse }));
    expect(await screen.findByDisplayValue('/tmp/demo')).toBeInTheDocument();
    expect(await screen.findByDisplayValue('demo')).toBeInTheDocument();
    const next = screen.getByRole('button', { name: INIT_COPY.next });
    await waitFor(() => expect(next).toBeEnabled());
    fireEvent.click(next);
    expect(screen.getByRole('heading', { name: INIT_COPY.board })).toBeInTheDocument();
    expect(screen.getByLabelText(INIT_COPY.name)).toHaveValue('demo');
    expect(postCount()).toBe(0);

    fireEvent.click(screen.getByRole('button', { name: INIT_COPY.next }));
    expect(screen.getByLabelText('Agent CLI')).toHaveValue('claude-code');
    expect(postCount()).toBe(0);
    expect(document.body.textContent).not.toMatch(/mock|fake/i);

    fireEvent.click(screen.getByRole('button', { name: INIT_COPY.create }));
    await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
    expect(postCount()).toBe(1);
    const body = JSON.parse(String(postCall()?.[1]?.body));
    expect(body).toEqual({
      presetId: 'starter',
      boardName: 'demo',
      connectorId: 'claude-code',
      repository: {
        name: 'demo',
        localPath: '/tmp/demo',
        remoteUrl: 'https://example.com/demo.git',
        defaultBranch: 'main',
      },
    });
    expect(onCreated).toHaveBeenCalledWith({
      boardId: '00000000-0000-4000-8000-0000000000b1',
      connectorTurnedOn: true,
      connectorDisplayName: 'Claude Code',
    });
  });

  it('keeps Create off while no CLI is ready and does not send a prompt', async () => {
    fetchMock.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/repos/inspect-path') return jsonResponse(inspectedReady);
      if (path === '/api/init' && init?.method === 'POST') {
        return jsonResponse({ code: 'none_ready' }, 409);
      }
      if (path === '/api/init') {
        return jsonResponse({ ...previewReady, status: 'none', connectors: [] });
      }
      if (path === '/api/connectors') {
        return jsonResponse({
          items: [
            {
              id: 'kilo-code',
              displayName: 'Kilo Code',
              console: 'structured',
              caps: { readOnlyTools: false, chatResume: false },
              readiness: 'found',
            },
          ],
        });
      }
      return jsonResponse([]);
    });
    renderWizard();
    fireEvent.click(await screen.findByRole('button', { name: INIT_COPY.browse }));
    const next = screen.getByRole('button', { name: INIT_COPY.next });
    await waitFor(() => expect(next).toBeEnabled());
    fireEvent.click(next);
    fireEvent.click(screen.getByRole('button', { name: INIT_COPY.next }));
    expect(
      await screen.findByText(
        'No agent CLI is ready yet. Install one and sign in, then check it in System → Connectors.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText('Create stays off until a CLI shows Ready.')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Agent CLI')).not.toBeInTheDocument();
    expect(
      await screen.findByText(
        "Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: INIT_COPY.create })).toBeDisabled();
    expect(postCount()).toBe(0);
    expect(document.body.textContent).not.toMatch(/mock|fake/i);
  });

  it('refuses a folder that is not a git checkout', async () => {
    fetchMock.mockImplementation((path: string) => {
      if (path === '/api/repos/inspect-path') {
        return jsonResponse({
          ...inspectedReady,
          suggestedName: null,
          remoteUrl: null,
          defaultBranch: null,
          verificationStatus: 'not_git_repo',
        });
      }
      if (path === '/api/init') return jsonResponse(previewReady);
      if (path === '/api/connectors') return jsonResponse({ items: [] });
      return jsonResponse([]);
    });
    renderWizard();
    fireEvent.click(await screen.findByRole('button', { name: INIT_COPY.browse }));
    expect(await screen.findByText(INIT_COPY.notGitRepo)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: INIT_COPY.next })).toBeDisabled();
    expect(screen.queryByText("Choose a folder that's a git repository.")).not.toBeInTheDocument();
    expect(postCount()).toBe(0);
  });

  it('sends the Ready CLI picked in the dropdown', async () => {
    fetchMock.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/repos/inspect-path') return jsonResponse(inspectedReady);
      if (path === '/api/init' && init?.method === 'POST') {
        return jsonResponse({
          boardId: '00000000-0000-4000-8000-0000000000b1',
          repositoryId: '00000000-0000-4000-8000-0000000000b2',
          connectorId: 'claude-code',
          connectorDisplayName: 'Claude Code',
          connectorTurnedOn: true,
          agents: [],
        });
      }
      if (path === '/api/init') {
        return jsonResponse({
          ...previewReady,
          connectors: [
            { id: 'cursor', displayName: 'Cursor' },
            { id: 'claude-code', displayName: 'Claude Code' },
          ],
        });
      }
      if (path === '/api/connectors') return jsonResponse({ items: [] });
      return jsonResponse([]);
    });
    const { onCreated } = renderWizard();
    fireEvent.click(await screen.findByRole('button', { name: INIT_COPY.browse }));
    const next = screen.getByRole('button', { name: INIT_COPY.next });
    await waitFor(() => expect(next).toBeEnabled());
    fireEvent.click(next);
    fireEvent.click(screen.getByRole('button', { name: INIT_COPY.next }));
    const cli = await screen.findByLabelText('Agent CLI');
    expect(cli).toHaveValue('cursor');
    expect(screen.getByText('They run on Cursor with its default model.')).toBeInTheDocument();
    fireEvent.change(cli, { target: { value: 'claude-code' } });
    expect(screen.getByText('They run on Claude Code with its default model.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: INIT_COPY.create }));
    await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
    const body = JSON.parse(String(postCall()?.[1]?.body));
    expect(body.connectorId).toBe('claude-code');
    expect(screen.queryByText('That folder is already registered.')).not.toBeInTheDocument();
  });
});

function postCount() {
  return fetchMockCalls().filter((call) => call[0] === '/api/init' && call[1]?.method === 'POST')
    .length;
}

function postCall() {
  return fetchMockCalls().find((call) => call[0] === '/api/init' && call[1]?.method === 'POST');
}

function fetchMockCalls(): [string, RequestInit?][] {
  return (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls as [string, RequestInit?][];
}

function session(desktopMode: boolean, role = 'admin'): SessionContextValue {
  return {
    user: { id: '1', email: 'admin@localhost', role },
    loading: false,
    desktopMode,
    establishSession: () => {},
    logout: () => Promise.resolve(),
  };
}

function renderOffer(desktopMode: boolean, role = 'admin') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <SessionContext.Provider value={session(desktopMode, role)}>
          <ThemeProvider>
            <MemoryRouter>
              <InitWizardOffer />
            </MemoryRouter>
          </ThemeProvider>
        </SessionContext.Provider>
      </ToastProvider>
    </QueryClientProvider>,
  );
  return client;
}

function deferredJson(body: unknown, status = 200) {
  let resolve!: (value: Response) => void;
  const promise = new Promise<Response>((r) => {
    resolve = r;
  });
  return {
    promise,
    resolve: () =>
      resolve(
        new Response(JSON.stringify(body), {
          status,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
  };
}

describe('InitWizardOffer', () => {
  const fetchMock = vi.fn();

  beforeEach(() => {
    localStorage.clear();
    fetchMock.mockReset();
    fetchMock.mockImplementation((path: string) => {
      if (path === '/api/boards') return jsonResponse([]);
      if (path === '/api/repos') return jsonResponse([]);
      if (path === '/api/agents') return jsonResponse({ items: [] });
      if (path === '/api/init') return jsonResponse(previewReady);
      if (path === '/api/connectors') return jsonResponse({ items: [] });
      return jsonResponse([]);
    });
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('opens as a welcome page on an empty desktop workspace', async () => {
    renderOffer(true);
    expect(await screen.findByRole('heading', { name: INIT_COPY.title })).toBeInTheDocument();
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-open', 'yes');
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-settled', 'yes');
    expect(screen.getByTestId('init-welcome')).toBeInTheDocument();
    expect(screen.queryByTestId('init-cover')).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('shows a cover while checks load so the home page never flashes first', async () => {
    const boards = deferredJson([]);
    const repos = deferredJson([]);
    const agents = deferredJson({ items: [] });
    fetchMock.mockImplementation((path: string) => {
      if (path === '/api/boards') return boards.promise;
      if (path === '/api/repos') return repos.promise;
      if (path === '/api/agents') return agents.promise;
      if (path === '/api/init') return jsonResponse(previewReady);
      if (path === '/api/connectors') return jsonResponse({ items: [] });
      return jsonResponse([]);
    });
    renderOffer(true);
    expect(screen.getByTestId('init-cover')).toBeInTheDocument();
    expect(screen.queryByTestId('init-welcome')).not.toBeInTheDocument();
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-settled', 'no');
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-open', 'no');

    boards.resolve();
    repos.resolve();
    agents.resolve();
    expect(await screen.findByTestId('init-welcome')).toBeInTheDocument();
    expect(screen.queryByTestId('init-cover')).not.toBeInTheDocument();
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-settled', 'yes');
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-open', 'yes');
  });

  it('leaves the welcome page when setup is skipped', async () => {
    renderOffer(true);
    fireEvent.click(await screen.findByRole('button', { name: INIT_COPY.skip }));
    await waitFor(() => expect(screen.queryByTestId('init-welcome')).not.toBeInTheDocument());
    expect(localStorage.getItem('coppice.initWizard')).toBe('skipped');
  });

  it('stays closed when a repo already exists', async () => {
    fetchMock.mockImplementation((path: string) => {
      if (path === '/api/boards') return jsonResponse([]);
      if (path === '/api/repos') return jsonResponse([{ id: 'repo-1' }]);
      if (path === '/api/agents') return jsonResponse({ items: [] });
      return jsonResponse([]);
    });
    renderOffer(true);
    await waitFor(() =>
      expect(screen.getByTestId('init-offer')).toHaveAttribute('data-settled', 'yes'),
    );
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-open', 'no');
    expect(screen.queryByTestId('init-cover')).not.toBeInTheDocument();
    expect(screen.queryByTestId('init-welcome')).not.toBeInTheDocument();
  });

  it('stays closed when a board already exists', async () => {
    fetchMock.mockImplementation((path: string) => {
      if (path === '/api/boards') return jsonResponse([{ id: 'board-1', name: 'Main' }]);
      if (path === '/api/repos') return jsonResponse([]);
      if (path === '/api/agents') return jsonResponse({ items: [] });
      return jsonResponse([]);
    });
    renderOffer(true);
    await waitFor(() =>
      expect(screen.getByTestId('init-offer')).toHaveAttribute('data-settled', 'yes'),
    );
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-open', 'no');
    expect(screen.queryByTestId('init-cover')).not.toBeInTheDocument();
    expect(screen.queryByTestId('init-welcome')).not.toBeInTheDocument();
  });

  it('clears the cover when a check errors so it never stays stuck', async () => {
    fetchMock.mockImplementation((path: string) => {
      if (path === '/api/boards') return jsonResponse([]);
      if (path === '/api/repos') return jsonResponse({ error: 'boom' }, 500);
      if (path === '/api/agents') return jsonResponse({ items: [] });
      return jsonResponse([]);
    });
    renderOffer(true);
    await waitFor(() =>
      expect(screen.getByTestId('init-offer')).toHaveAttribute('data-settled', 'yes'),
    );
    expect(screen.getByTestId('init-offer')).toHaveAttribute('data-open', 'no');
    expect(screen.queryByTestId('init-cover')).not.toBeInTheDocument();
    expect(screen.queryByTestId('init-welcome')).not.toBeInTheDocument();
  });

  it('stays closed outside the desktop app', () => {
    renderOffer(false);
    expect(screen.queryByRole('heading', { name: INIT_COPY.title })).not.toBeInTheDocument();
    expect(screen.queryByTestId('init-offer')).not.toBeInTheDocument();
    expect(screen.queryByTestId('init-cover')).not.toBeInTheDocument();
  });
});
