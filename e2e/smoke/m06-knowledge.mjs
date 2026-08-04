#!/usr/bin/env node
/**
 * M06 governed knowledge API + web route smoke test.
 *
 * Proves the default stack can govern a manual candidate, embed and retrieve
 * its exact revision in a Full run, expose the run audit, and idempotently
 * extract a Pending candidate after a separate ticket enters Done.
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

async function createProject(auth, suffix) {
  return expectJson(
    await api('POST', '/api/projects', {
      ...auth,
      body: { name: `M06 Knowledge Smoke ${suffix}` },
    }),
    201,
    'create project',
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

async function createAgent(auth, suffix) {
  const presets = await expectJson(
    await api('GET', '/api/agent-presets', auth),
    200,
    'list presets',
  );
  const presetId = presets.items?.[0]?.id;
  if (!presetId) fail('no agent preset available');
  return expectJson(
    await api('POST', '/api/agents', {
      ...auth,
      body: {
        name: `Knowledge Smoke Agent ${suffix}`,
        presetId,
        connector: 'mock',
      },
    }),
    201,
    'create agent',
  );
}

async function createTicket(projectId, title, description, auth) {
  return expectJson(
    await api('POST', `/api/projects/${projectId}/tickets`, {
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

async function main() {
  console.log(`smoke: waiting for ${API}/health`);
  await waitForHealth();
  await bootstrapIfNeeded();
  const auth = await login();
  const suffix = `${Date.now()}-${Math.floor(Math.random() * 10_000)}`;
  const project = await createProject(auth, suffix);
  const repo = await registerRepo(auth);
  const agent = await createAgent(auth, suffix);

  const title = `M06 exact retrieval ${suffix}`;
  const description = `Use governed knowledge marker ${suffix} for this ticket.`;
  const ticket = await createTicket(project.id, title, description, auth);

  const candidate = await expectJson(
    await api('POST', '/api/knowledge', {
      ...auth,
      body: {
        scope: 'project',
        projectId: project.id,
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
  if (candidate.status !== 'pending' || candidate.embeddingStatus !== 'not_requested') {
    fail(`manual candidate bypassed governance: ${JSON.stringify(candidate)}`);
  }

  const edited = await expectJson(
    await api('PATCH', `/api/knowledge/${candidate.id}`, {
      ...auth,
      body: {
        expectedVersion: candidate.version,
        title,
        content: description,
      },
    }),
    200,
    'edit knowledge candidate',
  );
  if (edited.revisionNumber !== 2 || edited.revisionId === candidate.revisionId) {
    fail('knowledge edit did not create an immutable replacement revision');
  }

  const approved = await expectJson(
    await api('POST', `/api/knowledge/${candidate.id}/approve`, {
      ...auth,
      body: { expectedVersion: edited.version },
    }),
    200,
    'approve knowledge candidate',
  );
  if (approved.status !== 'approved') fail('approved item has wrong status');

  const ready = await poll('knowledge embedding activation', async () => {
    const current = await expectJson(
      await api('GET', `/api/knowledge/${candidate.id}`, auth),
      200,
      'get knowledge item',
    );
    if (current.embeddingStatus === 'failed') {
      fail(`embedding failed: ${current.embeddingError ?? 'unknown error'}`);
    }
    return current.embeddingStatus === 'ready' &&
      current.activeRevisionId === current.revisionId
      ? current
      : null;
  });
  console.log(`smoke: activated knowledge revision ${ready.revisionId}`);

  const inbox = await expectJson(
    await api(
      'GET',
      `/api/knowledge/inbox?projectId=${encodeURIComponent(project.id)}&limit=100`,
      auth,
    ),
    200,
    'list knowledge inbox',
  );
  if (inbox.items.some((entry) => entry.id === candidate.id)) {
    fail('approved knowledge remained in Pending inbox');
  }

  await patchTicket(ticket.id, { repoId: repo.id }, auth, 'attach repo');
  await expectJson(
    await api('POST', `/api/tickets/${ticket.id}/assign`, {
      ...auth,
      body: { agentId: agent.id },
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
  const exact = usage.items.filter(
    (entry) =>
      entry.itemId === candidate.id && entry.revisionId === ready.revisionId,
  );
  if (exact.length !== 1) {
    fail(`expected exact knowledge revision once; usage=${JSON.stringify(usage)}`);
  }
  if (
    exact[0].tokenCount <= 0 ||
    !exact[0].renderedContent.includes(title) ||
    !exact[0].renderedContent.includes(description)
  ) {
    fail(`usage snapshot missing exact rendered content: ${JSON.stringify(exact[0])}`);
  }
  const usageAgain = await expectJson(
    await api('GET', `/api/agent-runs/${run.id}/knowledge-used`, auth),
    200,
    'repeat Knowledge Used query',
  );
  if (
    usageAgain.items.filter((entry) => entry.revisionId === ready.revisionId)
      .length !== 1
  ) {
    fail('knowledge revision was logged more than once for a run');
  }
  console.log(`smoke: audited exact revision on run ${run.id}`);

  const extractionTicket = await createTicket(
    project.id,
    `M06 extraction ${suffix}`,
    `Extraction evidence ${suffix}`,
    auth,
  );
  await expectJson(
    await api('PATCH', `/api/tickets/${extractionTicket.id}/status`, {
      ...auth,
      body: { status: 'done' },
    }),
    200,
    'transition extraction ticket to Done',
  );
  const extracted = await poll('Pending extracted candidate', async () => {
    const page = await expectJson(
      await api(
        'GET',
        `/api/knowledge/inbox?projectId=${encodeURIComponent(project.id)}&limit=100`,
        auth,
      ),
      200,
      'poll knowledge inbox',
    );
    return (
      page.items.find(
        (entry) =>
          entry.sourceType === 'agent_summary' &&
          entry.sourceId === extractionTicket.id,
      ) ?? null
    );
  });
  if (
    extracted.status !== 'pending' ||
    extracted.policyDecision !== 'human_review' ||
    extracted.embeddingStatus !== 'not_requested'
  ) {
    fail(`extraction policy did not fail closed: ${JSON.stringify(extracted)}`);
  }

  await expectJson(
    await api('PATCH', `/api/tickets/${extractionTicket.id}/status`, {
      ...auth,
      body: { status: 'done' },
    }),
    200,
    'repeat Done transition',
  );
  await new Promise((resolve) => setTimeout(resolve, 1000));
  const afterRepeat = await expectJson(
    await api(
      'GET',
      `/api/knowledge/inbox?projectId=${encodeURIComponent(project.id)}&limit=100`,
      auth,
    ),
    200,
    'verify idempotent extraction',
  );
  if (
    afterRepeat.items.filter(
      (entry) =>
        entry.sourceType === 'agent_summary' &&
        entry.sourceId === extractionTicket.id,
    ).length !== 1
  ) {
    fail('repeating Done produced duplicate extracted knowledge');
  }

  const webResponse = await fetch(`${WEB}/knowledge`);
  const webBody = await webResponse.text();
  if (!webResponse.ok || !webBody.includes('id="root"')) {
    fail(`Knowledge web route unavailable: ${webResponse.status}`);
  }

  console.log('smoke: M06 governed knowledge flow passed');
}

main().catch((error) => {
  fail(error instanceof Error ? error.message : String(error));
});
