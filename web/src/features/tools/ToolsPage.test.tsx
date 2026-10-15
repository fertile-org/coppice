import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MIN_PASSWORD_CHARS } from '../../lib/passwords';
import { ToolsPage } from './ToolsPage';

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

function renderPage(initialEntry = '/tools') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[initialEntry]}>
        <ToolsPage />
        <LocationProbe />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const fetchMock = vi.fn();

describe('ToolsPage', () => {
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

    expect(screen.getByRole('heading', { name: 'Tools' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Download backup' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Import backup' }),
    ).toBeInTheDocument();
  });

  it('tools page has backup and connectors tabs', async () => {
    renderPage();

    const backupTab = screen.getByRole('tab', { name: 'Backup' });
    const connectorsTab = screen.getByRole('tab', { name: 'Connectors' });
    expect(backupTab).toHaveAttribute('aria-selected', 'true');
    expect(connectorsTab).toHaveAttribute('aria-selected', 'false');
    const backupPanel = screen.getByRole('tabpanel', { name: 'Backup' });
    expect(backupTab).toHaveAttribute('aria-controls', backupPanel.id);

    fireEvent.click(connectorsTab);

    expect(screen.getByTestId('location')).toHaveTextContent('/tools?tab=connectors');
    expect(connectorsTab).toHaveAttribute('aria-selected', 'true');
    const connectorsPanel = screen.getByRole('tabpanel', { name: 'Connectors' });
    expect(connectorsTab).toHaveAttribute('aria-controls', connectorsPanel.id);
    expect(
      screen.queryByRole('button', { name: 'Download backup' }),
    ).not.toBeInTheDocument();
    expect(await screen.findByText(/No connectors/)).toBeVisible();
  });

  it('asks for a new admin password before replacing keys', () => {
    sessionState.desktopMode = true;
    renderPage();

    expect(MIN_PASSWORD_CHARS).toBe(1);
    expect(screen.getByRole('heading', { name: 'Replace keys' })).toBeInTheDocument();
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
  });

  it('hides replace keys outside the desktop app', () => {
    renderPage();
    expect(screen.queryByRole('heading', { name: 'Replace keys' })).not.toBeInTheDocument();
  });

  it('opens the connectors tab from the url', () => {
    renderPage('/tools?tab=connectors');

    expect(screen.getByRole('tab', { name: 'Connectors' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
  });
});
