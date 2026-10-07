import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../lib/api';
import { SettingsPage } from './SettingsPage';

const mocks = vi.hoisted(() => ({
  apiFetch: vi.fn(),
}));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../auth/useSession', () => ({
  useSession: () => ({ user: { role: 'admin' }, loading: false }),
}));

vi.mock('./TomlEditor', () => ({
  TomlEditor: ({
    value,
    onChange,
  }: {
    value: string;
    onChange: (value: string) => void;
    errorLine: number | null;
  }) => (
    <textarea
      aria-label="config.toml"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

const disk = {
  path: '/tmp/coppice/config.toml',
  text: '# keep me\n[server]\nport = 5000\n',
  revision: 'rev-1',
  backupAvailable: true,
};

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
    text: async () => JSON.stringify(body),
  };
}

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <SettingsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return { client, ...view };
}

describe('SettingsPage', () => {
  beforeEach(() => {
    mocks.apiFetch.mockReset();
    mocks.apiFetch.mockImplementation((path: string) => {
      if (path === '/api/settings/config') {
        return jsonResponse(disk);
      }
      throw new Error(`unexpected ${path}`);
    });
  });

  it('shows the config file behind the Settings title', async () => {
    renderPage();

    expect(screen.getByRole('heading', { name: 'Settings' })).toBeInTheDocument();
    expect(screen.getByText(/This is your/)).toHaveTextContent(
      'This is your config.toml. Comments are kept.',
    );
    expect(screen.getByText('config.toml', { selector: 'code' })).toBeInTheDocument();
    expect(await screen.findByRole('textbox', { name: 'config.toml' })).toHaveValue(disk.text);
    expect(screen.getByTestId('config-path')).toHaveTextContent('/tmp/coppice/config.toml');
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Restore last good version' })).toBeEnabled();
  });

  it('shows the parser line and leaves the editor unchanged on an invalid save', async () => {
    mocks.apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/settings/config' && init?.method === 'PUT') {
        throw new ApiError(
          400,
          JSON.stringify({
            line: 3,
            column: 8,
            message: 'invalid type: found string "nope", expected u16',
          }),
        );
      }
      return jsonResponse(disk);
    });
    renderPage();

    const editor = await screen.findByRole('textbox', { name: 'config.toml' });
    fireEvent.change(editor, { target: { value: '# keep me\n[server]\nport = "nope"\n' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(
      await screen.findByRole('alert'),
    ).toHaveTextContent(
      'Not saved. Line 3: invalid type: found string "nope", expected u16. Your file is unchanged.',
    );
    expect(editor).toHaveValue('# keep me\n[server]\nport = "nope"\n');
    expect(mocks.apiFetch).not.toHaveBeenCalledWith(
      '/api/settings/config/backup',
      expect.anything(),
    );
  });

  it('says Saved when the file applies without a restart', async () => {
    mocks.apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/settings/config' && init?.method === 'PUT') {
        return jsonResponse({
          revision: 'rev-2',
          restartRequired: false,
          backupAvailable: true,
        });
      }
      return jsonResponse(disk);
    });
    renderPage();
    await screen.findByRole('textbox', { name: 'config.toml' });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('Saved.');
    expect(screen.getByRole('alert')).not.toHaveTextContent('Restart');
  });

  it('says to restart when a saved key needs it', async () => {
    mocks.apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/settings/config' && init?.method === 'PUT') {
        return jsonResponse({
          revision: 'rev-2',
          restartRequired: true,
          backupAvailable: true,
        });
      }
      return jsonResponse(disk);
    });
    renderPage();
    await screen.findByRole('textbox', { name: 'config.toml' });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Saved. Restart Coppice to apply.',
    );
  });

  it('asks to reload or keep edits when the file changes outside the editor', async () => {
    const { client } = renderPage();
    await screen.findByRole('textbox', { name: 'config.toml' });

    mocks.apiFetch.mockImplementation((path: string) => {
      if (path === '/api/settings/config') {
        return jsonResponse({
          ...disk,
          text: '# from disk\n',
          revision: 'rev-disk',
        });
      }
      throw new Error(`unexpected ${path}`);
    });
    await client.refetchQueries({ queryKey: ['settings-config'] });

    expect(await screen.findByRole('status')).toHaveTextContent(
      'config.toml changed outside this editor.',
    );
    expect(screen.getByRole('button', { name: 'Reload' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Keep my edits' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: 'Reload' }));
    expect(screen.getByRole('textbox', { name: 'config.toml' })).toHaveValue('# from disk\n');
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('loads the last good version into the editor without saving', async () => {
    mocks.apiFetch.mockImplementation((path: string) => {
      if (path === '/api/settings/config/backup') {
        return jsonResponse({ text: '# last good\n[server]\nport = 1\n' });
      }
      if (path === '/api/settings/config') {
        return jsonResponse(disk);
      }
      throw new Error(`unexpected ${path}`);
    });
    renderPage();
    await screen.findByRole('textbox', { name: 'config.toml' });
    fireEvent.click(screen.getByRole('button', { name: 'Restore last good version' }));

    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: 'config.toml' })).toHaveValue(
        '# last good\n[server]\nport = 1\n',
      );
    });
    expect(mocks.apiFetch).not.toHaveBeenCalledWith(
      '/api/settings/config',
      expect.objectContaining({ method: 'PUT' }),
    );
  });
});
