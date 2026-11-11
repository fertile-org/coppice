#!/usr/bin/env node
/**
 * Stills for the English article notes. Same stack as `make screenshot`.
 *
 * Writes PNG files and mapping.json under COPPICE_ARTICLE_SHOTS
 * (default e2e/screenshot/out/pr-214). The Claude Code and Codex binaries
 * are the fixtures in fixtures/bin, copied into the server container only.
 * They are not installed on the host. The frames are demo data.
 */

import { lstat, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { composeInvocation } from './compose-cli.mjs';
import { resetMarketingWorkspace } from './seed.mjs';

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const WEB = process.env.COPPICE_WEB_URL ?? 'http://localhost:5001';
const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '../..');
const OUT = process.env.COPPICE_ARTICLE_SHOTS
  ? path.resolve(process.env.COPPICE_ARTICLE_SHOTS)
  : path.join(HERE, 'out', 'pr-214');
const PROJECT = process.env.COPPICE_COMPOSE_PROJECT ?? 'coppice-screenshot';
const REPO_PATH = '/tmp/coppice-screenshot-repo';
const VIEWPORT = { width: 1440, height: 900 };
const BOARD_NAME = 'Demo';
const REPO_NAME = 'Demo';
const TICKET_TITLE = 'Note a setting in the demo checkout';

const KNOWLEDGE_TITLE = 'Settings theme controls use the existing theme hook';
const KNOWLEDGE_BODY =
  'A dark mode toggle in Settings uses the theme hook the rest of the app already uses. Keep the control from storing its own theme. Include a settings test.';

const SHOTS = [
  {
    file: 'add-note.png',
    article: 'website/src/content/blog/en/stop-repeating-mistakes.md',
    alt: 'Add knowledge dialog for the acme-web board. The note says a dark mode toggle in Settings uses the existing theme hook and includes a settings test.',
    caption: 'The note names the theme hook, on the acme-web board.',
  },
  {
    file: 'pending-note.png',
    article: 'website/src/content/blog/en/stop-repeating-mistakes.md',
    alt: 'Pending Knowledge note on the acme-web board, titled Settings theme controls use the existing theme hook, with Approve.',
    caption: 'The note stays pending until you approve it.',
  },
  {
    file: 'connectors.png',
    article: 'website/src/content/blog/en/claude-code-and-codex.md',
    alt: 'Connectors list with Claude Code and Codex marked Ready.',
    caption: 'Demo: both CLIs pass the local sign-in check.',
  },
  {
    file: 'new-agent.png',
    article: 'website/src/content/blog/en/claude-code-and-codex.md',
    alt: 'New agent form with a role, Claude Code or Codex, and a model.',
    caption: 'Demo: one agent, one connector. Pick the model for this role.',
  },
  {
    file: 'ticket-drawer.png',
    article: 'website/src/content/blog/en/claude-code-and-codex.md',
    alt: 'Ticket with one assignee, a demo repository, Skip planning, and Run Agent.',
    caption: 'Demo: one assignee. Run Agent starts the PM when Skip planning is off.',
  },
  {
    file: 'done-card.png',
    article: 'website/src/content/blog/en/claude-code-and-codex.md',
    alt: 'Done card marked Not merged, with a separate Merge button.',
    caption: 'Demo: Accept and Merge are separate. Not a Claude Code or Codex result.',
  },
];

function fail(message) {
  console.error(`screenshot: ${message}`);
  process.exit(1);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function run(invocation, { input, cwd = REPO_ROOT } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(invocation.bin, invocation.args, {
      cwd,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code !== 0) {
        reject(new Error(`${invocation.bin} ${invocation.args.join(' ')} failed (${code}): ${stderr || stdout}`));
        return;
      }
      resolve(stdout);
    });
    if (input !== undefined) child.stdin.end(input);
    else child.stdin.end();
  });
}

function composeArgs(extra) {
  return composeInvocation(extra, { project: PROJECT });
}

function serverUser() {
  const uid = process.env.COPPICE_UID ?? String(process.getuid?.() ?? 1000);
  const gid = process.env.COPPICE_GID ?? String(process.getgid?.() ?? 1000);
  return `${uid}:${gid}`;
}

async function serverSh(script, { user } = {}) {
  const args = ['exec', '-T'];
  if (user) args.push('-u', user);
  args.push('server', 'sh', '-s');
  return run(composeArgs(args), { input: script });
}

async function psql(sql) {
  await run(
    composeArgs(['exec', '-T', 'postgres', 'psql', '-U', 'coppice', '-d', 'coppice', '-v', 'ON_ERROR_STOP=1', '-q']),
    { input: sql },
  );
}

function sqlQuote(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

async function waitForOk(url, label) {
  for (let attempt = 1; attempt <= 90; attempt += 1) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // not ready yet
    }
    await sleep(1000);
  }
  fail(`${label} not ready at ${url}`);
}

function parseSessionCookie(setCookieHeaders) {
  for (const header of setCookieHeaders) {
    const match = /coppice_session=([^;]+)/.exec(header);
    if (match) return match[1];
  }
  return null;
}

async function desktopSession() {
  const res = await fetch(`${API}/api/auth/desktop-session`, { method: 'POST' });
  if (!res.ok) fail(`desktop-session failed: ${res.status} ${await res.text()}`);
  const setCookie =
    typeof res.headers.getSetCookie === 'function'
      ? res.headers.getSetCookie()
      : [res.headers.get('set-cookie')].filter(Boolean);
  const sessionToken = parseSessionCookie(setCookie);
  if (!sessionToken) fail('desktop-session did not return coppice_session');
  const body = await res.json();
  if (!body.csrfToken) fail('desktop-session did not return csrfToken');
  return { cookie: `coppice_session=${sessionToken}`, csrfToken: body.csrfToken };
}

async function api(method, apiPath, auth, body) {
  const headers = { cookie: auth.cookie };
  if (body !== undefined) headers['content-type'] = 'application/json';
  if (method !== 'GET') headers['x-csrf-token'] = auth.csrfToken;
  const res = await fetch(`${API}${apiPath}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) fail(`${method} ${apiPath} failed: ${res.status} ${await res.text()}`);
  if (res.status === 204) return null;
  const text = await res.text();
  if (!text) return null;
  return JSON.parse(text);
}

async function publishConfig(text) {
  const file = process.env.COPPICE_SCREENSHOT_CONFIG;
  if (!file) fail('COPPICE_SCREENSHOT_CONFIG is unset');
  const stat = await lstat(file);
  if (stat.isSymbolicLink() || !stat.isFile()) fail(`refusing screenshot config: ${file}`);
  await writeFile(file, text);
  if (process.env.COPPICE_SCREENSHOT_TRANSFER === 'copy') {
    await run(composeArgs(['cp', file, 'server:/etc/coppice/config.toml']));
  }
}

async function enableDemoConnectors() {
  const file = process.env.COPPICE_SCREENSHOT_CONFIG;
  const text = await readFile(file, 'utf8');
  let next = text.replace(/(\[agent\.connectors\.codex\]\r?\nenabled = )false\b/, '$1true');
  next = next.replace(/(\[agent\.connectors\.claude-code\]\r?\nenabled = )false\b/, '$1true');
  if (!/\[agent\.connectors\.claude-code\]\r?\nenabled = true\b/.test(next)) {
    fail('could not enable claude-code in the private config');
  }
  if (!/\[agent\.connectors\.codex\]\r?\nenabled = true\b/.test(next)) {
    fail('could not enable codex in the private config');
  }
  await publishConfig(next);
  await serverSh(
    `
    set -eu
    mkdir -p /home/coppice/.local/bin
    chown -R ${serverUser()} /home/coppice/.local
    `,
    { user: '0:0' },
  );
  await serverSh('rm -f /home/coppice/.local/bin/cursor-agent', { user: '0:0' });
  for (const name of ['claude', 'codex']) {
    const bin = path.join(HERE, 'fixtures/bin', name);
    await run(composeArgs(['cp', bin, `server:/home/coppice/.local/bin/${name}`]));
    await serverSh(
      `
      set -eu
      chown ${serverUser()} /home/coppice/.local/bin/${name}
      chmod 755 /home/coppice/.local/bin/${name}
      `,
      { user: '0:0' },
    );
  }
  await run(composeArgs(['restart', 'server']));
  await waitForOk(`${API}/health`, 'API');
  console.log('screenshot: demo claude and codex fixtures are in the container');
}

async function waitReady(auth, id) {
  for (let attempt = 1; attempt <= 30; attempt += 1) {
    const listed = await api('GET', '/api/connectors', auth);
    const item = (listed.items ?? []).find((row) => row.id === id);
    if (item?.readiness === 'ready') return;
    await sleep(1000);
  }
  fail(`${id} did not become ready`);
}

async function demoGit(mode, short) {
  const script = await readFile(path.join(HERE, 'seed-demo-repo.sh'), 'utf8');
  const args = ['exec', '-T', '-u', serverUser(), 'server', 'sh', '-s', '--', mode];
  if (short) args.push(short);
  return run(composeArgs(args), { input: script });
}

async function heroGit(mode) {
  const script = await readFile(path.join(HERE, 'seed-hero-repo.sh'), 'utf8');
  const args = ['exec', '-T', '-u', serverUser(), 'server', 'sh', '-s', '--', mode];
  return run(composeArgs(args), { input: script });
}

async function seedScene(auth) {
  await resetMarketingWorkspace();
  const existing = await api('GET', '/api/agents', auth);
  for (const agent of existing.items ?? []) {
    await api('DELETE', `/api/agents/${agent.id}`, auth);
  }
  const presets = await api('GET', '/api/agent-presets', auth);
  const pm = (presets.items ?? []).find((preset) => preset.key === 'pm');
  if (!pm) fail('PM preset is missing');
  const agent = await api('POST', '/api/agents', auth, {
    name: 'PM',
    presetId: pm.id,
    connector: 'claude-code',
    enabled: true,
  });
  await demoGit('init');
  const repo = await api('POST', '/api/repos', auth, {
    name: REPO_NAME,
    localPath: REPO_PATH,
    defaultBranch: 'main',
  });
  if (repo.verificationStatus !== 'ready') {
    fail(`repo is not ready: ${repo.verificationStatus} ${repo.verificationError ?? ''}`);
  }
  const board = await api('POST', '/api/boards', auth, { name: BOARD_NAME });
  const ticket = await api('POST', `/api/boards/${board.id}/tickets`, auth, {
    title: TICKET_TITLE,
    description: 'A demo note for the article stills.',
    priority: 'medium',
  });
  await api('POST', `/api/tickets/${ticket.id}/assign`, auth, { agentId: agent.id });
  await api('PATCH', `/api/tickets/${ticket.id}`, auth, { repoId: repo.id });
  return { agent, board, ticket, repo };
}

async function launchBrowser() {
  try {
    return await chromium.launch({ headless: true });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.log(`screenshot: bundled Chromium failed (${message.split('\n')[0]}). Trying Google Chrome.`);
    return chromium.launch({ channel: 'chrome', headless: true });
  }
}

async function assertClean(page, label) {
  const bodyText = await page.locator('body').innerText();
  if (/sign out/i.test(bodyText) || bodyText.includes('admin@localhost')) {
    fail(`${label} still shows account chrome`);
  }
  if (/\/home\/|\/Users\/|should-not-appear|sk-/.test(bodyText)) {
    fail(`${label} shows a host path or secret`);
  }
  if (/\bacme\b/i.test(bodyText)) fail(`${label} shows acme`);
  if (/\bmock\b/i.test(bodyText)) fail(`${label} shows Mock`);
}

async function preparePage(page, label) {
  await page.evaluate(() => document.fonts.ready);
  await page.addStyleTag({
    content:
      'html,body{overflow:hidden!important}*{scrollbar-width:none!important}*{caret-color:transparent!important}::-webkit-scrollbar{display:none!important}',
  });
  await assertClean(page, label);
}

async function shoot(page, file) {
  const dest = path.join(OUT, file);
  await page.screenshot({ path: dest, type: 'png', animations: 'disabled' });
  console.log(`screenshot: wrote ${dest}`);
}

async function hideMockRow(page) {
  await page.evaluate(() => {
    for (const row of document.querySelectorAll('li')) {
      const text = row.innerText ?? '';
      if (/^Mock\b/.test(text.trim())) row.remove();
    }
  });
}

async function captureConnectors(page) {
  await page.goto(`${WEB}/system/connectors`, { waitUntil: 'domcontentloaded' });
  await page.getByText('Claude Code', { exact: false }).first().waitFor({ timeout: 30_000 });
  await page.getByText('Codex', { exact: true }).first().waitFor();
  await hideDevMock(page);
  const claude = page.getByRole('button', { name: /Claude Code/ });
  const codex = page.getByRole('button', { name: /^Codex\b/ });
  await claude.waitFor();
  await codex.waitFor();
  await claude.evaluate((node) => node.scrollIntoView({ block: 'center' }));
  await preparePage(page, 'connectors');
  const claudeText = await claude.innerText();
  const codexText = await codex.innerText();
  if (!claudeText.includes('Ready') || !codexText.includes('Ready')) {
    fail(`connector rows are not both Ready:\n${claudeText}\n${codexText}`);
  }
  await shoot(page, 'connectors.png');
}

async function selectCombobox(page, id, optionName) {
  await page.locator(`#${id}`).click();
  await page.getByRole('option', { name: optionName, exact: true }).click();
  await page.getByRole('listbox').waitFor({ state: 'hidden' }).catch(() => {});
}

async function hideDevMock(page) {
  // The Compose image is built with mock-provider. Desktop builds omit it.
  // Remove that row so the still matches the desktop product.
  await page.evaluate(() => {
    for (const el of document.querySelectorAll('li, button, [role="option"]')) {
      const text = (el.innerText || el.textContent || '').trim();
      if (/^Mock\b/.test(text)) el.remove();
    }
  });
}

async function captureNewAgent(page) {
  await page.goto(`${WEB}/agents`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'New agent' }).click();
  await page.getByRole('heading', { name: 'New agent' }).waitFor();
  await selectCombobox(page, 'agent-preset', 'pm — PM');
  await page.locator('#agent-name').fill('PM');
  await page.waitForFunction(() => {
    const role = document.getElementById('agent-role');
    return role instanceof HTMLInputElement && role.readOnly && role.value === 'PM';
  });
  await selectCombobox(page, 'agent-connector', 'Claude Code — Ready');
  await page.locator('#agent-model').waitFor();
  await page.locator('#agent-system-prompt').evaluate((node) => {
    node.style.height = '4.5rem';
  });
  await page.locator('#agent-preset').evaluate((node) => node.scrollIntoView({ block: 'start' }));
  const fitted = await page.evaluate(() => {
    const dialog = document.querySelector('[role="dialog"]');
    const ids = ['agent-preset', 'agent-role', 'agent-connector', 'agent-model'];
    if (!dialog) return false;
    const box = dialog.getBoundingClientRect();
    return ids.every((id) => {
      const node = document.getElementById(id);
      if (!node) return false;
      const rect = node.getBoundingClientRect();
      return rect.top >= box.top - 1 && rect.bottom <= box.bottom + 1;
    });
  });
  if (!fitted) fail('new agent form does not show the preset, role, connector, and model together');
  await hideDevMock(page);
  await preparePage(page, 'new agent');
  await shoot(page, 'new-agent.png');
}

async function captureDrawer(page, scene) {
  await page.goto(`${WEB}/boards/${scene.board.id}`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: BOARD_NAME, level: 1 }).waitFor({ timeout: 30_000 });
  await page.getByText(TICKET_TITLE).click();
  const detail = page.locator('[aria-label="Ticket detail"]');
  await detail.getByRole('button', { name: 'Run Agent' }).waitFor({ timeout: 15_000 });
  await detail.getByText('Skip planning').waitFor();
  await detail.getByText(REPO_NAME, { exact: true }).first().waitFor();
  await preparePage(page, 'ticket drawer');
  await shoot(page, 'ticket-drawer.png');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
}

async function showDone(page) {
  await page.locator('#board').evaluate((board) => {
    const column = [...board.querySelectorAll('[role="region"]')].find(
      (el) => el.getAttribute('aria-label') === 'Done',
    );
    if (!column) return;
    const boardRect = board.getBoundingClientRect();
    const colRect = column.getBoundingClientRect();
    board.scrollLeft += colRect.left - boardRect.left - 16;
  });
}

async function assertKnowledgeClean(page, label) {
  const bodyText = await page.locator('body').innerText();
  if (/sign out/i.test(bodyText) || bodyText.includes('admin@localhost')) {
    fail(`${label} still shows account chrome`);
  }
  if (/\/home\/|\/Users\/|should-not-appear|sk-/.test(bodyText)) {
    fail(`${label} shows a host path or secret`);
  }
  if (/\bmock\b/i.test(bodyText)) fail(`${label} shows Mock`);
  if (!bodyText.includes('acme-web')) fail(`${label} is missing acme-web`);
}

async function prepareKnowledge(page, label) {
  await page.evaluate(() => document.fonts.ready);
  await page.addStyleTag({
    content:
      'html,body{overflow:hidden!important}*{scrollbar-width:none!important}*{caret-color:transparent!important}::-webkit-scrollbar{display:none!important}',
  });
  await assertKnowledgeClean(page, label);
}

async function seedKnowledgeScene(auth) {
  await resetMarketingWorkspace();
  await heroGit('init');
  const repo = await api('POST', '/api/repos', auth, {
    name: 'acme-web',
    localPath: REPO_PATH,
    defaultBranch: 'main',
  });
  if (repo.verificationStatus !== 'ready') {
    fail(`repo is not ready: ${repo.verificationStatus} ${repo.verificationError ?? ''}`);
  }
  const board = await api('POST', '/api/boards', auth, { name: 'acme-web' });
  await api('POST', `/api/boards/${board.id}/tickets`, auth, {
    title: 'Add a dark mode toggle to Settings',
    description:
      'Settings should offer a dark mode toggle and follow the same theme preference as the rest of the app.',
    priority: 'medium',
  });
  return { board, repo };
}

async function captureKnowledge(page, auth) {
  await seedKnowledgeScene(auth);
  await page.goto(`${WEB}/knowledge`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: 'Knowledge', level: 1 }).waitFor({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Add knowledge' }).first().click();
  const dialog = page.getByRole('dialog', { name: 'Add knowledge' });
  await dialog.waitFor();
  await dialog.locator('#knowledge-title').fill(KNOWLEDGE_TITLE);
  await dialog.locator('#knowledge-content').fill(KNOWLEDGE_BODY);
  await selectCombobox(page, 'knowledge-type', 'Review feedback');
  await selectCombobox(page, 'knowledge-board', 'acme-web');
  await dialog.getByText('acme-web', { exact: true }).waitFor();
  await prepareKnowledge(page, 'add knowledge');
  await shoot(page, 'add-note.png');
  await dialog.getByRole('button', { name: 'Add to Pending' }).click();
  await dialog.waitFor({ state: 'hidden', timeout: 15_000 });
  const toast = page.getByText('Knowledge added to Pending.');
  await toast.waitFor({ timeout: 10_000 });
  await toast.waitFor({ state: 'hidden', timeout: 10_000 });
  const card = page.getByRole('article').filter({ hasText: KNOWLEDGE_TITLE });
  await card.waitFor({ timeout: 15_000 });
  await card.getByText('No close matches').waitFor({ timeout: 15_000 });
  await card.getByRole('button', { name: 'Approve', exact: true }).waitFor();
  await card.evaluate((node) => node.scrollIntoView({ block: 'start' }));
  await prepareKnowledge(page, 'pending knowledge');
  if (!(await card.getByText('acme-web', { exact: true }).count())) {
    fail('pending note is not scoped to acme-web');
  }
  await shoot(page, 'pending-note.png');
}

async function captureDone(page, scene) {
  const short = scene.ticket.id.split('-')[0];
  await demoGit('worktree', short);
  await psql(`
    UPDATE tickets
    SET branch_name = ${sqlQuote(`agent/TICKET-${short}`)}
    WHERE id = ${sqlQuote(scene.ticket.id)}::uuid;
  `);
  await api('PATCH', `/api/tickets/${scene.ticket.id}/status`, scene.auth, {
    status: 'wait_for_final_review',
  });
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: BOARD_NAME, level: 1 }).waitFor({ timeout: 30_000 });
  await page.getByRole('region', { name: 'Wait for Human Review' }).getByText(TICKET_TITLE).click();
  const detail = page.locator('[aria-label="Ticket detail"]');
  await detail.getByRole('button', { name: 'Accept', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Accept this ticket?' });
  await dialog.getByRole('button', { name: 'Accept', exact: true }).click();
  await dialog.waitFor({ state: 'hidden', timeout: 15_000 });
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  const done = page.getByRole('region', { name: 'Done' });
  await done.getByText(TICKET_TITLE).waitFor({ timeout: 15_000 });
  await done.getByText('Not merged', { exact: true }).waitFor({ timeout: 15_000 });
  await done.getByRole('button', { name: 'Merge', exact: true }).waitFor();
  await showDone(page);
  await preparePage(page, 'done card');
  await shoot(page, 'done-card.png');
}

async function main() {
  await mkdir(OUT, { recursive: true });
  await waitForOk(`${API}/health`, 'API');
  await waitForOk(WEB, 'web');
  await enableDemoConnectors();
  const auth = await desktopSession();
  await waitReady(auth, 'claude-code');
  await waitReady(auth, 'codex');
  const scene = await seedScene(auth);
  scene.auth = auth;
  const browser = await launchBrowser();
  try {
    const context = await browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: 1,
      colorScheme: 'light',
      reducedMotion: 'reduce',
      locale: 'en-US',
      timezoneId: 'UTC',
    });
    await context.addInitScript(() => {
      localStorage.setItem('coppice.theme', 'light');
      localStorage.setItem('coppice.sidebar.collapsed', '0');
    });
    const page = await context.newPage();
    await captureConnectors(page);
    await captureNewAgent(page);
    await captureDrawer(page, scene);
    await captureDone(page, scene);
    await captureKnowledge(page, auth);
  } finally {
    await browser.close();
  }
  await writeFile(path.join(OUT, 'mapping.json'), `${JSON.stringify({ shots: SHOTS }, null, 2)}\n`);
  console.log(`screenshot: mapping ${path.join(OUT, 'mapping.json')}`);
}

async function browserPage() {
  const browser = await launchBrowser();
  const context = await browser.newContext({
    viewport: VIEWPORT,
    deviceScaleFactor: 1,
    colorScheme: 'light',
    reducedMotion: 'reduce',
    locale: 'en-US',
    timezoneId: 'UTC',
  });
  await context.addInitScript(() => {
    localStorage.setItem('coppice.theme', 'light');
    localStorage.setItem('coppice.sidebar.collapsed', '0');
  });
  const page = await context.newPage();
  return { browser, page };
}

async function mainKnowledgeOnly() {
  await mkdir(OUT, { recursive: true });
  await waitForOk(`${API}/health`, 'API');
  await waitForOk(WEB, 'web');
  const auth = await desktopSession();
  const { browser, page } = await browserPage();
  try {
    await captureKnowledge(page, auth);
  } finally {
    await browser.close();
  }
  const shots = SHOTS.filter((shot) => shot.article.endsWith('stop-repeating-mistakes.md'));
  await writeFile(path.join(OUT, 'mapping.json'), `${JSON.stringify({ shots }, null, 2)}\n`);
  console.log(`screenshot: mapping ${path.join(OUT, 'mapping.json')}`);
}

const entry = process.env.COPPICE_ARTICLE_ONLY === 'knowledge' ? mainKnowledgeOnly : main;
entry().catch((err) => {
  fail(err instanceof Error ? err.stack ?? err.message : String(err));
});
