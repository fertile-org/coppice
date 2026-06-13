#!/usr/bin/env node
/**
 * M03 agent run API smoke test.
 *
 * Validates bootstrap/login, repo + agent setup, run-agent execution with the
 * mock provider, and agent comment creation against a running Coppice stack.
 *
 * Env:
 *   COPPICE_API_URL            default http://localhost:5000
 *   COPPICE_BOOTSTRAP_PASSWORD default changeme
 *   COPPICE_SMOKE_EMAIL        default admin@localhost
 *   COPPICE_SMOKE_PASSWORD     default changeme
 *   COPPICE_SMOKE_REPO_PATH    default /tmp/smoke-repo (path inside server container)
 */

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const BOOTSTRAP_PASSWORD =
  process.env.COPPICE_BOOTSTRAP_PASSWORD ?? 'changeme';
const EMAIL = process.env.COPPICE_SMOKE_EMAIL ?? 'admin@localhost';
const PASSWORD = process.env.COPPICE_SMOKE_PASSWORD ?? 'changeme';
const SMOKE_REPO_PATH =
  process.env.COPPICE_SMOKE_REPO_PATH ?? '/tmp/smoke-repo';
const MOCK_SUMMARY = 'Mock implementation complete.';

const MAX_HEALTH_ATTEMPTS = 60;
const HEALTH_INTERVAL_MS = 1000;
const RUN_POLL_TIMEOUT_MS = 30_000;
const RUN_POLL_INTERVAL_MS = 500;

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
    body: { name: 'M03 Smoke Project' },
  });

  if (res.status !== 201) {
    fail(`create project failed: ${res.status} ${await res.text()}`);
  }

  const project = await res.json();
  if (!project.id) {
    fail('create project response missing id');
  }

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
    if (!repo.id) {
      fail('register repo response missing id');
    }
    console.log(`smoke: registered repo ${repo.id} at ${SMOKE_REPO_PATH}`);
    return repo;
  }

  if (res.status === 409) {
    const listRes = await api('GET', '/api/repos', auth);
    if (!listRes.ok) {
      fail(`list repos failed: ${listRes.status} ${await listRes.text()}`);
    }
    const repos = await listRes.json();
    const existing = repos.find((r) => r.localPath === SMOKE_REPO_PATH);
    if (existing?.id) {
      console.log(`smoke: reusing registered repo ${existing.id}`);
      return existing;
    }
  }

  fail(`register repo failed: ${res.status} ${await res.text()}`);
}

async function createAgentFromPreset(auth) {
  const presetsRes = await api('GET', '/api/agent-presets', auth);
  if (!presetsRes.ok) {
    fail(
      `list agent presets failed: ${presetsRes.status} ${await presetsRes.text()}`,
    );
  }

  const presets = await presetsRes.json();
  const presetId = presets.items?.[0]?.id;
  if (!presetId) {
    fail('no agent presets available');
  }

  const res = await api('POST', '/api/agents', {
    ...auth,
    body: { name: 'Smoke Worker', presetId },
  });

  if (res.status !== 201) {
    fail(`create agent failed: ${res.status} ${await res.text()}`);
  }

  const agent = await res.json();
  if (!agent.id) {
    fail('create agent response missing id');
  }

  console.log(`smoke: created agent ${agent.id}`);
  return agent;
}

async function createTicket(projectId, auth) {
  const res = await api('POST', `/api/projects/${projectId}/tickets`, {
    ...auth,
    body: {
      title: 'M03 smoke ticket',
      description: 'Agent run smoke test',
    },
  });

  if (res.status !== 201) {
    fail(`create ticket failed: ${res.status} ${await res.text()}`);
  }

  const ticket = await res.json();
  if (!ticket.id) {
    fail('create ticket response missing id');
  }

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

async function runAgent(ticketId, auth) {
  const res = await api('POST', `/api/tickets/${ticketId}/run-agent`, auth);

  if (res.status !== 201) {
    fail(`run agent failed: ${res.status} ${await res.text()}`);
  }

  const body = await res.json();
  const runId = body.run?.id;
  if (!runId) {
    fail('run-agent response missing run.id');
  }

  console.log(`smoke: started run ${runId}`);
  return runId;
}

async function listRuns(ticketId, auth) {
  const res = await api('GET', `/api/tickets/${ticketId}/runs`, auth);

  if (!res.ok) {
    fail(`list runs failed: ${res.status} ${await res.text()}`);
  }

  const body = await res.json();
  if (!Array.isArray(body.runs)) {
    fail('list runs response missing runs array');
  }

  return body.runs;
}

async function pollRunUntilSucceeded(ticketId, auth) {
  const deadline = Date.now() + RUN_POLL_TIMEOUT_MS;

  while (Date.now() < deadline) {
    const runs = await listRuns(ticketId, auth);
    const run = runs[0];
    if (run?.status === 'succeeded') {
      console.log(`smoke: run ${run.id} succeeded`);
      return run;
    }
    if (run?.status === 'failed' || run?.status === 'blocked') {
      fail(`run ended with status ${run.status}: ${run.errorMessage ?? ''}`);
    }
    await new Promise((resolve) => setTimeout(resolve, RUN_POLL_INTERVAL_MS));
  }

  fail(`timed out waiting for run to succeed after ${RUN_POLL_TIMEOUT_MS / 1000}s`);
}

async function assertAgentComment(ticketId, auth) {
  const res = await api('GET', `/api/tickets/${ticketId}/comments`, auth);

  if (!res.ok) {
    fail(`list comments failed: ${res.status} ${await res.text()}`);
  }

  const comments = await res.json();
  if (!Array.isArray(comments)) {
    fail('comments response is not an array');
  }

  const agentComment = comments.find((c) => c.authorType === 'agent');
  if (!agentComment) {
    fail('expected agent comment on ticket');
  }

  if (!agentComment.body.includes(MOCK_SUMMARY)) {
    fail(
      `agent comment missing mock summary; body=${JSON.stringify(agentComment.body)}`,
    );
  }

  console.log('smoke: agent comment contains mock summary');
}

async function main() {
  console.log(`smoke: waiting for ${API}/health`);
  await waitForHealth();

  await bootstrapIfNeeded();
  const auth = await login();
  const project = await createProject(auth);
  const repo = await registerRepo(auth);
  const agent = await createAgentFromPreset(auth);
  const ticket = await createTicket(project.id, auth);

  await setTicketRepo(ticket.id, repo.id, auth);
  await assignAgent(ticket.id, agent.id, auth);
  await runAgent(ticket.id, auth);
  await pollRunUntilSucceeded(ticket.id, auth);

  const runs = await listRuns(ticket.id, auth);
  if (runs.length < 1) {
    fail('expected at least one run for ticket');
  }
  console.log(`smoke: listed ${runs.length} run(s)`);

  await assertAgentComment(ticket.id, auth);

  console.log('smoke: M03 agent run API flow passed');
}

main().catch((err) => {
  fail(err instanceof Error ? err.message : String(err));
});
