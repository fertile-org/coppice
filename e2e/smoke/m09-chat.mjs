#!/usr/bin/env node
/**
 * M09 agent chat API smoke test.
 *
 * Validates session create (agent required, project optional), one mock
 * conversation turn, and cutoff → child session with summary seed.
 *
 * Env:
 *   COPPICE_API_URL            default http://localhost:5000
 *   COPPICE_BOOTSTRAP_PASSWORD default changeme
 *   COPPICE_SMOKE_EMAIL        default admin@localhost
 *   COPPICE_SMOKE_PASSWORD     default changeme
 */

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const BOOTSTRAP_PASSWORD =
  process.env.COPPICE_BOOTSTRAP_PASSWORD ?? 'changeme';
const EMAIL = process.env.COPPICE_SMOKE_EMAIL ?? 'admin@localhost';
const PASSWORD = process.env.COPPICE_SMOKE_PASSWORD ?? 'changeme';
const MOCK_CHAT_SUMMARY = 'Mock chat reply for conversation mode.';

const MAX_HEALTH_ATTEMPTS = 60;
const HEALTH_INTERVAL_MS = 1000;
const TURN_POLL_TIMEOUT_MS = 30_000;
const TURN_POLL_INTERVAL_MS = 500;

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

  return fetch(`${API}${path}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
}

async function createBackendEngineerAgent(auth) {
  const presetsRes = await api('GET', '/api/agent-presets', auth);
  if (!presetsRes.ok) {
    fail(
      `list agent presets failed: ${presetsRes.status} ${await presetsRes.text()}`,
    );
  }

  const presets = await presetsRes.json();
  const preset = presets.items?.find((item) => item.key === 'backend_engineer');
  if (!preset?.id) {
    fail('backend_engineer preset not found');
  }

  const res = await api('POST', '/api/agents', {
    ...auth,
    body: { name: 'M09 Smoke Chat Agent', presetId: preset.id },
  });

  if (res.status !== 201) {
    fail(`create agent failed: ${res.status} ${await res.text()}`);
  }

  const agent = await res.json();
  if (!agent.id) {
    fail('create agent response missing id');
  }

  console.log(`smoke: created backend_engineer agent ${agent.id}`);
  return agent;
}

async function createChatSession(agentId, auth) {
  const res = await api('POST', '/api/chat/sessions', {
    ...auth,
    body: { agentId },
  });

  if (res.status !== 201) {
    fail(`create chat session failed: ${res.status} ${await res.text()}`);
  }

  const session = await res.json();
  if (!session.id) {
    fail('create chat session response missing id');
  }
  if (session.projectId != null) {
    fail(`expected unbound session projectId null, got ${session.projectId}`);
  }
  if (session.status !== 'active') {
    fail(`expected active session, got ${session.status}`);
  }

  console.log(`smoke: created chat session ${session.id} (project unbound)`);
  return session;
}

async function postHumanMessage(sessionId, auth) {
  const res = await api('POST', `/api/chat/sessions/${sessionId}/messages`, {
    ...auth,
    body: { body: 'What is the cwd policy for chat?' },
  });

  if (res.status !== 201) {
    fail(`post chat message failed: ${res.status} ${await res.text()}`);
  }

  const body = await res.json();
  if (!body.runId) {
    fail('post message response missing runId');
  }
  if (body.message?.role !== 'human') {
    fail(`expected human message, got ${body.message?.role}`);
  }

  console.log(`smoke: posted human message; run ${body.runId}`);
  return body;
}

async function pollUntilAgentReply(sessionId, auth) {
  const deadline = Date.now() + TURN_POLL_TIMEOUT_MS;

  while (Date.now() < deadline) {
    const res = await api('GET', `/api/chat/sessions/${sessionId}/messages`, auth);
    if (!res.ok) {
      fail(`list messages failed: ${res.status} ${await res.text()}`);
    }

    const body = await res.json();
    const messages = body.messages;
    if (!Array.isArray(messages)) {
      fail('list messages response missing messages array');
    }

    const agentMessage = messages.find((m) => m.role === 'agent');
    if (agentMessage) {
      if (!agentMessage.body?.includes(MOCK_CHAT_SUMMARY)) {
        fail(
          `agent reply missing mock summary; body=${JSON.stringify(agentMessage.body)}`,
        );
      }
      console.log('smoke: agent chat reply persisted with mock summary');
      return agentMessage;
    }

    await new Promise((resolve) => setTimeout(resolve, TURN_POLL_INTERVAL_MS));
  }

  fail(
    `timed out waiting for agent chat reply after ${TURN_POLL_TIMEOUT_MS / 1000}s`,
  );
}

async function cutoffSession(sessionId, auth) {
  const res = await api('POST', `/api/chat/sessions/${sessionId}/cutoff`, auth);

  if (!res.ok) {
    fail(`cutoff failed: ${res.status} ${await res.text()}`);
  }

  const body = await res.json();
  if (body.parent?.status !== 'cutoff') {
    fail(`expected parent cutoff, got ${body.parent?.status}`);
  }
  if (body.parent?.id !== sessionId) {
    fail('cutoff parent id mismatch');
  }
  if (!body.child?.id) {
    fail('cutoff response missing child session');
  }
  if (body.child.status !== 'active') {
    fail(`expected active child session, got ${body.child.status}`);
  }
  if (body.child.parentSessionId !== sessionId) {
    fail('child parentSessionId mismatch');
  }
  if (body.seedMessage?.role !== 'system') {
    fail(`expected system seed message, got ${body.seedMessage?.role}`);
  }
  if (!body.seedMessage?.body?.includes('Prior conversation summary')) {
    fail('seed message missing prior conversation summary');
  }

  console.log(
    `smoke: cutoff parent ${sessionId} → child ${body.child.id} with seed`,
  );
  return body;
}

async function assertParentRejectsMessages(sessionId, auth) {
  const res = await api('POST', `/api/chat/sessions/${sessionId}/messages`, {
    ...auth,
    body: { body: 'should fail after cutoff' },
  });

  if (res.status === 201) {
    fail('cutoff parent still accepted a new message');
  }

  console.log('smoke: cutoff parent rejects further messages');
}

async function main() {
  console.log(`smoke: waiting for ${API}/health`);
  await waitForHealth();

  await bootstrapIfNeeded();
  const auth = await login();
  const agent = await createBackendEngineerAgent(auth);
  const session = await createChatSession(agent.id, auth);
  await postHumanMessage(session.id, auth);
  await pollUntilAgentReply(session.id, auth);
  await cutoffSession(session.id, auth);
  await assertParentRejectsMessages(session.id, auth);

  console.log('smoke: M09 agent chat API flow passed');
}

main().catch((err) => {
  fail(err instanceof Error ? err.message : String(err));
});
