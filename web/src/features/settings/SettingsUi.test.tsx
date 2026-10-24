import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
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
  TomlEditor: ({ value }: { value: string }) => (
    <textarea aria-label="config.toml" value={value} readOnly />
  ),
}));

const disk = {
  path: '/tmp/coppice/config.toml',
  text: '[logs]\nretention_days = 7\n',
  revision: 'rev-1',
  backupAvailable: false,
};

function setting(overrides: Record<string, unknown>) {
  return {
    group: 'Logs',
    description: 'A setting.',
    defaultLabel: null,
    restart: false,
    value: null,
    ...overrides,
  };
}

const ui = {
  path: disk.path,
  revision: 'rev-1',
  backupAvailable: false,
  fileError: null,
  settings: [
    setting({
      key: 'logs.retention_days',
      path: ['logs', 'retention_days'],
      description: 'How many days of logs to keep.',
      kind: { type: 'integerChoice', options: [3, 7, 14, 30] },
      default: 14,
      value: 7,
    }),
    setting({
      key: 'git.push_enabled',
      path: ['git', 'push_enabled'],
      group: 'Git',
      description: 'Lets Coppice push branches.',
      kind: { type: 'bool' },
      default: false,
    }),
    setting({
      key: 'agent.worker_count',
      path: ['agent', 'worker_count'],
      group: 'Agent',
      description: 'Total cap on agent jobs.',
      kind: { type: 'integer', min: 1, max: null },
      default: 8,
    }),
    setting({
      key: 'agent.connectors.opencode.enabled',
      path: ['agent', 'connectors', 'opencode', 'enabled'],
      group: 'Connectors · OpenCode',
      description: 'Turns OpenCode on or off.',
      kind: { type: 'optionalBool', unsetLabel: 'Automatic' },
      default: null,
      defaultLabel: 'Automatic',
    }),
    setting({
      key: 'keybindings.palette.open',
      path: ['keybindings', 'palette.open'],
      group: 'Keyboard shortcuts',
      description: 'palette.open',
      kind: { type: 'shortcut' },
      default: ['Mod+K'],
    }),
  ],
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
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <SettingsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function puts() {
  return mocks.apiFetch.mock.calls
    .filter(([path]) => path === '/api/settings/config/value')
    .map(([, init]) => JSON.parse((init as RequestInit).body as string));
}

describe('Settings UI mode', () => {
  beforeEach(() => {
    localStorage.clear();
    mocks.apiFetch.mockReset();
    let revision = 'rev-1';
    mocks.apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/settings/config') return jsonResponse(disk);
      if (path === '/api/settings/config/ui') return jsonResponse({ ...ui, revision });
      if (path === '/api/keybindings') return jsonResponse({ overrides: {}, warnings: [] });
      if (path === '/api/settings/config/value' && init?.method === 'PUT') {
        revision = 'rev-2';
        return jsonResponse({
          revision,
          restartRequired: false,
          backupAvailable: true,
        });
      }
      throw new Error(`unexpected ${path}`);
    });
  });

  afterEach(() => {
    localStorage.clear();
  });

  it('opens in UI mode and shows defaults for keys the file does not set', async () => {
    renderPage();

    const push = await screen.findByTestId('setting-git.push_enabled');
    expect(within(push).getByRole('switch')).not.toBeChecked();
    expect(within(push).queryByText('Modified')).not.toBeInTheDocument();
    expect(within(push).queryByRole('button', { name: /Reset/ })).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId('setting-agent.worker_count')).getByRole('spinbutton'),
    ).toHaveValue(8);
    expect(screen.getByRole('switch', { name: 'Edit as TOML' })).not.toBeChecked();
    expect(screen.queryByRole('textbox', { name: 'config.toml' })).not.toBeInTheDocument();
    expect(screen.getByTestId('config-path')).toHaveTextContent(disk.path);
  });

  it('marks a key the file sets and resets it to the default', async () => {
    renderPage();

    const retention = await screen.findByTestId('setting-logs.retention_days');
    expect(within(retention).getByText('Modified')).toBeVisible();
    expect(within(retention).getByText('Default: 14')).toBeVisible();
    fireEvent.click(
      within(retention).getByRole('button', {
        name: 'Reset logs.retention_days',
      }),
    );

    await waitFor(() =>
      expect(puts()).toEqual([
        {
          path: ['logs', 'retention_days'],
          value: null,
          baseRevision: 'rev-1',
        },
      ]),
    );
  });

  it('writes a default-valued setting when you change it', async () => {
    renderPage();

    const push = await screen.findByTestId('setting-git.push_enabled');
    fireEvent.click(within(push).getByRole('switch'));
    await waitFor(() =>
      expect(puts()).toEqual([
        { path: ['git', 'push_enabled'], value: true, baseRevision: 'rev-1' },
      ]),
    );
    expect(await within(push).findByText('Saved.')).toBeVisible();

    const workers = within(screen.getByTestId('setting-agent.worker_count')).getByRole(
      'spinbutton',
    );
    fireEvent.change(workers, { target: { value: '8' } });
    fireEvent.keyDown(workers, { key: 'Enter' });
    expect(puts()).toHaveLength(1);
    fireEvent.change(workers, { target: { value: '4' } });
    fireEvent.keyDown(workers, { key: 'Enter' });
    await waitFor(() =>
      expect(puts()[1]).toEqual({
        path: ['agent', 'worker_count'],
        value: 4,
        baseRevision: 'rev-2',
      }),
    );
  });

  it('names shortcuts and saves several keys as a list', async () => {
    renderPage();

    const row = await screen.findByTestId('setting-keybindings.palette.open');
    expect(within(row).getByText('Open command palette')).toBeVisible();
    const input = within(row).getByRole('textbox');
    expect(input).toHaveValue('Mod+K');
    fireEvent.change(input, { target: { value: 'Mod+P or Mod+K' } });
    fireEvent.blur(input);

    await waitFor(() =>
      expect(puts()).toEqual([
        {
          path: ['keybindings', 'palette.open'],
          value: ['Mod+P', 'Mod+K'],
          baseRevision: 'rev-1',
        },
      ]),
    );
  });

  it('filters settings by search', async () => {
    renderPage();

    await screen.findByTestId('setting-git.push_enabled');
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search settings' }), {
      target: { value: 'opencode' },
    });
    expect(screen.getByTestId('setting-agent.connectors.opencode.enabled')).toBeVisible();
    expect(screen.queryByTestId('setting-git.push_enabled')).not.toBeInTheDocument();
  });

  it('shows why a value was refused', async () => {
    mocks.apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/settings/config') return jsonResponse(disk);
      if (path === '/api/settings/config/ui') return jsonResponse(ui);
      if (path === '/api/settings/config/value' && init?.method === 'PUT') {
        return Promise.reject(
          new ApiError(
            400,
            JSON.stringify({
              message: 'Set git.push_enabled to true or false.',
            }),
          ),
        );
      }
      throw new Error(`unexpected ${path}`);
    });
    renderPage();

    const push = await screen.findByTestId('setting-git.push_enabled');
    fireEvent.click(within(push).getByRole('switch'));
    expect(await within(push).findByRole('alert')).toHaveTextContent(
      'Set git.push_enabled to true or false.',
    );
  });

  it('remembers TOML mode and shows the file there', async () => {
    renderPage();

    fireEvent.click(await screen.findByRole('switch', { name: 'Edit as TOML' }));
    expect(await screen.findByRole('textbox', { name: 'config.toml' })).toHaveValue(disk.text);
    expect(localStorage.getItem('coppice.settings.mode')).toBe('toml');
  });

  it('points to TOML mode when the file cannot be read', async () => {
    mocks.apiFetch.mockImplementation((path: string) => {
      if (path === '/api/settings/config') return jsonResponse(disk);
      if (path === '/api/settings/config/ui') {
        return jsonResponse({ ...ui, fileError: 'unclosed table' });
      }
      throw new Error(`unexpected ${path}`);
    });
    renderPage();

    expect(await screen.findByRole('alert')).toHaveTextContent(
      "config.toml can't be read: unclosed table. Turn on Edit as TOML to fix it.",
    );
    expect(
      within(screen.getByTestId('setting-git.push_enabled')).getByRole('switch'),
    ).toBeDisabled();
  });
});
