#!/usr/bin/env node
/**
 * M05 workflow collaboration API smoke test.
 *
 * Scope B mock pipeline: PM → Ready + recommendation → engineer blocked +
 * @mention → PM respond → engineer resume → Wait for Final Review → Final Approve.
 *
 * Requires WORKFLOW_AUTO_START_RUNS=true on the server (see deploy/docker-compose.yml).
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

const MAX_HEALTH_ATTEMPTS = 60;
const HEALTH_INTERVAL_MS = 1000;
const POLL_INTERVAL_MS = 200;
const RUN_POLL_TIMEOUT_MS = 30_000;
const WORKFLOW_POLL_TIMEOUT_MS = 120_000;

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
    body: { name: 'M05 Smoke Project' },
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

async function createAgentFromPresetKey(auth, presetKey, name) {
  const presetsRes = await api('GET', '/api/agent-presets', auth);
  if (!presetsRes.ok) {
    fail(
      `list agent presets failed: ${presetsRes.status} ${await presetsRes.text()}`,
    );
  }

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
  if (!agent.id) {
    fail(`create agent ${presetKey} response missing id`);
  }
  if (agent.connector !== 'mock') {
    fail(`expected mock connector for ${presetKey}, got ${agent.connector}`);
  }

  console.log(`smoke: created ${presetKey} agent ${agent.id}`);
  return agent;
}

async function createTicket(projectId, auth) {
  const res = await api('POST', `/api/projects/${projectId}/tickets`, {
    ...auth,
    body: {
      title: 'M05 smoke ticket',
      description: 'Workflow collaboration smoke test',
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

  return res.json();
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
  if (!Array.isArray(body.runs)) {
    fail('list runs response missing runs array');
  }
  return body.runs;
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
    `timed out waiting for ticket condition: ${label}; last status=${last.status}`,
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

async function finalApprove(ticketId, auth) {
  const res = await api('POST', `/api/tickets/${ticketId}/final-approve`, {
    ...auth,
    body: {},
  });

  if (!res.ok) {
    fail(`final-approve failed: ${res.status} ${await res.text()}`);
  }

  return res.json();
}

async function main() {
  console.log(`smoke: waiting for ${API}/health`);
  await waitForHealth();

  await bootstrapIfNeeded();
  const auth = await login();
  const project = await createProject(auth);
  const repo = await registerRepo(auth);
  const pm = await createAgentFromPresetKey(auth, 'pm', 'PM Agent');
  const engineer = await createAgentFromPresetKey(
    auth,
    'backend_engineer',
    'Backend Engineer',
  );
  const ticket = await createTicket(project.id, auth);

  await setTicketRepo(ticket.id, repo.id, auth);
  await assignAgent(ticket.id, pm.id, auth);

  const pmReady = await pollTicketUntil(
    ticket.id,
    auth,
    'PM run → ready + recommendation',
    RUN_POLL_TIMEOUT_MS,
    (t) =>
      t.status === 'ready' &&
      t.pendingAssignRecommendation?.recommendedAgentKey === 'backend_engineer',
  );
  console.log(
    `smoke: PM run complete; status=${pmReady.status}, recommendation=${pmReady.pendingAssignRecommendation.recommendedAgentKey}`,
  );

  const afterEngineerAssign = await assignAgent(ticket.id, engineer.id, auth);
  if (afterEngineerAssign.pendingAssignRecommendation != null) {
    fail('expected pending recommendation cleared after engineer assign');
  }
  console.log('smoke: assigned backend engineer');

  await pollRunsUntil(
    ticket.id,
    auth,
    'engineer blocked run',
    RUN_POLL_TIMEOUT_MS,
    (runs) =>
      runs.some(
        (run) =>
          run.agentId === engineer.id &&
          run.jobType === 'work_on_ticket' &&
          run.status === 'blocked',
      ),
  );
  console.log('smoke: engineer blocked with mention');

  await pollRunsUntil(
    ticket.id,
    auth,
    'PM respond_to_mention succeeded',
    RUN_POLL_TIMEOUT_MS,
    (runs) =>
      runs.some(
        (run) =>
          run.agentId === pm.id &&
          run.jobType === 'respond_to_mention' &&
          run.status === 'succeeded',
      ),
  );
  console.log('smoke: PM respond_to_mention succeeded');

  await pollRunsUntil(
    ticket.id,
    auth,
    'engineer resume succeeded',
    RUN_POLL_TIMEOUT_MS,
    (runs) => {
      const engineerWorkRuns = runs.filter(
        (run) =>
          run.agentId === engineer.id && run.jobType === 'work_on_ticket',
      );
      return (
        engineerWorkRuns.length >= 2 &&
        engineerWorkRuns.some((run) => run.status === 'succeeded')
      );
    },
  );
  console.log('smoke: engineer resume succeeded');

  const finalTicket = await pollTicketUntil(
    ticket.id,
    auth,
    'wait_for_final_review',
    WORKFLOW_POLL_TIMEOUT_MS,
    (t) => t.status === 'wait_for_final_review',
  );
  console.log(`smoke: ticket reached ${finalTicket.status}`);

  const approved = await finalApprove(ticket.id, auth);
  if (approved.status !== 'done') {
    fail(`expected status done after final-approve, got ${approved.status}`);
  }
  if (approved.substatus != null) {
    fail(`expected null substatus after final-approve, got ${approved.substatus}`);
  }

  console.log('smoke: M05 workflow collaboration flow passed');
}

main().catch((err) => {
  fail(err instanceof Error ? err.message : String(err));
});
