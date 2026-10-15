import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../lib/api';
import { savedExport } from './copy';
import { LogsPage } from './LogsPage';
import { reportProblemUrl } from './query';

const session = vi.hoisted(() => ({ role: 'admin' }));

const mocks = vi.hoisted(() => ({
  apiFetch: vi.fn(),
}));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../auth/useSession', () => ({
  useSession: () => ({ user: { role: session.role }, loading: false }),
}));

const httpLine = {
  timestamp: '2026-10-14T03:35:01Z',
  level: 'INFO',
  source: 'http',
  target: 'coppice_server::middleware::request_log',
  message: 'http request',
  runId: null,
  ticketId: null,
  stream: null,
  fields: { method: 'GET', path: '/api/health', status: 200, duration_ms: 4 },
};

const metaBody = {
  retentionDays: 14,
  version: '0.1.0',
  commit: 'abc',
  os: 'linux',
  arch: 'x64',
  osVersion: 'Debian GNU/Linux 12',
};

function jsonResponse(body: unknown) {
  return {
    ok: true,
    status: 200,
    json: async () => body,
    text: async () => JSON.stringify(body),
  };
}

function apiResult(path: string, body: unknown) {
  if (String(path).startsWith('/api/logs/meta')) {
    return jsonResponse(metaBody);
  }
  return jsonResponse(body);
}

class SocketStub {
  static latest: SocketStub | null = null;
  url: string;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  closed = false;

  constructor(url: string) {
    this.url = url;
    SocketStub.latest = this;
  }

  send(data: string) {
    this.sent.push(data);
  }

  close() {
    this.closed = true;
  }
}

function renderPage(path = '/logs') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/logs" element={<LogsPage />} />
          <Route path="/boards" element={<p>Boards</p>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('LogsPage', () => {
  beforeEach(() => {
    session.role = 'admin';
    SocketStub.latest = null;
    vi.stubGlobal('WebSocket', SocketStub);
    delete window.coppiceDesktop;
    mocks.apiFetch.mockReset();
    mocks.apiFetch.mockImplementation((path: string) =>
      apiResult(path, { lines: [], nextCursor: null }),
    );
  });

  it('shows the empty state', async () => {
    renderPage();
    expect(await screen.findByText('No logs match these filters.')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Logs' })).toBeInTheDocument();
    expect(screen.getByLabelText('Source')).toBeInTheDocument();
    expect(screen.getByLabelText('Level')).toHaveValue('info');
    expect(screen.getByRole('checkbox', { name: 'Live' })).not.toBeChecked();
  });

  it('shows a request line and keeps the run from the address', async () => {
    mocks.apiFetch.mockImplementation((path: string) =>
      apiResult(path, { lines: [httpLine], nextCursor: null }),
    );
    renderPage('/logs?run=run-9');
    expect(await screen.findByText('http request')).toBeInTheDocument();
    expect(screen.getByText('GET /api/health 200')).toBeInTheDocument();
    expect(mocks.apiFetch).toHaveBeenCalledWith(expect.stringContaining('run=run-9'));
  });

  it('asks for agent lines when the source changes', async () => {
    renderPage();
    await screen.findByText('No logs match these filters.');
    fireEvent.change(screen.getByLabelText('Source'), { target: { value: 'cli' } });
    await waitFor(() => {
      expect(mocks.apiFetch).toHaveBeenCalledWith(expect.stringContaining('source=cli'));
    });
  });

  it('sends the live filter when Live is on', async () => {
    renderPage('/logs?run=run-9');
    await screen.findByText('No logs match these filters.');
    fireEvent.click(screen.getByRole('checkbox', { name: 'Live' }));
    await waitFor(() => expect(SocketStub.latest).not.toBeNull());
    SocketStub.latest?.onopen?.();
    expect(SocketStub.latest?.sent[0]).toContain('"run":"run-9"');
    await act(async () => {
      SocketStub.latest?.onmessage?.({
        data: JSON.stringify({
          ...httpLine,
          source: 'cli',
          message: 'arrived later',
          stream: 'stdout',
          runId: 'run-9',
          fields: {},
        }),
      });
    });
    expect(await screen.findByText('arrived later')).toBeInTheDocument();
  });

  it('labels the warn level Warning', async () => {
    renderPage();
    await screen.findByText('No logs match these filters.');
    expect(screen.getByRole('option', { name: 'Warning' })).toHaveValue('warn');
  });

  it('shows the server sentence when the log store is missing', async () => {
    mocks.apiFetch.mockImplementation(() => {
      throw new ApiError(
        503,
        JSON.stringify({ message: "Logs aren't available on this server." }),
      );
    });
    renderPage();
    expect(await screen.findByText("Logs aren't available on this server.")).toBeInTheDocument();
    expect(screen.queryByText("Couldn't load logs. Try again.")).not.toBeInTheDocument();
  });

  it('shows the search sentence for a bad query', async () => {
    mocks.apiFetch.mockImplementation(() => {
      throw new ApiError(
        400,
        JSON.stringify({ message: "Couldn't read that search. Check it and try again." }),
      );
    });
    renderPage();
    expect(
      await screen.findByText("Couldn't read that search. Check it and try again."),
    ).toBeInTheDocument();
  });

  it('shows the retry sentence when the request fails without a server sentence', async () => {
    mocks.apiFetch.mockImplementation(() => {
      throw new Error('network');
    });
    renderPage();
    expect(await screen.findByText("Couldn't load logs. Try again.")).toBeInTheDocument();
  });

  it('exports the current filters and says the file was saved', async () => {
    window.coppiceDesktop = {
      pickDirectory: async () => null,
      appInfo: async () => ({ version: '1.4.2', platform: 'darwin', arch: 'arm64' }),
    };
    URL.createObjectURL = () => 'blob:logs';
    URL.revokeObjectURL = () => {};
    HTMLAnchorElement.prototype.click = () => {};
    mocks.apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (String(path).startsWith('/api/logs/export')) {
        expect(init?.method).toBe('POST');
        expect(String(init?.body)).toContain('"source":"cli"');
        expect(String(init?.body)).toContain('"shellVersion":"1.4.2"');
        return {
          ok: true,
          status: 200,
          headers: {
            get: (name: string) =>
              name === 'Content-Disposition' ? 'attachment; filename="coppice-logs.zip"' : null,
          },
          blob: async () => new Blob(['zip']),
          json: async () => ({}),
          text: async () => '',
        };
      }
      return apiResult(path, { lines: [], nextCursor: null });
    });
    renderPage('/logs?source=cli');
    await screen.findByText('No logs match these filters.');
    fireEvent.click(screen.getByRole('button', { name: 'Export logs' }));
    expect(await screen.findByText(savedExport('coppice-logs.zip'))).toBeInTheDocument();
  });

  it('opens a GitHub issue for the report link', async () => {
    renderPage();
    const link = await screen.findByRole('link', { name: 'Report a problem' });
    await waitFor(() => {
      expect(link).toHaveAttribute('href', reportProblemUrl('0.1.0', 'Debian GNU/Linux 12'));
    });
    expect(link).toHaveAttribute('target', '_blank');
  });

  it('shows the export sentence when the download fails', async () => {
    mocks.apiFetch.mockImplementation((path: string) => {
      if (String(path).startsWith('/api/logs/export')) {
        throw new Error('network');
      }
      return apiResult(path, { lines: [], nextCursor: null });
    });
    renderPage();
    await screen.findByText('No logs match these filters.');
    fireEvent.click(screen.getByRole('button', { name: 'Export logs' }));
    expect(await screen.findByText("Couldn't export logs. Try again.")).toBeInTheDocument();
  });

  it('sends a member back to the boards', async () => {
    session.role = 'member';
    renderPage();
    expect(await screen.findByText('Boards')).toBeInTheDocument();
    expect(mocks.apiFetch).not.toHaveBeenCalled();
  });
});
