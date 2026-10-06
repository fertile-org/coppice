import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setCsrfToken } from '../../lib/api';
import { selectComboboxOption } from '../../test/combobox';
import {
  connectorStatusSchema,
  type ConnectorStatus,
} from '../../lib/schemas/connectorDiagnostics';
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

async function card(id: string) {
  return within(await screen.findByTestId(`connector-card-${id}`));
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

  it('shows status per connector', async () => {
    renderTab();

    const kiloCard = await card('kilo-code');
    expect(kiloCard.getByText('/home/coppice/.local/bin/kilo')).toBeVisible();
    expect(kiloCard.getByText('kilo 9.9.9')).toBeVisible();
    expect(kiloCard.getByText(/KILO_API_KEY/)).toBeVisible();
    expect(kiloCard.getByText(/\.kilocode/)).toBeVisible();
    expect(kiloCard.queryByText(/restart the server/)).not.toBeInTheDocument();
    expect(kiloCard.getByRole('switch', { name: 'Enabled Kilo Code' })).toHaveAttribute(
      'aria-checked',
      'false',
    );
    expect(kiloCard.getByRole('button', { name: 'Test connection' })).toBeDisabled();

    const claudeCard = await card('claude-code');
    expect(claudeCard.getByText('Not on your PATH')).toBeVisible();
    expect(
      claudeCard.getByText('Run claude login or set ANTHROPIC_API_KEY'),
    ).toBeVisible();
    expect(claudeCard.getByRole('link', { name: /install docs/i })).toHaveAttribute(
      'href',
      'https://docs.anthropic.com/claude-code',
    );
  });

  it('shows checking state before the first probe', async () => {
    fetchMock.mockReset();
    stubApi((path, init) =>
      path === '/api/tools/connectors' && (init.method ?? 'GET') === 'GET'
        ? json([{ ...claude, probedAt: null }])
        : undefined,
    );
    renderTab();

    const claudeCard = await card('claude-code');
    expect(claudeCard.getAllByText('Checking…').length).toBeGreaterThan(0);
    expect(claudeCard.queryByText('Not on your PATH')).not.toBeInTheDocument();
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

    const claudeCard = await card('claude-code');
    expect(claudeCard.getAllByText('Checking…').length).toBeGreaterThan(0);
    expect(await claudeCard.findByText('Not on your PATH', {}, { timeout: 5000 })).toBeVisible();
    expect(lists).toBe(2);
  }, 10_000);

  it('status refetch interval only while a probe is pending', () => {
    expect(connectorStatusesRefetchInterval(undefined)).toBe(false);
    expect(connectorStatusesRefetchInterval([claude, codex])).toBe(false);
    expect(connectorStatusesRefetchInterval([claude, { ...codex, probedAt: null }])).toBe(2_000);
  });

  it('shows last real run', async () => {
    renderTab();

    const codexCard = await card('codex');
    expect(codexCard.getByText('ticket_get ✓ · result_submit ✗')).toBeVisible();
    expect(codexCard.getByRole('button', { name: 'Open ticket' })).toBeVisible();
    expect((await card('opencode')).getByText('No runs yet')).toBeVisible();
  });

  it('run check posts with csrf and refreshes card', async () => {
    stubApi((path, init) =>
      path === '/api/tools/connectors/codex/check' && init.method === 'POST'
        ? json({ ...codex, cli: { ...codex.cli, probe: { status: 'ok', detail: 'codex 2.0.0' } } })
        : undefined,
    );
    renderTab();

    const codexCard = await card('codex');
    fireEvent.click(codexCard.getByRole('button', { name: 'Run check' }));

    expect(await codexCard.findByText('codex 2.0.0')).toBeVisible();
    const [, init] = callsFor('/api/tools/connectors/codex/check', 'POST')[0];
    expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('csrf-token');
  });

  it('run check with an unchanged result updates the checked time', async () => {
    stubApi((path, init) =>
      path === '/api/tools/connectors/codex/check' && init.method === 'POST'
        ? json({ ...codex, probedAt: new Date().toISOString() })
        : undefined,
    );
    renderTab();

    const codexCard = await card('codex');
    expect(codexCard.queryByText('now')).not.toBeInTheDocument();
    fireEvent.click(codexCard.getByRole('button', { name: 'Run check' }));

    expect(await codexCard.findByText('now')).toBeVisible();
    expect(codexCard.getByText('Checked')).toBeVisible();
  });

  it('run check failure shows the error on the card', async () => {
    stubApi((path, init) =>
      path === '/api/tools/connectors/codex/check' && init.method === 'POST'
        ? new Response('', { status: 500 })
        : undefined,
    );
    renderTab();

    const codexCard = await card('codex');
    fireEvent.click(codexCard.getByRole('button', { name: 'Run check' }));

    expect(await codexCard.findByRole('alert')).toHaveTextContent('Check failed.');
  });

  it('test connection disabled without agents', async () => {
    renderTab();

    const claudeCard = await card('claude-code');
    await waitFor(() =>
      expect(claudeCard.getByText(/Create an agent with this connector first/)).toBeVisible(),
    );
    expect(claudeCard.getByRole('button', { name: 'Test connection' })).toBeDisabled();
    expect(claudeCard.getByRole('link', { name: 'Agents' })).toHaveAttribute('href', '/agents');
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

    const codexCard = await card('codex');
    await waitFor(() =>
      expect(codexCard.getByRole('button', { name: 'Test connection' })).toBeEnabled(),
    );
    fireEvent.click(codexCard.getByRole('button', { name: 'Test connection' }));
    selectComboboxOption(codexCard.getByLabelText('Agent'), 'Codex QA · gpt-5-mini');
    fireEvent.click(codexCard.getByRole('button', { name: 'Start test' }));

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

  it('test connection start refreshes statuses', async () => {
    stubApi((path, init) => {
      if (path === '/api/tools/connectors/codex/test' && init.method === 'POST') {
        return json({ checkId: CHECK_ID, runId: RUN_ID });
      }
      if (path === `/api/tools/connector-checks/${CHECK_ID}`) {
        return json(check('running'));
      }
      return undefined;
    });
    renderTab();

    const codexCard = await card('codex');
    await waitFor(() =>
      expect(codexCard.getByRole('button', { name: 'Test connection' })).toBeEnabled(),
    );
    expect(callsFor('/api/tools/connectors', 'GET')).toHaveLength(1);
    fireEvent.click(codexCard.getByRole('button', { name: 'Test connection' }));
    selectComboboxOption(codexCard.getByLabelText('Agent'), 'Codex QA · gpt-5-mini');
    fireEvent.click(codexCard.getByRole('button', { name: 'Start test' }));

    await waitFor(() =>
      expect(callsFor('/api/tools/connectors', 'GET')).toHaveLength(2),
    );
    const result = within(await screen.findByTestId('connector-test-result-codex'));
    expect(await result.findByText('Running…')).toBeVisible();
  });

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

    const openCard = await card('opencode');
    await waitFor(() =>
      expect(openCard.getByRole('button', { name: 'Test connection' })).toBeEnabled(),
    );
    fireEvent.click(openCard.getByRole('button', { name: 'Test connection' }));
    expect(openCard.getByLabelText('Agent')).toHaveTextContent('Open Dev');
    fireEvent.click(openCard.getByRole('button', { name: 'Start test' }));

    const result = within(await screen.findByTestId('connector-test-result-opencode'));
    expect(await result.findByText('Failed')).toBeVisible();
    expect(result.getByText('ticket_get was not called')).toBeVisible();
  });

  it('test connection start error renders inline', async () => {
    stubApi((path, init) =>
      path === '/api/tools/connectors/opencode/test' && init.method === 'POST'
        ? new Response('', { status: 409 })
        : undefined,
    );
    renderTab();

    const openCard = await card('opencode');
    await waitFor(() =>
      expect(openCard.getByRole('button', { name: 'Test connection' })).toBeEnabled(),
    );
    fireEvent.click(openCard.getByRole('button', { name: 'Test connection' }));
    fireEvent.click(openCard.getByRole('button', { name: 'Start test' }));

    expect(await openCard.findByRole('alert')).toHaveTextContent(
      'A test is already running for this connector.',
    );
  });

  it('confirms before turning off a connector that agents use', async () => {
    stubApi((path, init) => {
      if (path === '/api/tools/connectors/codex' && init.method === 'PATCH') {
        return json({ ...codex, enabled: false });
      }
      return undefined;
    });
    renderTab();

    const codexCard = await card('codex');
    const toggle = await codexCard.findByRole('switch', { name: 'Enabled Codex' });
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
  });

  it('unknown statuses fall back', () => {
    const parsed = connectorStatusSchema.parse({
      ...claude,
      cli: { found: false, probe: { status: 'weird' } },
      auth: { status: 'maybe', envSet: [], pathsFound: [] },
    });
    expect(parsed.cli.probe.status).toBe('not_run');
    expect(parsed.auth.status).toBe('not_found');
  });
});
