#!/usr/bin/env node
/**
 * M10 plugin MCP smoke test.
 *
 * Adds the fixture plugin dir, saves the plugin setting, runs the MCP Test,
 * enables the plugin, assigns it to a new agent, and runs a mock ticket that
 * calls a core tool, a plugin skill, a plugin MCP tool, and `result_submit`.
 * Requires the server to run with MOCK_AGENT_RESPONSE=mcp/m10_smoke.
 *
 * Env:
 *   COPPICE_API_URL            default http://localhost:5000
 *   COPPICE_BOOTSTRAP_PASSWORD default changeme
 *   COPPICE_SMOKE_EMAIL        default admin@localhost
 *   COPPICE_SMOKE_PASSWORD     default changeme
 *   COPPICE_SMOKE_REPO_PATH    default /tmp/smoke-repo (path inside server container)
 *   COPPICE_SMOKE_PLUGIN_DIR   default /app/fixtures/plugins/m10-smoke (inside server container)
 */

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const BOOTSTRAP_PASSWORD =
  process.env.COPPICE_BOOTSTRAP_PASSWORD ?? 'changeme';
const EMAIL = process.env.COPPICE_SMOKE_EMAIL ?? 'admin@localhost';
const PASSWORD = process.env.COPPICE_SMOKE_PASSWORD ?? 'changeme';
const SMOKE_REPO_PATH =
  process.env.COPPICE_SMOKE_REPO_PATH ?? '/tmp/smoke-repo';
const PLUGIN_DIR =
  process.env.COPPICE_SMOKE_PLUGIN_DIR ?? '/app/fixtures/plugins/m10-smoke';
const PLUGIN_NAME = 'm10-smoke';
const PLUGIN_TOOL = 'm10-smoke__echo';
const PLUGIN_SKILL = 'm10-smoke:greet';
const EXPECTED_TOOLS = ['ticket_get', 'skill_load', PLUGIN_TOOL, 'result_submit'];

const MAX_HEALTH_ATTEMPTS = 60;
const HEALTH_INTERVAL_MS = 1000;
const RUN_POLL_TIMEOUT_MS = 60_000;
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

async function ensurePluginDir(auth) {
  const res = await api('POST', '/api/plugin-dirs', {
    ...auth,
    body: { path: PLUGIN_DIR },
  });
  if (res.status === 201) {
    const dir = await res.json();
    console.log(`smoke: added plugin dir ${dir.id} at ${PLUGIN_DIR}`);
    return;
  }
  if (res.status === 409) {
    const dirs = await expectJson(
      await api('GET', '/api/plugin-dirs', auth),
      200,
      'list plugin dirs',
    );
    const existing = dirs.find((dir) => dir.path === PLUGIN_DIR);
    if (existing) {
      console.log(`smoke: reusing plugin dir ${existing.id}`);
      return;
    }
  }
  fail(`add plugin dir failed: ${res.status} ${await res.text()}`);
}

async function rescanAndFindPlugin(auth) {
  const plugins = await expectJson(
    await api('POST', '/api/plugins/rescan', auth),
    200,
    'rescan plugins',
  );
  const plugin = plugins.find(
    (p) => p.name === PLUGIN_NAME && p.status === 'ok',
  );
  if (!plugin) {
    const seen = plugins.map((p) => `${p.name}:${p.status}`).join(', ');
    fail(`plugin ${PLUGIN_NAME} not found with status ok after rescan (saw ${seen})`);
  }
  if (!plugin.skills?.some((s) => s.name === 'greet')) {
    fail(`plugin ${PLUGIN_NAME} is missing skill greet`);
  }
  const server = plugin.mcpServers?.find((s) => s.name === 'echo');
  if (server?.kind !== 'stdio') {
    fail(`plugin ${PLUGIN_NAME} is missing stdio server echo`);
  }
  if (!plugin.settings?.some((s) => s.key === 'GREETING')) {
    fail(`plugin ${PLUGIN_NAME} does not list setting GREETING`);
  }
  console.log(`smoke: found plugin ${plugin.id}`);
  return plugin;
}

async function saveSetting(pluginId, auth) {
  const plugin = await expectJson(
    await api('PUT', `/api/plugins/${pluginId}/settings`, {
      ...auth,
      body: { values: { GREETING: 'hello' } },
    }),
    200,
    'save plugin settings',
  );
  const setting = plugin.settings?.find((s) => s.key === 'GREETING');
  if (setting?.configured !== true || setting?.source !== 'setting') {
    fail('setting GREETING not reported as configured');
  }
  if (Object.keys(setting).sort().join(',') !== 'configured,key,source') {
    fail('plugin settings response exposes more than key, configured, and source');
  }
  console.log('smoke: saved setting GREETING');
}

async function testPlugin(pluginId, auth) {
  const body = await expectJson(
    await api('POST', `/api/plugins/${pluginId}/test`, auth),
    200,
    'test plugin',
  );
  const server = body.servers?.find((s) => s.name === 'echo');
  if (!server) {
    fail('plugin test did not report server echo');
  }
  if (server.status !== 'ok') {
    fail(`plugin test server echo status ${server.status}: ${server.error ?? ''}`);
  }
  const tool = server.tools?.find((t) => t.exposedName === PLUGIN_TOOL);
  if (!tool) {
    fail(`plugin test did not list tool ${PLUGIN_TOOL}`);
  }
  if (tool.readOnly !== true) {
    fail(`tool ${PLUGIN_TOOL} not reported read-only`);
  }
  console.log(`smoke: plugin test ok; lists ${PLUGIN_TOOL}`);
}

async function enablePlugin(pluginId, auth) {
  const plugin = await expectJson(
    await api('PATCH', `/api/plugins/${pluginId}`, {
      ...auth,
      body: { enabled: true },
    }),
    200,
    'enable plugin',
  );
  if (plugin.enabled !== true) {
    fail('plugin not enabled after PATCH');
  }
  console.log('smoke: enabled plugin');
}

async function createAgent(pluginId, suffix, auth) {
  const presets = await expectJson(
    await api('GET', '/api/agent-presets', auth),
    200,
    'list agent presets',
  );
  const preset = presets.items?.find((item) => item.key === 'backend_engineer');
  if (!preset?.id) {
    fail('backend_engineer preset not found');
  }
  const agent = await expectJson(
    await api('POST', '/api/agents', {
      ...auth,
      body: { name: `M10 Smoke Agent ${suffix}`, presetId: preset.id },
    }),
    201,
    'create agent',
  );
  const assigned = await expectJson(
    await api('PUT', `/api/agents/${agent.id}/plugins`, {
      ...auth,
      body: { pluginIds: [pluginId] },
    }),
    200,
    'assign plugin to agent',
  );
  if (!assigned.pluginIds?.includes(pluginId)) {
    fail('agent plugin assignment missing the smoke plugin');
  }
  console.log(`smoke: created agent ${agent.id} with plugin assigned`);
  return agent;
}

async function registerRepo(auth) {
  const res = await api('POST', '/api/repos', {
    ...auth,
    body: { name: 'smoke-repo', localPath: SMOKE_REPO_PATH, defaultBranch: 'main' },
  });
  if (res.status === 201) {
    const repo = await res.json();
    console.log(`smoke: registered repo ${repo.id} at ${SMOKE_REPO_PATH}`);
    return repo;
  }
  if (res.status === 409) {
    const repos = await expectJson(await api('GET', '/api/repos', auth), 200, 'list repos');
    const existing = repos.find((r) => r.localPath === SMOKE_REPO_PATH);
    if (existing?.id) {
      console.log(`smoke: reusing registered repo ${existing.id}`);
      return existing;
    }
  }
  fail(`register repo failed: ${res.status} ${await res.text()}`);
}

async function createTicket(repoId, agentId, suffix, auth) {
  const board = await expectJson(
    await api('POST', '/api/boards', { ...auth, body: { name: `M10 Smoke ${suffix}` } }),
    201,
    'create board',
  );
  const ticket = await expectJson(
    await api('POST', `/api/boards/${board.id}/tickets`, {
      ...auth,
      body: { title: `M10 smoke ticket ${suffix}`, description: 'Plugin MCP smoke test' },
    }),
    201,
    'create ticket',
  );
  const patched = await api('PATCH', `/api/tickets/${ticket.id}`, {
    ...auth,
    body: { repoId },
  });
  if (!patched.ok) {
    fail(`set ticket repo failed: ${patched.status} ${await patched.text()}`);
  }
  const assigned = await api('POST', `/api/tickets/${ticket.id}/assign`, {
    ...auth,
    body: { agentId },
  });
  if (!assigned.ok) {
    fail(`assign agent failed: ${assigned.status} ${await assigned.text()}`);
  }
  console.log(`smoke: created ticket ${ticket.id} on board ${board.id}`);
  return ticket;
}

async function runAgent(ticketId, auth) {
  const body = await expectJson(
    await api('POST', `/api/tickets/${ticketId}/run-agent`, auth),
    201,
    'run agent',
  );
  const runId = body.run?.id;
  if (!runId) {
    fail('run-agent response missing run.id');
  }
  console.log(`smoke: started run ${runId}`);
  return runId;
}

async function pollRunUntilSucceeded(runId, auth) {
  const deadline = Date.now() + RUN_POLL_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const body = await expectJson(
      await api('GET', `/api/agent-runs/${runId}`, auth),
      200,
      'get run',
    );
    const status = body.run?.status;
    if (status === 'succeeded') {
      console.log(`smoke: run ${runId} succeeded`);
      return;
    }
    if (status === 'failed' || status === 'blocked' || status === 'cancelled') {
      fail(`run ended with status ${status}: ${body.run?.errorMessage ?? ''}`);
    }
    await new Promise((resolve) => setTimeout(resolve, RUN_POLL_INTERVAL_MS));
  }
  fail(`timed out waiting for run to succeed after ${RUN_POLL_TIMEOUT_MS / 1000}s`);
}

async function assertToolCalls(runId, pluginId, auth) {
  const body = await expectJson(
    await api('GET', `/api/agent-runs/${runId}/tool-calls`, auth),
    200,
    'list tool calls',
  );
  const items = body.items;
  if (!Array.isArray(items)) {
    fail('tool-calls response missing items array');
  }
  const summary = items.map((c) => `${c.tool}:${c.status}`).join(', ');
  const tools = items.map((c) => c.tool);
  if (tools.join(',') !== EXPECTED_TOOLS.join(',')) {
    fail(`expected tool calls ${EXPECTED_TOOLS.join(', ')}; got ${summary}`);
  }
  const notOk = items.filter((c) => c.status !== 'ok');
  if (notOk.length > 0) {
    const detail = notOk.map((c) => `${c.tool}:${c.status} ${c.error ?? ''}`).join('; ');
    fail(`tool calls not ok: ${detail}`);
  }
  const echo = items.find((c) => c.tool === PLUGIN_TOOL);
  if (echo.source !== 'plugin') {
    fail(`${PLUGIN_TOOL} source ${echo.source}, expected plugin`);
  }
  if (echo.pluginId !== pluginId || echo.pluginName !== PLUGIN_NAME) {
    fail(`${PLUGIN_TOOL} not attributed to plugin ${PLUGIN_NAME}`);
  }
  const skills = body.skillsUsed ?? [];
  if (skills.join(',') !== PLUGIN_SKILL) {
    fail(`expected skillsUsed [${PLUGIN_SKILL}], got [${skills.join(', ')}]`);
  }
  console.log(`smoke: tool calls ${summary}; skillsUsed ${skills.join(', ')}`);
}

async function main() {
  console.log(`smoke: waiting for ${API}/health`);
  await waitForHealth();

  await bootstrapIfNeeded();
  const auth = await login();
  const suffix = Date.now().toString(36);

  await ensurePluginDir(auth);
  const plugin = await rescanAndFindPlugin(auth);
  await saveSetting(plugin.id, auth);
  await testPlugin(plugin.id, auth);
  await enablePlugin(plugin.id, auth);
  const agent = await createAgent(plugin.id, suffix, auth);
  const repo = await registerRepo(auth);
  const ticket = await createTicket(repo.id, agent.id, suffix, auth);
  const runId = await runAgent(ticket.id, auth);
  await pollRunUntilSucceeded(runId, auth);
  await assertToolCalls(runId, plugin.id, auth);

  console.log('smoke: M10 plugin MCP flow passed');
}

main().catch((err) => {
  fail(err instanceof Error ? err.message : String(err));
});
