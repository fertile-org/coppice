#!/usr/bin/env node
/**
 * M02 board API smoke test.
 *
 * Validates bootstrap/login, project creation, and ticket CRUD against a
 * running Coppice server. Browser UI smoke (login page, drag-drop board)
 * runs locally via `make e2e-smoke` with compose services up.
 *
 * Env:
 *   COPPICE_API_URL            default http://localhost:8080
 *   COPPICE_WEB_URL            optional http://localhost:5173 (reachability only)
 *   COPPICE_BOOTSTRAP_PASSWORD default changeme
 *   COPPICE_SMOKE_EMAIL        default admin@localhost
 *   COPPICE_SMOKE_PASSWORD     default changeme
 */

const API = process.env.COPPICE_API_URL ?? 'http://localhost:8080';
const WEB = process.env.COPPICE_WEB_URL ?? 'http://localhost:5173';
const BOOTSTRAP_PASSWORD =
  process.env.COPPICE_BOOTSTRAP_PASSWORD ?? 'changeme';
const EMAIL = process.env.COPPICE_SMOKE_EMAIL ?? 'admin@localhost';
const PASSWORD = process.env.COPPICE_SMOKE_PASSWORD ?? 'changeme';

const MAX_HEALTH_ATTEMPTS = 60;
const HEALTH_INTERVAL_MS = 1000;

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
    body: { name: 'Smoke Test Project' },
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

async function createTicket(projectId, auth) {
  const res = await api('POST', `/api/projects/${projectId}/tickets`, {
    ...auth,
    body: { title: 'Smoke ticket', description: 'M02 smoke test' },
  });

  if (res.status !== 201) {
    fail(`create ticket failed: ${res.status} ${await res.text()}`);
  }

  const ticket = await res.json();
  if (!ticket.id || ticket.status !== 'backlog') {
    fail(`unexpected ticket response: ${JSON.stringify(ticket)}`);
  }

  console.log(`smoke: created ticket ${ticket.id} (status=${ticket.status})`);
  return ticket;
}

async function listTickets(projectId, auth) {
  const res = await api('GET', `/api/projects/${projectId}/tickets`, auth);

  if (!res.ok) {
    fail(`list tickets failed: ${res.status} ${await res.text()}`);
  }

  const tickets = await res.json();
  if (!Array.isArray(tickets) || tickets.length < 1) {
    fail('expected at least one ticket in project');
  }

  console.log(`smoke: listed ${tickets.length} ticket(s)`);
}

async function checkWebOptional() {
  try {
    const res = await fetch(WEB, { redirect: 'manual' });
    if (res.ok || res.status === 304 || res.status === 302) {
      console.log(`smoke: web reachable at ${WEB}`);
      return;
    }
    console.log(`smoke: web at ${WEB} returned ${res.status} (non-fatal)`);
  } catch {
    console.log(`smoke: web at ${WEB} not reachable (non-fatal)`);
  }
}

async function main() {
  console.log(`smoke: waiting for ${API}/health`);
  await waitForHealth();

  await bootstrapIfNeeded();
  const auth = await login();
  const project = await createProject(auth);
  await createTicket(project.id, auth);
  await listTickets(project.id, auth);
  await checkWebOptional();

  console.log('smoke: M02 board API flow passed');
}

main().catch((err) => {
  fail(err instanceof Error ? err.message : String(err));
});
