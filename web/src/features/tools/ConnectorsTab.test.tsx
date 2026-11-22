import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setCsrfToken } from '../../lib/api';
import { selectComboboxOption } from '../../test/combobox';
import type { ConnectorStatus } from '../../lib/schemas/connectorDiagnostics';
import { ConnectorsTab } from './ConnectorsTab';
import { connectorStatusesRefetchInterval } from './useConnectorDiagnostics';

const PROBED_AT = '2026-10-03T10:00:00Z';
const CHECK_ID = '00000000-0000-4000-8000-0000000000c1';
const RUN_ID = '00000000-0000-4000-8000-0000000000r1';
const CODEX_AGENT_A = '00000000-0000-4000-8000-0000000000a1';
const CODEX_AGENT_B = '00000000-0000-4000-8000-0000000000a2';
const OPENCODE_AGENT = '00000000-0000-4000-8000-0000000000a3';

function connector(overrides: Partial<ConnectorStatus>): ConnectorStatus {
  const merged: ConnectorStatus = {
    id: 'x',
    displayName: 'X',
    enabled: true,
    cli: { found: true, path: '/usr/local/bin/x', probe: { status: 'ok', detail: 'x 1.0' } },
    auth: { status: 'detected', envSet: [], pathsFound: [] },
    authHint: '',
    docsUrl: '',
    lastRun: null,
    lastCheck: null,
    probedAt: PROBED_AT,
    readiness: 'found',
    ...overrides,
  };
  if (overrides.readiness === undefined && merged.probedAt) {
    merged.readiness = merged.cli.found ? 'found' : 'not_on_path';
  }
  return merged;
}

const kilo = connector({
  id: 'kilo-code',
  displayName: 'Kilo Code',
  enabled: false,
  cli: {
    found: true,
    path: '/home/coppice/.local/bin/kilo',
    probe: { status: 'ok', detail: 'kilo 9.9.9' },
  },
  auth: { status: 'detected', envSet: ['KILO_API_KEY'], pathsFound: ['.kilocode'] },
  authHint: 'Run kilo auth login',
  docsUrl: 'https://kilo.ai/docs',
});

const claude = connector({
  id: 'claude-code',
  displayName: 'Claude Code',
  cli: { found: false, path: null, probe: { status: 'not_run', detail: null } },
  auth: { status: 'not_found', envSet: [], pathsFound: [] },
  authHint: 'Run claude login or set ANTHROPIC_API_KEY',
  docsUrl: 'https://docs.anthropic.com/claude-code',
});

const codex = connector({
  id: 'codex',
  displayName: 'Codex',
  lastRun: {
    runId: '00000000-0000-4000-8000-0000000000f1',
    ticketId: '00000000-0000-4000-8000-0000000000t1',
    status: 'failed',
    finishedAt: PROBED_AT,
    ticketGet: true,
    resultSubmit: false,
  },
});

const opencode = connector({ id: 'opencode', displayName: 'OpenCode' });

const agents = [
  { id: CODEX_AGENT_A, name: 'Codex Dev', connector: 'codex', model: 'gpt-5' },
  { id: CODEX_AGENT_B, name: 'Codex QA', connector: 'codex', model: 'gpt-5-mini' },
  { id: OPENCODE_AGENT, name: 'Open Dev', connector: 'opencode', model: null },
];

const fetchMock = vi.fn();

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function check(status: string, failure: string | null = null) {
  return {
    id: CHECK_ID,
    connector: 'codex',
    agentId: CODEX_AGENT_B,
    status,
    failure,
    runId: RUN_ID,
    createdAt: PROBED_AT,
    finishedAt: status === 'passed' || status === 'failed' ? PROBED_AT : null,
    toolCalls: [],
  };
}

type Handler = (path: string, init: RequestInit) => Response | undefined;

function stubApi(extra: Handler = () => undefined) {
  fetchMock.mockImplementation((path: string, init: RequestInit = {}) => {
    const response = extra(path, init);
    if (response) return Promise.resolve(response);
    const method = init.method ?? 'GET';
    if (path === '/api/tools/connectors' && method === 'GET') {
      return Promise.resolve(json([kilo, claude, codex, opencode]));
    }
    if (path === '/api/agents' && method === 'GET') {
      return Promise.resolve(json({ items: agents }));
    }
    if (method === 'GET' && /^\/api\/connectors\/[^/]+\/model-providers$/.test(path)) {
      return Promise.resolve(json({ items: [] }));
    }
    if (method === 'GET' && /^\/api\/tools\/connectors\/[^/]+\/uncovered-models$/.test(path)) {
      return Promise.resolve(json({ modelIds: [] }));
    }
    return Promise.reject(new Error(`unexpected ${method} ${path}`));
  });
}

function renderTab() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConnectorsTab />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

async function row(id: string) {
  return within(await screen.findByTestId(`connector-row-${id}`));
}

async function openDrawer(id: string) {
  const connectorRow = await screen.findByTestId(`connector-row-${id}`);
  fireEvent.click(within(connectorRow).getByRole('button'));
  return within(await screen.findByLabelText('Connector details'));
}

function callsFor(path: string, method: string) {
  return fetchMock.mock.calls.filter(
    ([p, init]) => p === path && ((init as RequestInit | undefined)?.method ?? 'GET') === method,
  ) as [string, RequestInit][];
}

describe('ConnectorsTab', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
    setCsrfToken('csrf-token');
    stubApi();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('filters connectors by search and chips', async () => {
    renderTab();
    await row('kilo-code');

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search connectors' }), {
      target: { value: 'claude' },
    });
    expect(screen.getByTestId('connector-row-claude-code')).toBeInTheDocument();
    expect(screen.queryByTestId('connector-row-kilo-code')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'All 1' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Needs attention/ }));
    expect(screen.getByTestId('connector-row-claude-code')).toBeInTheDocument();

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search connectors' }), {
      target: { value: 'nothing-like-this' },
    });
    expect(screen.getByText('No connectors match.')).toBeInTheDocument();
  });

  it('shows the oldest CLI version Coppice runs', async () => {
    stubApi((path, init) =>
      path === '/api/tools/connectors' && (init.method ?? 'GET') === 'GET'
        ? json([{ ...kilo, minVersion: '2.0.0' }, claude, codex, opencode])
        : undefined,
    );
    renderTab();
    const kiloDrawer = await openDrawer('kilo-code');
    expect(kiloDrawer.getByText('Needs version 2.0.0 or later')).toBeVisible();
    fireEvent.click(kiloDrawer.getByRole('button', { name: 'Close' }));
    const codexDrawer = await openDrawer('codex');
    expect(codexDrawer.queryByText(/Needs version/)).not.toBeInTheDocument();
  });

  it('polls statuses until startup probes finish', async () => {
    let lists = 0;
    stubApi((path, init) => {
      if (path === '/api/tools/connectors' && (init.method ?? 'GET') === 'GET') {
        lists += 1;
        return json([lists === 1 ? { ...claude, probedAt: null } : claude]);
      }
      return undefined;
    });
    renderTab();

    const claudeRow = await row('claude-code');
    expect(claudeRow.getByText('Checking…')).toBeVisible();
    expect(claudeRow.queryByText('Not on your PATH')).not.toBeInTheDocument();
    const claudeDrawer = await openDrawer('claude-code');
    expect(claudeDrawer.getAllByText('Checking…').length).toBeGreaterThan(0);
    expect(claudeDrawer.queryByText('Not on your PATH')).not.toBeInTheDocument();
    expect(await claudeRow.findByText('Not on your PATH', {}, { timeout: 5000 })).toBeVisible();
    expect(lists).toBe(2);
  }, 10_000);

  it('status refetch interval only while a probe is pending', () => {
    expect(connectorStatusesRefetchInterval(undefined)).toBe(false);
    expect(connectorStatusesRefetchInterval([claude, codex])).toBe(false);
    expect(connectorStatusesRefetchInterval([claude, { ...codex, probedAt: null }])).toBe(2_000);
  });

  it('shows last real run in the drawer', async () => {
    renderTab();

    const codexDrawer = await openDrawer('codex');
    expect(codexDrawer.getByText('ticket_get ✓ · result_submit ✗')).toBeVisible();
    expect(codexDrawer.getByRole('button', { name: 'Open ticket' })).toBeVisible();
    fireEvent.click(codexDrawer.getByRole('button', { name: 'Close' }));
    expect((await openDrawer('opencode')).getByText('No runs yet')).toBeVisible();
  });

  it('run check posts with csrf, refreshes probe or checked time, and surfaces errors', async () => {
    let checkAttempts = 0;
    stubApi((path, init) => {
      if (path === '/api/tools/connectors/codex/check' && init.method === 'POST') {
        checkAttempts += 1;
        if (checkAttempts === 1) {
          return json({
            ...codex,
            cli: { ...codex.cli, probe: { status: 'ok', detail: 'codex 2.0.0' } },
          });
        }
        if (checkAttempts === 2) {
          return json({ ...codex, probedAt: new Date().toISOString() });
        }
        return new Response('', { status: 500 });
      }
      return undefined;
    });
    renderTab();

    const codexDrawer = await openDrawer('codex');
    expect(codexDrawer.queryByText('now')).not.toBeInTheDocument();
    fireEvent.click(codexDrawer.getByRole('button', { name: 'Run check' }));
    expect(await codexDrawer.findByText('codex 2.0.0')).toBeVisible();
    const [, init] = callsFor('/api/tools/connectors/codex/check', 'POST')[0];
    expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('csrf-token');

    fireEvent.click(codexDrawer.getByRole('button', { name: 'Run check' }));
    expect(await codexDrawer.findByText('now')).toBeVisible();

    fireEvent.click(codexDrawer.getByRole('button', { name: 'Run check' }));
    expect(await codexDrawer.findByRole('alert')).toHaveTextContent('Check failed.');
  });

  it('test connection disabled without agents', async () => {
    renderTab();

    const claudeDrawer = await openDrawer('claude-code');
    await waitFor(() =>
      expect(claudeDrawer.getByText(/Create an agent with this connector first/)).toBeVisible(),
    );
    expect(claudeDrawer.getByRole('button', { name: 'Test connection' })).toBeDisabled();
    expect(claudeDrawer.getByRole('link', { name: 'Agents' })).toHaveAttribute('href', '/agents');
  });

  it('test connection polls to passed', async () => {
    let polls = 0;
    stubApi((path, init) => {
      if (path === '/api/tools/connectors/codex/test' && init.method === 'POST') {
        return json({ checkId: CHECK_ID, runId: RUN_ID });
      }
      if (path === `/api/tools/connector-checks/${CHECK_ID}`) {
        polls += 1;
        return json(check(polls === 1 ? 'running' : 'passed'));
      }
      return undefined;
    });
    renderTab();

    const codexDrawer = await openDrawer('codex');
    await waitFor(() =>
      expect(codexDrawer.getByRole('button', { name: 'Test connection' })).toBeEnabled(),
    );
    expect(callsFor('/api/tools/connectors', 'GET')).toHaveLength(1);
    fireEvent.click(codexDrawer.getByRole('button', { name: 'Test connection' }));
    selectComboboxOption(codexDrawer.getByLabelText('Agent'), 'Codex QA · gpt-5-mini');
    fireEvent.click(codexDrawer.getByRole('button', { name: 'Start test' }));
    await waitFor(() =>
      expect(callsFor('/api/tools/connectors', 'GET')).toHaveLength(2),
    );

    await waitFor(() =>
      expect(callsFor('/api/tools/connectors/codex/test', 'POST')).toHaveLength(1),
    );
    const [, init] = callsFor('/api/tools/connectors/codex/test', 'POST')[0];
    expect(JSON.parse(init.body as string)).toEqual({ agentId: CODEX_AGENT_B });
    expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('csrf-token');

    const result = within(await screen.findByTestId('connector-test-result-codex'));
    expect(await result.findByText('Running…')).toBeVisible();
    expect(await result.findByText('Passed', {}, { timeout: 5000 })).toBeVisible();
    await waitFor(() =>
      expect(callsFor('/api/tools/connectors', 'GET').length).toBeGreaterThan(1),
    );
  });

  it('test keeps polling after the drawer closes and a filter hides the row', async () => {
    let polls = 0;
    stubApi((path, init) => {
      if (path === '/api/tools/connectors/codex/test' && init.method === 'POST') {
        return json({ checkId: CHECK_ID, runId: RUN_ID });
      }
      if (path === `/api/tools/connector-checks/${CHECK_ID}`) {
        polls += 1;
        return json(check(polls < 3 ? 'running' : 'passed'));
      }
      return undefined;
    });
    renderTab();

    const codexDrawer = await openDrawer('codex');
    await waitFor(() =>
      expect(codexDrawer.getByRole('button', { name: 'Test connection' })).toBeEnabled(),
    );
    fireEvent.click(codexDrawer.getByRole('button', { name: 'Test connection' }));
    selectComboboxOption(codexDrawer.getByLabelText('Agent'), 'Codex QA · gpt-5-mini');
    fireEvent.click(codexDrawer.getByRole('button', { name: 'Start test' }));
    expect(
      await within(await screen.findByTestId('connector-test-result-codex')).findByText(
        'Running…',
      ),
    ).toBeVisible();
    expect((await row('codex')).getByText('Running…')).toBeVisible();

    fireEvent.click(codexDrawer.getByRole('button', { name: 'Close' }));
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search connectors' }), {
      target: { value: 'claude' },
    });
    expect(screen.queryByTestId('connector-row-codex')).not.toBeInTheDocument();

    await waitFor(
      () =>
        expect(callsFor(`/api/tools/connector-checks/${CHECK_ID}`, 'GET').length).toBeGreaterThan(2),
      { timeout: 5000 },
    );
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search connectors' }), {
      target: { value: '' },
    });
    const codexRow = await row('codex');
    await waitFor(() => expect(codexRow.queryByText('Running…')).not.toBeInTheDocument(), {
      timeout: 5000,
    });
    const reopened = await openDrawer('codex');
    expect(await reopened.findByText('Passed')).toBeVisible();
  }, 10_000);

  it('test connection shows failure reason', async () => {
    stubApi((path, init) => {
      if (path === '/api/tools/connectors/opencode/test' && init.method === 'POST') {
        return json({ checkId: CHECK_ID, runId: RUN_ID });
      }
      if (path === `/api/tools/connector-checks/${CHECK_ID}`) {
        return json({ ...check('failed', 'ticket_get was not called'), connector: 'opencode' });
      }
      return undefined;
    });
    renderTab();

    const openDrawerPanel = await openDrawer('opencode');
    await waitFor(() =>
      expect(openDrawerPanel.getByRole('button', { name: 'Test connection' })).toBeEnabled(),
    );
    fireEvent.click(openDrawerPanel.getByRole('button', { name: 'Test connection' }));
    expect(openDrawerPanel.getByLabelText('Agent')).toHaveTextContent('Open Dev');
    fireEvent.click(openDrawerPanel.getByRole('button', { name: 'Start test' }));

    const result = within(await screen.findByTestId('connector-test-result-opencode'));
    expect(await result.findByText('Failed')).toBeVisible();
    expect(result.getByText('ticket_get was not called')).toBeVisible();
  });

  it('test start errors render inline in the drawer', async () => {
    stubApi((path, init) =>
      path === '/api/tools/connectors/opencode/test' && init.method === 'POST'
        ? new Response('', { status: 409 })
        : undefined,
    );
    renderTab();

    const openDrawerPanel = await openDrawer('opencode');
    await waitFor(() =>
      expect(openDrawerPanel.getByRole('button', { name: 'Test connection' })).toBeEnabled(),
    );
    fireEvent.click(openDrawerPanel.getByRole('button', { name: 'Test connection' }));
    fireEvent.click(openDrawerPanel.getByRole('button', { name: 'Start test' }));

    expect(await openDrawerPanel.findByRole('alert')).toHaveTextContent(
      'A test is already running for this connector.',
    );
  });

  it('toggles connector enabled state with confirm when agents depend on it', async () => {
    stubApi((path, init) => {
      if (path === '/api/tools/connectors/codex' && init.method === 'PATCH') {
        return json({ ...codex, enabled: false });
      }
      if (path === '/api/tools/connectors/claude-code' && init.method === 'PATCH') {
        return json({ ...claude, enabled: false });
      }
      return undefined;
    });
    renderTab();

    const codexRow = await row('codex');
    const toggle = await codexRow.findByRole('switch', { name: 'Enabled Codex' });
    await waitFor(() => expect(toggle).toBeEnabled());
    fireEvent.click(toggle);

    expect(
      await screen.findByText(
        "Turn off Codex? 2 agents use it and can't run tickets until you turn it back on or switch them to another connector.",
      ),
    ).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(callsFor('/api/tools/connectors/codex', 'PATCH')).toHaveLength(0);

    fireEvent.click(toggle);
    fireEvent.click(await screen.findByRole('button', { name: 'Turn off' }));
    await waitFor(() => expect(callsFor('/api/tools/connectors/codex', 'PATCH')).toHaveLength(1));
    expect(JSON.parse(String(callsFor('/api/tools/connectors/codex', 'PATCH')[0][1].body))).toEqual({
      enabled: false,
    });

    const claudeRow = await row('claude-code');
    const claudeToggle = await claudeRow.findByRole('switch', { name: 'Enabled Claude Code' });
    await waitFor(() => expect(claudeToggle).toBeEnabled());
    fireEvent.click(claudeToggle);

    await waitFor(() =>
      expect(callsFor('/api/tools/connectors/claude-code', 'PATCH')).toHaveLength(1),
    );
    expect(screen.queryByRole('button', { name: 'Turn off' })).not.toBeInTheDocument();
  });

  it('checks one model as not covered by the plan', async () => {
    const cursor = connector({ id: 'cursor', displayName: 'Cursor' });
    stubApi((path, init) => {
      const method = init.method ?? 'GET';
      if (path === '/api/tools/connectors' && method === 'GET') {
        return json([cursor]);
      }
      if (path === '/api/connectors/cursor/model-providers' && method === 'GET') {
        return json({ items: [{ id: 'cursor' }] });
      }
      if (path === '/api/connectors/cursor/model-providers/cursor/models' && method === 'GET') {
        return json({
          items: [
            { id: 'composer-2.5', name: 'Composer 2.5' },
            { id: 'auto', name: 'Auto' },
          ],
        });
      }
      if (path === '/api/tools/connectors/cursor/uncovered-models' && method === 'GET') {
        return json({ modelIds: [] });
      }
      if (path === '/api/tools/connectors/cursor/uncovered-models' && method === 'PUT') {
        return json({ modelIds: ['composer-2.5'] });
      }
      return undefined;
    });
    renderTab();

    const cursorDrawer = await openDrawer('cursor');
    const disclosure = cursorDrawer.getByRole('button', { name: 'Models not covered by my plan' });
    expect(disclosure).toHaveAttribute('aria-expanded', 'false');
    expect(
      cursorDrawer.getByText('Runs on these models show Billed outside your plan.'),
    ).toBeVisible();
    expect(cursorDrawer.queryByRole('checkbox')).not.toBeInTheDocument();

    fireEvent.click(disclosure);
    const composer = await cursorDrawer.findByRole('checkbox', { name: /Composer 2.5/ });
    expect(composer).not.toBeChecked();
    expect(cursorDrawer.getByRole('checkbox', { name: /^Auto/ })).not.toBeChecked();

    fireEvent.click(composer);

    await waitFor(() =>
      expect(callsFor('/api/tools/connectors/cursor/uncovered-models', 'PUT')).toHaveLength(1),
    );
    expect(
      JSON.parse(
        String(callsFor('/api/tools/connectors/cursor/uncovered-models', 'PUT')[0][1].body),
      ),
    ).toEqual({ modelIds: ['composer-2.5'] });
    expect(composer).toBeChecked();
  });

  it('offers Refresh models when the live list fails', async () => {
    const cursor = connector({ id: 'cursor', displayName: 'Cursor' });
    stubApi((path, init) => {
      const method = init.method ?? 'GET';
      if (path === '/api/tools/connectors' && method === 'GET') {
        return json([cursor]);
      }
      if (
        (path === '/api/connectors/cursor/model-providers' ||
          path === '/api/connectors/cursor/model-providers?refresh=true') &&
        method === 'GET'
      ) {
        return json({ items: [{ id: 'cursor' }] });
      }
      if (path === '/api/connectors/cursor/model-providers/cursor/models?refresh=true' && method === 'GET') {
        return json({
          items: [{ id: 'composer-2.5', name: 'Composer 2.5' }],
        });
      }
      if (path === '/api/connectors/cursor/model-providers/cursor/models' && method === 'GET') {
        return json({ message: 'unavailable' }, 500);
      }
      if (path === '/api/tools/connectors/cursor/uncovered-models' && method === 'GET') {
        return json({ modelIds: [] });
      }
      return undefined;
    });
    renderTab();

    const cursorDrawer = await openDrawer('cursor');
    expect(
      await cursorDrawer.findByText(
        "Couldn't get Cursor's models. Click Refresh models to try again.",
      ),
    ).toBeVisible();
    fireEvent.click(cursorDrawer.getByRole('button', { name: 'Refresh models' }));

    await waitFor(() =>
      expect(
        fetchMock.mock.calls.some(
          ([path]) => path === '/api/connectors/cursor/model-providers/cursor/models?refresh=true',
        ),
      ).toBe(true),
    );
  });

});
