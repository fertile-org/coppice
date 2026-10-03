import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ToolsPage } from './ToolsPage';

vi.mock('../auth/useSession', () => ({
  useSession: () => ({
    user: { id: '1', email: 'admin@localhost', role: 'admin' },
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

  it('opens the connectors tab from the url', () => {
    renderPage('/tools?tab=connectors');

    expect(screen.getByRole('tab', { name: 'Connectors' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
  });
});
