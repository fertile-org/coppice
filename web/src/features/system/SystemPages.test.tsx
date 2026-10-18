import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { MIN_PASSWORD_CHARS } from '../../lib/passwords';
import {
  BackupPage,
  ConnectorsPage,
  LegacyLogsRedirect,
  LegacyToolsRedirect,
} from './SystemPages';

const sessionState = vi.hoisted(() => ({ desktopMode: false }));

vi.mock('../auth/useSession', () => ({
  useSession: () => ({
    user: { id: '1', email: 'admin@localhost', role: 'admin' },
    desktopMode: sessionState.desktopMode,
    establishSession: () => {},
  }),
}));

function LocationProbe() {
  const location = useLocation();
  return <p data-testid="location">{`${location.pathname}${location.search}`}</p>;
}

function renderPage(initialEntry = '/system/backup') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <MemoryRouter initialEntries={[initialEntry]}>
          <Routes>
            <Route path="/system/backup" element={<BackupPage />} />
            <Route path="/system/connectors" element={<ConnectorsPage />} />
            <Route path="/system/logs" element={<p>Logs page</p>} />
            <Route path="/tools" element={<LegacyToolsRedirect />} />
            <Route path="/logs" element={<LegacyLogsRedirect />} />
          </Routes>
          <LocationProbe />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

const fetchMock = vi.fn();

describe('System pages', () => {
  beforeEach(() => {
    sessionState.desktopMode = false;
    fetchMock.mockReset();
    fetchMock.mockImplementation((path: string) => {
      const body = path === '/api/agents' ? { items: [] } : [];
      return Promise.resolve(
        new Response(JSON.stringify(body), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    });
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('shows export and import sections for admins', () => {
    renderPage();

    expect(screen.getByRole('heading', { name: 'Backup' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Download backup' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Import backup' }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        'Download a backup of your boards, tickets, settings, files, and agent worktrees. Passwords, keys, and saved tokens are left out. Keep the file private.',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Importing replaces all data on this install with the backup's. Passwords, keys, folder paths, agent commands, and plugin and MCP settings aren't imported, so set those up again. You'll paste saved tokens again, and you may be signed out.",
      ),
    ).toBeInTheDocument();
  });

  it('lists connectors on their own page', async () => {
    renderPage('/system/connectors');

    expect(screen.getByRole('heading', { name: 'Connectors' })).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Download backup' }),
    ).not.toBeInTheDocument();
    expect(await screen.findByText(/No connectors/)).toBeVisible();
  });

  it.each([
    ['/tools', '/system/backup'],
    ['/tools?tab=connectors', '/system/connectors'],
    ['/logs?level=warn&q=boom', '/system/logs?level=warn&q=boom'],
  ])('sends the old link %s to %s', (from, to) => {
    renderPage(from);
    expect(screen.getByTestId('location')).toHaveTextContent(to);
  });

  it('asks for a new admin password before replacing keys', () => {
    sessionState.desktopMode = true;
    renderPage();

    expect(MIN_PASSWORD_CHARS).toBe(8);
    expect(screen.getByRole('heading', { name: 'Replace keys' })).toBeInTheDocument();
    expect(
      screen.getByText(
        'Coppice will make new keys and database passwords. Saved tokens keep working, and everyone is signed out.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByLabelText('New admin password')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Replace keys' }));
    expect(screen.getByText('Check the box to continue.')).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole('checkbox', {
        name: "I understand everyone will be signed out, and I'll sign in with the new password.",
      }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Replace keys' }));
    expect(screen.getByText('Enter a new admin password.')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('New admin password'), {
      target: { value: '1234567' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Replace keys' }));
    expect(screen.getByText('Use at least 8 characters.')).toBeInTheDocument();
  });

  it('hides replace keys outside the desktop app', () => {
    renderPage();
    expect(screen.queryByRole('heading', { name: 'Replace keys' })).not.toBeInTheDocument();
    expect(screen.queryByText('coppice-server replace-keys')).not.toBeInTheDocument();
  });

  it('offers setup only in the desktop app', () => {
    renderPage('/system/connectors');
    expect(screen.queryByRole('button', { name: 'Set up workspace' })).not.toBeInTheDocument();
  });

  it('opens setup from the connectors page in the desktop app', async () => {
    sessionState.desktopMode = true;
    fetchMock.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/init' && init?.method === 'POST') {
        return Promise.resolve(new Response('{}', { status: 500 }));
      }
      if (path === '/api/init') {
        return Promise.resolve(
          new Response(
            JSON.stringify({
              presetId: 'starter',
              status: 'ready',
              connectors: [{ id: 'claude-code', displayName: 'Claude Code' }],
              members: [
                { presetKey: 'pm', name: 'PM', role: 'PM' },
                { presetKey: 'tech_lead', name: 'Technical Lead', role: 'Technical Lead' },
                {
                  presetKey: 'backend_engineer',
                  name: 'Backend Engineer',
                  role: 'Backend Engineer',
                },
              ],
            }),
            { status: 200, headers: { 'Content-Type': 'application/json' } },
          ),
        );
      }
      const body = path === '/api/agents' ? { items: [] } : path === '/api/connectors' ? { items: [] } : [];
      return Promise.resolve(
        new Response(JSON.stringify(body), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    });
    renderPage('/system/connectors');

    fireEvent.click(screen.getByRole('button', { name: 'Set up workspace' }));
    expect(await screen.findByRole('heading', { name: 'Set up Coppice' })).toBeInTheDocument();
    expect(screen.queryByText(/mock|fake/i)).not.toBeInTheDocument();
  });

});
