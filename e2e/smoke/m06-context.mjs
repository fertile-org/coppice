#!/usr/bin/env node
/**
 * M06 context & long-running tasks API smoke test.
 *
 * PM run with mock `pm/split_pending` fixture → pending split recommendation
 * visible on ticket (powers metadata panel badge). Requires server started with
 * MOCK_AGENT_RESPONSE=pm/split_pending (see Makefile e2e-smoke-m06).
 *
 * Env:
 *   COPPICE_API_URL            default http://localhost:8080
 *   COPPICE_BOOTSTRAP_PASSWORD default changeme
 *   COPPICE_SMOKE_EMAIL        default admin@localhost
 *   COPPICE_SMOKE_PASSWORD     default changeme
 *   COPPICE_SMOKE_REPO_PATH    default /tmp/smoke-repo (path inside server container)
 */

const API = process.env.COPPICE_API_URL ?? 'http://localhost:8080';
const BOOTSTRAP_PASSWORD =
  process.env.COPPICE_BOOTSTRAP_PASSWORD ?? 'changeme';
const EMAIL = process.env.COPPICE_SMOKE_EMAIL ?? 'admin@localhost';
const PASSWORD = process.env.COPPICE_SMOKE_PASSWORD ?? 'changeme';
const SMOKE_REPO_PATH =
  process.env.COPPICE_SMOKE_REPO_PATH ?? '/tmp/smoke-repo';

const MAX_HEALTH_ATTEMPTS = 60;
const HEALTH_INTERVAL_MS = 1000;
const POLL_INTERVAL_MS = 200;
const RUN_POLL_TIMEOUT_MS = 30_000;

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
      const res = await fetch(`${API}/health`);
      if (res.ok) {
        return;
      }
    } catch {
      // server not ready yet
    }
    await new Promise((resolve) => setTimeout(resolve, HEALTH_INTERVAL_MS));
  }
  fail(`server not healthy at ${API}/health after ${MAX_HEALTH_ATTEMPTS}s`);
}

async function bootstrapIfNeeded() {
  const res = await fetch(`${API}/api/auth/bootstrap`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-bootstrap-password': BOOTSTRAP_PASSWORD,
    },
    body: JSON.stringify({ email: EMAIL, password: PASSWORD }),
  });

  if (res.ok) {
    console.log('smoke: bootstrapped admin user');
    return;
  }

  if (res.status === 403) {
    console.log('smoke: admin already bootstrapped');
    return;
  }

  fail(`bootstrap failed: ${res.status} ${await res.text()}`);
}

async function login() {
  const res = await fetch(`${API}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ email: EMAIL, password: PASSWORD }),
  });

  if (!res.ok) {
    fail(`login failed: ${res.status} ${await res.text()}`);
  }

  const setCookie = res.headers.get('set-cookie');
  const sessionToken = setCookie ? parseSessionCookie(setCookie) : null;
  if (!sessionToken) {
    fail('login did not return coppice_session cookie');
  }

  const body = await res.json();
  const csrfToken = body.csrfToken;
  if (!csrfToken) {
    fail('login did not return csrfToken');
  }

  return {
    cookie: `coppice_session=${sessionToken}`,
    csrfToken,
  };
}

async function api(method, path, { cookie, csrfToken, body } = {}) {
  const headers = { cookie };
  if (body !== undefined) {
    headers['content-type'] = 'application/json';
  }
  if (method !== 'GET') {
    headers['x-csrf-token'] = csrfToken;
  }

  const res = await fetch(`${API}${path}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  return res;
}

async function createProject(auth) {
  const res = await api('POST', '/api/projects', {
    ...auth,
    body: { name: 'M06 Smoke Project' },
  });

  if (res.status !== 201) {
    fail(`create project failed: ${res.status} ${await res.text()}`);
  }

  const project = await res.json();
  console.log(`smoke: created project ${project.id}`);
  return project;
}

async function registerRepo(auth) {
  const res = await api('POST', '/api/repos', {
    ...auth,
    body: {
      name: 'smoke-repo',
      localPath: SMOKE_REPO_PATH,
      defaultBranch: 'main',
    },
  });

  if (res.status === 201) {
    const repo = await res.json();
    console.log(`smoke: registered repo ${repo.id}`);
    return repo;
  }

  if (res.status === 409) {
    const listRes = await api('GET', '/api/repos', auth);
    const repos = await listRes.json();
    const existing = repos.find((r) => r.localPath === SMOKE_REPO_PATH);
    if (existing?.id) {
      console.log(`smoke: reusing registered repo ${existing.id}`);
      return existing;
    }
  }

  fail(`register repo failed: ${res.status} ${await res.text()}`);
}

async function createAgentFromPresetKey(auth, presetKey, name) {
  const presetsRes = await api('GET', '/api/agent-presets', auth);
  const presets = await presetsRes.json();
  const preset = presets.items?.find((item) => item.key === presetKey);
  if (!preset?.id) {
    fail(`preset ${presetKey} not found`);
  }

  const res = await api('POST', '/api/agents', {
    ...auth,
    body: { name, presetId: preset.id, connector: 'mock' },
  });

  if (res.status !== 201) {
    fail(`create agent ${presetKey} failed: ${res.status} ${await res.text()}`);
  }

  const agent = await res.json();
  console.log(`smoke: created ${presetKey} agent ${agent.id}`);
  return agent;
}

async function createTicket(projectId, auth) {
  const res = await api('POST', `/api/projects/${projectId}/tickets`, {
    ...auth,
    body: {
      title: 'M06 smoke ticket',
      description: 'Context long-running smoke test',
    },
  });

  if (res.status !== 201) {
    fail(`create ticket failed: ${res.status} ${await res.text()}`);
  }

  const ticket = await res.json();
  console.log(`smoke: created ticket ${ticket.id}`);
  return ticket;
}

async function setTicketRepo(ticketId, repoId, auth) {
  const res = await api('PATCH', `/api/tickets/${ticketId}`, {
    ...auth,
    body: { repoId },
  });

  if (!res.ok) {
    fail(`set ticket repo failed: ${res.status} ${await res.text()}`);
  }
}

async function assignAgent(ticketId, agentId, auth) {
  const res = await api('POST', `/api/tickets/${ticketId}/assign`, {
    ...auth,
    body: { agentId },
  });

  if (!res.ok) {
    fail(`assign agent failed: ${res.status} ${await res.text()}`);
  }
}

async function getTicket(ticketId, auth) {
  const res = await api('GET', `/api/tickets/${ticketId}`, auth);
  if (!res.ok) {
    fail(`get ticket failed: ${res.status} ${await res.text()}`);
  }
  return res.json();
}

async function listRuns(ticketId, auth) {
  const res = await api('GET', `/api/tickets/${ticketId}/runs`, auth);
  if (!res.ok) {
    fail(`list runs failed: ${res.status} ${await res.text()}`);
  }
  const body = await res.json();
  return body.runs ?? [];
}

async function pollTicketUntil(ticketId, auth, label, timeoutMs, predicate) {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    const ticket = await getTicket(ticketId, auth);
    if (predicate(ticket)) {
      return ticket;
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }

  const last = await getTicket(ticketId, auth);
  fail(
    `timed out waiting for ticket condition: ${label}; pendingSplit=${Boolean(last.pendingSplitRecommendation)}`,
  );
}

async function pollRunsUntil(ticketId, auth, label, timeoutMs, predicate) {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    const runs = await listRuns(ticketId, auth);
    if (predicate(runs)) {
      return runs;
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }

  fail(`timed out waiting for runs condition: ${label}`);
}

async function main() {
  console.log(`smoke: waiting for ${API}/health`);
  await waitForHealth();

  await bootstrapIfNeeded();
  const auth = await login();
  const project = await createProject(auth);
  const repo = await registerRepo(auth);
  const pm = await createAgentFromPresetKey(auth, 'pm', 'PM Agent');
  const ticket = await createTicket(project.id, auth);

  await setTicketRepo(ticket.id, repo.id, auth);
  await assignAgent(ticket.id, pm.id, auth);

  const runRes = await api('POST', `/api/tickets/${ticket.id}/run-agent`, auth);
  if (runRes.status !== 201) {
    fail(`run-agent failed: ${runRes.status} ${await runRes.text()}`);
  }
  console.log('smoke: started PM run');

  await pollRunsUntil(
    ticket.id,
    auth,
    'PM split_pending run succeeded',
    RUN_POLL_TIMEOUT_MS,
    (runs) =>
      runs.some(
        (run) =>
          run.agentId === pm.id &&
          run.jobType === 'work_on_ticket' &&
          run.status === 'succeeded',
      ),
  );
  console.log('smoke: PM run succeeded');

  const withSplits = await pollTicketUntil(
    ticket.id,
    auth,
    'pending split recommendation',
    RUN_POLL_TIMEOUT_MS,
    (t) =>
      Array.isArray(t.pendingSplitRecommendation?.splits) &&
      t.pendingSplitRecommendation.splits.length >= 2,
  );

  const splitTitles = withSplits.pendingSplitRecommendation.splits.map(
    (s) => s.title,
  );
  console.log(
    `smoke: pending split badge data present (${splitTitles.length} splits: ${splitTitles.join(', ')})`,
  );

  const childrenRes = await api(
    'GET',
    `/api/tickets/${ticket.id}/children`,
    auth,
  );
  if (!childrenRes.ok) {
    fail(`list children failed: ${childrenRes.status} ${await childrenRes.text()}`);
  }
  const children = await childrenRes.json();
  if (!Array.isArray(children) || children.length !== 0) {
    fail(`expected no child tickets before approval, got ${children?.length ?? '?'}`);
  }

  console.log('smoke: M06 context long-running flow passed');
}

main().catch((err) => {
  fail(err instanceof Error ? err.message : String(err));
});
