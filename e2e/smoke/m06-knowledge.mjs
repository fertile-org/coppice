#!/usr/bin/env node
/**
 * M06 governed knowledge API + web route smoke test.
 *
 * Proves the default stack can govern a manual candidate, compact a Done
 * ticket into a Pending candidate with the configured compaction agent
 * (MockProvider fixtures, no notification on success), and retrieve both
 * approved revisions through the `knowledge_search` MCP tool in a Full run with
 * an exact audit. Knowledge is no longer injected into the run context: the
 * worker agent's mock fixture (`m06-knowledge-search-worker/work_on_ticket.json`)
 * calls `knowledge_search` through the gateway, which logs usage. Restores the
 * previous compaction agent setting when done.
 *
 * Env:
 *   COPPICE_API_URL            default http://localhost:5000
 *   COPPICE_WEB_URL            default http://localhost:5001
 *   COPPICE_BOOTSTRAP_PASSWORD default changeme
 *   COPPICE_SMOKE_EMAIL        default admin@localhost
 *   COPPICE_SMOKE_PASSWORD     default changeme
 *   COPPICE_SMOKE_REPO_PATH    default /tmp/smoke-repo
 */

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const WEB = process.env.COPPICE_WEB_URL ?? 'http://localhost:5001';
const BOOTSTRAP_PASSWORD =
  process.env.COPPICE_BOOTSTRAP_PASSWORD ?? 'changeme';
const EMAIL = process.env.COPPICE_SMOKE_EMAIL ?? 'admin@localhost';
const PASSWORD = process.env.COPPICE_SMOKE_PASSWORD ?? 'changeme';
const SMOKE_REPO_PATH =
  process.env.COPPICE_SMOKE_REPO_PATH ?? '/tmp/smoke-repo';
// Custom (preset-less) agent: its mock fixture key is the slug of this name, so
// `fixtures/agent-responses/<slug>/work_on_ticket.json` drives knowledge_search.
const SEARCH_WORKER_NAME = 'm06-knowledge-search-worker';

const MAX_HEALTH_ATTEMPTS = 90;
const HEALTH_INTERVAL_MS = 1000;
const POLL_INTERVAL_MS = 250;
const POLL_TIMEOUT_MS = 45_000;

function fail(message) {
  console.error(`smoke: ${message}`);
  process.exit(1);
}

function parseSessionCookie(setCookie) {
  const match = /coppice_session=([^;]+)/.exec(setCookie);
  return match?.[1] ?? null;
}

async function waitForHealth() {
  for (let attempt = 1; attempt <= MAX_HEALTH_ATTEMPTS; attempt += 1) {
    try {
      const response = await fetch(`${API}/health`);
      if (response.ok) return;
    } catch {
      // The freshly built server is not listening yet.
    }
    await new Promise((resolve) => setTimeout(resolve, HEALTH_INTERVAL_MS));
  }
  fail(`server not healthy at ${API}/health after ${MAX_HEALTH_ATTEMPTS}s`);
}

async function bootstrapIfNeeded() {
  const response = await fetch(`${API}/api/auth/bootstrap`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-bootstrap-password': BOOTSTRAP_PASSWORD,
    },
    body: JSON.stringify({ email: EMAIL, password: PASSWORD }),
  });
  if (response.ok || response.status === 403) return;
  fail(`bootstrap failed: ${response.status} ${await response.text()}`);
}

async function login() {
  const response = await fetch(`${API}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ email: EMAIL, password: PASSWORD }),
  });
  if (!response.ok) {
    fail(`login failed: ${response.status} ${await response.text()}`);
  }
  const sessionToken = parseSessionCookie(
    response.headers.get('set-cookie') ?? '',
  );
  const body = await response.json();
  if (!sessionToken || !body.csrfToken) {
    fail('login response missing session cookie or CSRF token');
  }
  return {
    cookie: `coppice_session=${sessionToken}`,
    csrfToken: body.csrfToken,
  };
}

async function api(method, path, { cookie, csrfToken, body } = {}) {
  const headers = { cookie };
  if (body !== undefined) headers['content-type'] = 'application/json';
  if (method !== 'GET') headers['x-csrf-token'] = csrfToken;
  return fetch(`${API}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

async function expectJson(response, expectedStatus, label) {
  if (response.status !== expectedStatus) {
    fail(`${label} failed: ${response.status} ${await response.text()}`);
  }
  return response.json();
}

async function createBoard(auth, suffix) {
  return expectJson(
    await api('POST', '/api/boards', {
      ...auth,
      body: { name: `M06 Knowledge Smoke ${suffix}` },
    }),
    201,
    'create board',
  );
}

async function registerRepo(auth) {
  const response = await api('POST', '/api/repos', {
    ...auth,
    body: {
      name: 'smoke-repo',
      localPath: SMOKE_REPO_PATH,
      defaultBranch: 'main',
    },
  });
  if (response.status === 201) return response.json();
  if (response.status === 409) {
    const reposResponse = await api('GET', '/api/repos', auth);
    const repos = await expectJson(reposResponse, 200, 'list repos');
    const existing = repos.find((repo) => repo.localPath === SMOKE_REPO_PATH);
    if (existing?.id) return existing;
  }
  fail(`register repo failed: ${response.status} ${await response.text()}`);
}

async function createAgent(auth, name, presetKey) {
  const presets = await expectJson(
    await api('GET', '/api/agent-presets', auth),
    200,
    'list presets',
  );
  const presetId = presets.items?.find((preset) => preset.key === presetKey)?.id;
  if (!presetId) fail(`agent preset ${presetKey} not available`);
  return expectJson(
    await api('POST', '/api/agents', {
      ...auth,
      body: { name, presetId, connector: 'mock' },
    }),
    201,
    'create agent',
  );
}

async function createSearchWorker(auth) {
  return expectJson(
    await api('POST', '/api/agents', {
      ...auth,
      body: {
        name: SEARCH_WORKER_NAME,
        role: 'Research',
        systemPrompt: 'Search project knowledge, then report.',
        connector: 'mock',
      },
    }),
    201,
    'create knowledge search worker',
  );
}

async function createTicket(boardId, title, description, auth) {
  return expectJson(
    await api('POST', `/api/boards/${boardId}/tickets`, {
      ...auth,
      body: { title, description },
    }),
    201,
    'create ticket',
  );
}

async function patchTicket(ticketId, body, auth, label) {
  return expectJson(
    await api('PATCH', `/api/tickets/${ticketId}`, { ...auth, body }),
    200,
    label,
  );
}

async function listRuns(ticketId, auth) {
  const body = await expectJson(
    await api('GET', `/api/tickets/${ticketId}/runs`, auth),
    200,
    'list runs',
  );
  return body.runs ?? [];
}

async function ensureRun(ticketId, auth) {
  const existing = await listRuns(ticketId, auth);
  if (existing.length > 0) return existing[0];
  const response = await api('POST', `/api/tickets/${ticketId}/run-agent`, auth);
  if (response.status === 409) {
    const runs = await listRuns(ticketId, auth);
    if (runs[0]) return runs[0];
  }
  const body = await expectJson(response, 201, 'start run');
  return body.run;
}

async function poll(label, callback) {
  const deadline = Date.now() + POLL_TIMEOUT_MS;
  let last;
  while (Date.now() < deadline) {
    last = await callback();
    if (last) return last;
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
  fail(`timed out waiting for ${label}; last=${JSON.stringify(last)}`);
}

async function inbox(boardId, auth) {
  const page = await expectJson(
    await api(
      'GET',
      `/api/knowledge/inbox?boardId=${encodeURIComponent(boardId)}&limit=100`,
      auth,
    ),
    200,
    'list knowledge inbox',
  );
  return page.items;
}

async function approve(item, auth) {
  const approved = await expectJson(
    await api('POST', `/api/knowledge/${item.id}/approve`, {
      ...auth,
      body: { expectedVersion: item.version },
    }),
    200,
    'approve knowledge candidate',
  );
  if (approved.status !== 'approved' || approved.activeRevisionId !== approved.revisionId) {
    fail(`approval did not activate the current revision: ${JSON.stringify(approved)}`);
  }
  return approved;
}

async function governManualCandidate(board, title, description, auth) {
  const candidate = await expectJson(
    await api('POST', '/api/knowledge', {
      ...auth,
      body: {
        scope: 'board',
        boardId: board.id,
        agentId: null,
        knowledgeType: 'test_command',
        title: 'Draft retrieval title',
        content: 'Draft retrieval content',
        sourceType: 'human_note',
        sourceId: null,
        sourceRunId: null,
        confidence: 'high',
      },
    }),
    201,
    'create manual knowledge candidate',
  );
  if (candidate.status !== 'pending' || candidate.activeRevisionId !== null) {
    fail(`manual candidate bypassed governance: ${JSON.stringify(candidate)}`);
  }
  const edited = await expectJson(
    await api('PATCH', `/api/knowledge/${candidate.id}`, {
      ...auth,
      body: { expectedVersion: candidate.version, title, content: description },
    }),
    200,
    'edit knowledge candidate',
  );
  if (edited.revisionNumber !== 2 || edited.revisionId === candidate.revisionId) {
    fail('knowledge edit did not create an immutable replacement revision');
  }
  const approved = await approve(edited, auth);
  if ((await inbox(board.id, auth)).some((entry) => entry.id === candidate.id)) {
    fail('approved knowledge remained in Pending inbox');
  }
  console.log(`smoke: activated manual knowledge revision ${approved.revisionId}`);
  return approved;
}

async function compactDoneTicket(board, compactor, suffix, auth) {
  const startedAt = Date.now() - 1000;
  await expectJson(
    await api('PUT', '/api/settings/knowledge', {
      ...auth,
      body: { compactionAgentId: compactor.id },
    }),
    200,
    'configure compaction agent',
  );
  const ticket = await createTicket(
    board.id,
    `M06 compaction ${suffix}`,
    `Run make test-unit while iterating. Compaction marker ${suffix}.`,
    auth,
  );
  await expectJson(
    await api('PATCH', `/api/tickets/${ticket.id}/status`, {
      ...auth,
      body: { status: 'wait_for_final_review' },
    }),
    200,
    'move compaction ticket to Human Review',
  );
  await expectJson(
    await api('POST', `/api/tickets/${ticket.id}/final-approve`, {
      ...auth,
      body: {},
    }),
    200,
    'accept compaction ticket',
  );

  const candidate = await poll('compacted Pending candidate', async () => {
    const status = await expectJson(
      await api('GET', '/api/knowledge/compaction', auth),
      200,
      'get compaction status',
    );
    if (status.state === 'failed' && status.lastBatch?.errorMessage !== 'cancelled') {
      fail(`compaction failed: ${status.lastBatch?.errorMessage ?? 'unknown error'}`);
    }
    if (status.state === 'idle' && status.queuedCount > 0) {
      const response = await api('POST', '/api/knowledge/compaction/run', auth);
      if (response.status !== 202 && response.status !== 409) {
        fail(`compact now failed: ${response.status} ${await response.text()}`);
      }
    }
    return (
      (await inbox(board.id, auth)).find(
        (entry) =>
          entry.compactionBatchId &&
          entry.sourceTicketIds.includes(ticket.id) &&
          entry.knowledgeType === 'test_command',
      ) ?? null
    );
  });
  if (
    candidate.status !== 'pending' ||
    candidate.sourceType !== 'ticket' ||
    candidate.policyDecision !== 'human_review' ||
    candidate.compactionAgentName !== compactor.name
  ) {
    fail(`compaction candidate did not fail closed: ${JSON.stringify(candidate)}`);
  }
  const notifications = await expectJson(
    await api('GET', '/api/notifications?filter=all&limit=100', auth),
    200,
    'list notifications',
  );
  if (
    (notifications.items ?? []).some(
      (entry) =>
        new Date(entry.createdAt).getTime() >= startedAt &&
        (entry.type === 'knowledge_compaction_failed' ||
          (entry.type === 'agent_run_finished' && entry.ticketId === null)),
    )
  ) {
    fail('successful compaction produced a notification');
  }
  const approved = await approve(candidate, auth);
  console.log(`smoke: compacted and approved candidate ${approved.id}`);
  return approved;
}

async function fullRunUses(board, repo, worker, items, suffix, auth) {
  const ticket = await createTicket(
    board.id,
    `M06 retrieval ${suffix}: make test-unit`,
    `Use governed knowledge marker ${suffix} and make test-unit while iterating.`,
    auth,
  );
  await patchTicket(ticket.id, { repoId: repo.id }, auth, 'attach repo');
  await expectJson(
    await api('POST', `/api/tickets/${ticket.id}/assign`, {
      ...auth,
      body: { agentId: worker.id },
    }),
    200,
    'assign agent',
  );
  const startedRun = await ensureRun(ticket.id, auth);
  const run = await poll('Full agent run success', async () => {
    const runs = await listRuns(ticket.id, auth);
    const current = runs.find((entry) => entry.id === startedRun.id) ?? runs[0];
    if (current?.status === 'failed' || current?.status === 'blocked') {
      fail(`run ${current.id} ended ${current.status}: ${current.errorMessage ?? ''}`);
    }
    return current?.status === 'succeeded' ? current : null;
  });

  const usage = await expectJson(
    await api('GET', `/api/agent-runs/${run.id}/knowledge-used`, auth),
    200,
    'get Knowledge Used',
  );
  // Only approved revisions may be returned by knowledge_search: the pending
  // compaction candidate (CSRF rule) must not appear in the audit.
  for (const pending of await inbox(board.id, auth)) {
    if (usage.items.some((entry) => entry.itemId === pending.id)) {
      fail(`pending knowledge ${pending.id} was used by the run`);
    }
  }
  for (const item of items) {
    const exact = usage.items.filter(
      (entry) => entry.itemId === item.id && entry.revisionId === item.revisionId,
    );
    if (exact.length !== 1) {
      fail(`expected revision ${item.revisionId} once; usage=${JSON.stringify(usage)}`);
    }
    if (
      exact[0].tokenCount <= 0 ||
      !exact[0].renderedContent.includes(item.title) ||
      typeof exact[0].score !== 'number'
    ) {
      fail(`usage snapshot missing exact rendered content: ${JSON.stringify(exact[0])}`);
    }
  }
  console.log(`smoke: audited ${items.length} knowledge revisions on run ${run.id}`);
}

async function main() {
  console.log(`smoke: waiting for ${API}/health`);
  await waitForHealth();
  await bootstrapIfNeeded();
  const auth = await login();
  const suffix = `${Date.now()}-${Math.floor(Math.random() * 10_000)}`;
  const board = await createBoard(auth, suffix);
  const repo = await registerRepo(auth);
  const worker = await createSearchWorker(auth);
  const compactor = await createAgent(
    auth,
    `Knowledge Smoke Compactor ${suffix}`,
    'backend_engineer',
  );
  const previous = await expectJson(
    await api('GET', '/api/settings/knowledge', auth),
    200,
    'get knowledge settings',
  );

  try {
    const manual = await governManualCandidate(
      board,
      `M06 exact retrieval ${suffix}`,
      `Use governed knowledge marker ${suffix} for this ticket.`,
      auth,
    );
    const compacted = await compactDoneTicket(board, compactor, suffix, auth);
    await fullRunUses(board, repo, worker, [manual, compacted], suffix, auth);
  } finally {
    await api('PUT', '/api/settings/knowledge', {
      ...auth,
      body: { compactionAgentId: previous.compactionAgentId },
    });
    // The fixed name would otherwise pile up duplicate agents across reruns.
    await api('DELETE', `/api/agents/${worker.id}`, auth);
  }

  for (const route of ['/knowledge', '/agents']) {
    const webResponse = await fetch(`${WEB}${route}`);
    const webBody = await webResponse.text();
    if (!webResponse.ok || !webBody.includes('id="root"')) {
      fail(`web route ${route} unavailable: ${webResponse.status}`);
    }
  }

  console.log('smoke: M06 governed knowledge flow passed');
}

main().catch((error) => {
  fail(error instanceof Error ? error.message : String(error));
});
