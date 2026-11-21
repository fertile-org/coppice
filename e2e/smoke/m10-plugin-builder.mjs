#!/usr/bin/env node
/**
 * M10 plugin-builder smoke (PB-14).
 *
 * Skills-only happy path: mock plan → plan_ready → mock build (real fixture
 * bytes) → disabled install + auto-test → Accept with enable.
 *
 * Requires the server with MOCK_AGENT_RESPONSE cleared so agent-keyed fixtures
 * apply (pm/plan_plugin_build.json, backend_engineer/build_plugin.json).
 * See `make e2e-smoke-m10`.
 */

import { execFileSync } from 'node:child_process';

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const BOOTSTRAP_PASSWORD =
  process.env.COPPICE_BOOTSTRAP_PASSWORD ?? 'changeme';
const EMAIL = process.env.COPPICE_SMOKE_EMAIL ?? 'admin@localhost';
const PASSWORD = process.env.COPPICE_SMOKE_PASSWORD ?? 'changeme';
const PLUGIN_NAME = 'm10-builder-smoke';
const COMPOSE_FILE = 'deploy/docker-compose.yml';

const MAX_HEALTH_ATTEMPTS = 60;
const HEALTH_INTERVAL_MS = 1000;
const POLL_TIMEOUT_MS = 90_000;
const POLL_INTERVAL_MS = 500;

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
      if (res.ok) return;
    } catch {
      // not ready
    }
    await new Promise((r) => setTimeout(r, HEALTH_INTERVAL_MS));
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
  if (res.ok || res.status === 403) return;
  fail(`bootstrap failed: ${res.status} ${await res.text()}`);
}

async function login() {
  const res = await fetch(`${API}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ email: EMAIL, password: PASSWORD }),
  });
  if (!res.ok) fail(`login failed: ${res.status} ${await res.text()}`);
  const setCookie = res.headers.get('set-cookie');
  const sessionToken = setCookie ? parseSessionCookie(setCookie) : null;
  if (!sessionToken) fail('login did not return coppice_session cookie');
  const body = await res.json();
  if (!body.csrfToken) fail('login did not return csrfToken');
  return { cookie: `coppice_session=${sessionToken}`, csrfToken: body.csrfToken };
}

async function api(method, path, { cookie, csrfToken, body } = {}) {
  const headers = { cookie };
  if (body !== undefined) headers['content-type'] = 'application/json';
  if (method !== 'GET') headers['x-csrf-token'] = csrfToken;
  return fetch(`${API}${path}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
}

async function expectJson(res, status, label) {
  if (res.status !== status) {
    fail(`${label} failed: ${res.status} ${await res.text()}`);
  }
  return res.json();
}

async function disableAllAgents(auth) {
  const agents = await expectJson(await api('GET', '/api/agents', auth), 200, 'list agents');
  const list = Array.isArray(agents) ? agents : agents.items ?? [];
  const enabled = list.filter((a) => a.enabled);
  for (const agent of enabled) {
    await expectJson(
      await api('PATCH', `/api/agents/${agent.id}`, {
        ...auth,
        body: { enabled: false },
      }),
      200,
      `disable agent ${agent.id}`,
    );
  }
  if (enabled.length > 0) {
    console.log(`smoke: disabled ${enabled.length} pre-existing agent(s)`);
  }
}

async function createMockAgent(presetKey, name, auth) {
  const presets = await expectJson(
    await api('GET', '/api/agent-presets', auth),
    200,
    'list presets',
  );
  const preset = presets.items?.find((item) => item.key === presetKey);
  if (!preset?.id) fail(`preset ${presetKey} not found`);

  const agent = await expectJson(
    await api('POST', '/api/agents', {
      ...auth,
      body: { name, presetId: preset.id, connector: 'mock' },
    }),
    201,
    `create ${presetKey}`,
  );
  if (agent.connector !== 'mock') {
    fail(`expected mock connector for ${presetKey}, got ${agent.connector}`);
  }
  console.log(`smoke: created ${presetKey} agent ${agent.id}`);
  return agent;
}

function removeInstalledPluginDir(name) {
  try {
    execFileSync(
      'docker',
      [
        'compose',
        '-f',
        COMPOSE_FILE,
        'exec',
        '-T',
        'server',
        'sh',
        '-c',
        `rm -rf "/data/plugins/${name}"`,
      ],
      { stdio: ['ignore', 'pipe', 'pipe'] },
    );
  } catch (err) {
    // Volume may already be empty on first run.
    console.log(`smoke: remove ${name} (ok if missing): ${err.message}`);
  }
}

async function pollBuild(id, auth, wantStatus) {
  const deadline = Date.now() + POLL_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const build = await expectJson(
      await api('GET', `/api/plugin-builds/${id}`, auth),
      200,
      'get plugin build',
    );
    if (build.status === wantStatus) return build;
    if (build.status === 'cancelled' || build.status === 'discarded') {
      fail(`build ended as ${build.status}`);
    }
    if (
      build.status === 'building' &&
      Array.isArray(build.checkErrors) &&
      build.checkErrors.length > 0 &&
      build.formatRetries >= 2
    ) {
      fail(`format check failed: ${JSON.stringify(build.checkErrors)}`);
    }
    await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
  }
  fail(`timed out waiting for build status ${wantStatus}`);
}

async function main() {
  console.log(`smoke: waiting for ${API}/health (builder phase)`);
  await waitForHealth();
  await bootstrapIfNeeded();
  const auth = await login();
  const suffix = Date.now().toString(36);

  // Oldest enabled implementer/planner wins; keep only this run's mock agents.
  await disableAllAgents(auth);
  await createMockAgent('pm', `M10 Builder PM ${suffix}`, auth);
  await createMockAgent('backend_engineer', `M10 Builder BE ${suffix}`, auth);

  removeInstalledPluginDir(PLUGIN_NAME);
  await expectJson(await api('POST', '/api/plugins/rescan', auth), 200, 'rescan');

  const created = await expectJson(
    await api('POST', '/api/plugin-builds', auth),
    201,
    'create plugin build',
  );
  const buildId = created.id;
  console.log(`smoke: plugin build ${buildId}`);

  await expectJson(
    await api('PATCH', `/api/plugin-builds/${buildId}`, {
      ...auth,
      body: {
        answers: {
          purpose: 'M10 builder smoke skills-only plugin',
          jobs: ['Say hello via a skill'],
          tools: ['none'],
          secrets: 'none',
          where: 'local',
        },
      },
    }),
    200,
    'save answers',
  );

  const planned = await expectJson(
    await api('POST', `/api/plugin-builds/${buildId}/plan-runs`, {
      ...auth,
      body: {},
    }),
    202,
    'start plan run',
  );
  console.log(`smoke: plan run ${planned.runId}`);
  const afterPlan = await pollBuild(buildId, auth, 'plan_ready');
  if (!afterPlan.plans?.[0]?.body) fail('plan_ready without plan body');
  console.log('smoke: plan_ready');

  const built = await expectJson(
    await api('POST', `/api/plugin-builds/${buildId}/build-runs`, {
      ...auth,
      body: {},
    }),
    202,
    'start build run',
  );
  console.log(`smoke: build run ${built.runId}`);
  const ready = await pollBuild(buildId, auth, 'ready_to_try');

  if (!ready.pluginId) fail('ready_to_try without pluginId');
  if (ready.pluginName !== PLUGIN_NAME) {
    fail(`expected pluginName ${PLUGIN_NAME}, got ${ready.pluginName}`);
  }
  if (!ready.testResult || !Array.isArray(ready.testResult.servers)) {
    fail('ready_to_try missing testResult.servers');
  }
  if (ready.testResult.servers.length !== 0) {
    fail('skills-only test should report empty servers');
  }

  const plugin = await expectJson(
    await api('GET', `/api/plugins/${ready.pluginId}`, auth),
    200,
    'get installed plugin',
  );
  if (plugin.enabled !== false) fail('plugin must be disabled before Accept');
  console.log(`smoke: installed disabled plugin ${plugin.id}; testResult ok`);

  const tested = await expectJson(
    await api('POST', `/api/plugins/${ready.pluginId}/test`, auth),
    200,
    'test plugin',
  );
  if (!Array.isArray(tested.servers) || tested.servers.length !== 0) {
    fail('skills-only Test should return empty servers');
  }
  console.log('smoke: Test button ok (empty servers)');

  const accepted = await expectJson(
    await api('POST', `/api/plugin-builds/${buildId}/accept`, {
      ...auth,
      body: { enable: true },
    }),
    200,
    'accept build',
  );
  if (accepted.status !== 'accepted') fail(`expected accepted, got ${accepted.status}`);

  const enabled = await expectJson(
    await api('GET', `/api/plugins/${ready.pluginId}`, auth),
    200,
    'get enabled plugin',
  );
  if (enabled.enabled !== true) fail('plugin not enabled after Accept');

  console.log('smoke: M10 plugin-builder happy path passed');
}

main().catch((err) => {
  fail(err instanceof Error ? err.message : String(err));
});
