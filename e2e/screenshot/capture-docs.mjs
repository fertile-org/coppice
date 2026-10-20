#!/usr/bin/env node
/**
 * Docs stills for the website guides.
 *
 * Same stack as the marketing capture (`make screenshot`): the
 * coppice-screenshot Compose project, desktop mode, and a 1440×900 light
 * frame. Installs a Cursor CLI that prints a stable model list, seeds one
 * PM agent, one repository, and one Backlog ticket, then writes WebP files
 * under website/public/assets/docs/.
 *
 * Run after the screenshot stack is up:
 *   node e2e/screenshot/capture-docs.mjs
 */

import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { resetMarketingWorkspace } from './seed.mjs';

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const WEB = process.env.COPPICE_WEB_URL ?? 'http://localhost:5001';
const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '../..');
const DOCS_ASSETS = path.join(REPO_ROOT, 'website', 'public', 'assets', 'docs');
const AGENT_BIN = path.join(HERE, 'fixtures', 'bin', 'agent');
const PROJECT = process.env.COPPICE_COMPOSE_PROJECT ?? 'coppice-screenshot';
const COMPOSE_FILES = [
  'deploy/docker-compose.yml',
  'deploy/docker-compose.screenshot.yml',
];
const REPO_PATH = '/tmp/coppice-screenshot-repo';
const VIEWPORT = { width: 1440, height: 900 };

const MAX_HEALTH_ATTEMPTS = 90;
const HEALTH_INTERVAL_MS = 1000;

const SHOTS = [
  {
    name: 'model-picker',
    alt: "The model picker on an agent, listing the models from the agent's CLI, with Refresh models below.",
  },
  {
    name: 'new-agent',
    alt: 'The new agent form with a name, a role and a CLI chosen.',
  },
  {
    name: 'backlog-ticket',
    alt: 'A board with a new ticket in Backlog and the Run agent button showing.',
  },
];

function fail(message) {
  console.error(`screenshot: ${message}`);
  process.exit(1);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function run(args, { input, cwd = REPO_ROOT } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn('docker', args, {
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
        reject(new Error(`docker ${args.join(' ')} failed (${code}): ${stderr || stdout}`));
        return;
      }
      resolve(stdout);
    });
    if (input !== undefined) child.stdin.end(input);
    else child.stdin.end();
  });
}

function composeArgs(extra) {
  const args = ['compose', '-p', PROJECT];
  for (const file of COMPOSE_FILES) args.push('-f', file);
  args.push(...extra);
  return args;
}

function serverUser() {
  const uid = process.env.COPPICE_UID ?? String(process.getuid?.() ?? 1000);
  const gid = process.env.COPPICE_GID ?? String(process.getgid?.() ?? 1000);
  return `${uid}:${gid}`;
}

function parseSessionCookie(setCookieHeaders) {
  for (const header of setCookieHeaders) {
    const match = /coppice_session=([^;]+)/.exec(header);
    if (match) return match[1];
  }
  return null;
}

async function waitForOk(url, label) {
  for (let attempt = 1; attempt <= MAX_HEALTH_ATTEMPTS; attempt += 1) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // not ready yet
    }
    await sleep(HEALTH_INTERVAL_MS);
  }
  fail(`${label} not ready at ${url} after ${MAX_HEALTH_ATTEMPTS}s`);
}

async function requireDesktopMode() {
  const res = await fetch(`${API}/api/auth/capabilities`);
  if (!res.ok) fail(`capabilities failed: ${res.status} ${await res.text()}`);
  const body = await res.json();
  if (body.desktopMode !== true) {
    fail(
      'server is not in desktop_mode (capabilities.desktopMode is false). Run `make screenshot` so the capture matches the Electron app.',
    );
  }
  console.log('screenshot: desktop_mode is on');
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
  if (!res.ok) {
    fail(`${method} ${apiPath} failed: ${res.status} ${await res.text()}`);
  }
  if (res.status === 204) return null;
  const text = await res.text();
  if (!text) return null;
  return JSON.parse(text);
}

async function serverSh(script, { user } = {}) {
  const args = ['exec', '-T'];
  if (user) args.push('-u', user);
  args.push('server', 'sh', '-s');
  return run(composeArgs(args), { input: script });
}

async function enableCursorInConfig() {
  // Saving an agent turns its connector on by rewriting config.toml. That
  // rewrite cannot replace the bind-mounted file, so Cursor is turned on
  // here before the server starts. The example leaves it off.
  const file = path.join(REPO_ROOT, 'deploy/config/config.toml');
  const text = await readFile(file, 'utf8');
  const next = text.replace(
    /(\[agent\.connectors\.cursor\]\r?\nenabled = )false\b/,
    '$1true',
  );
  if (!/\[agent\.connectors\.cursor\]\r?\nenabled = true\b/.test(next)) {
    fail('could not turn Cursor on in deploy/config/config.toml');
  }
  if (next !== text) await writeFile(file, next);
  console.log('screenshot: Cursor connector is on in config');
}

async function installCursorCli() {
  await serverSh(
    `
    set -eu
    mkdir -p /home/coppice/.local/bin
    chown -R ${serverUser()} /home/coppice/.local
    `,
    { user: '0:0' },
  );
  await run(composeArgs(['cp', AGENT_BIN, 'server:/home/coppice/.local/bin/cursor-agent']));
  await serverSh(
    `
    set -eu
    chown ${serverUser()} /home/coppice/.local/bin/cursor-agent
    chmod 755 /home/coppice/.local/bin/cursor-agent
    `,
    { user: '0:0' },
  );
  await enableCursorInConfig();
  console.log('screenshot: installed Cursor CLI into the server home');
  await run(composeArgs(['restart', 'server']));
  await waitForOk(`${API}/health`, 'API');
}

async function seedCheckout() {
  await serverSh(
    `
    set -eu
    rm -rf ${REPO_PATH}
    git init -b main ${REPO_PATH}
    git -C ${REPO_PATH} config user.email screenshot@coppice.local
    git -C ${REPO_PATH} config user.name "Coppice Screenshot"
    printf '%s\\n' '# Coppice' > ${REPO_PATH}/README.md
    git -C ${REPO_PATH} add README.md
    git -C ${REPO_PATH} commit -m "Initial commit"
    `,
    { user: serverUser() },
  );
}

async function waitForCursorReady(auth) {
  for (let attempt = 1; attempt <= 30; attempt += 1) {
    const listed = await api('GET', '/api/connectors', auth);
    const cursor = (listed.items ?? []).find((item) => item.id === 'cursor');
    if (cursor?.readiness === 'ready') return cursor;
    await sleep(1000);
  }
  fail('Cursor CLI did not become Ready');
}

async function seedDocsWorkspace(auth) {
  await resetMarketingWorkspace();
  await seedCheckout();
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
    connector: 'cursor',
    enabled: true,
  });
  await waitForCursorReady(auth);
  const models = await api(
    'GET',
    '/api/connectors/cursor/model-providers/cursor/models',
    auth,
  );
  const names = (models.items ?? []).map((model) => model.name);
  if (!names.includes('Composer 2.5') || !names.includes('Auto')) {
    fail(`Cursor model list was not the CLI list: ${names.join(', ')}`);
  }
  console.log(`screenshot: CLI models ${names.join(', ')}`);
  const repo = await api('POST', '/api/repos', auth, {
    name: 'Coppice',
    localPath: REPO_PATH,
    defaultBranch: 'main',
  });
  if (repo.verificationStatus !== 'ready') {
    fail(`repo is not ready: ${repo.verificationStatus} ${repo.verificationError ?? ''}`);
  }
  const board = await api('POST', '/api/boards', auth, { name: 'Coppice' });
  const ticket = await api('POST', `/api/boards/${board.id}/tickets`, auth, {
    title: 'Add a health check to the API',
    description: 'The load balancer needs a path it can call without signing in.',
    priority: 'medium',
  });
  await api('POST', `/api/tickets/${ticket.id}/assign`, auth, { agentId: agent.id });
  await api('PATCH', `/api/tickets/${ticket.id}`, auth, { repoId: repo.id });
  console.log(`screenshot: seeded docs board ${board.id}`);
  return { agentId: agent.id, boardId: board.id, ticketId: ticket.id };
}

async function launchBrowser() {
  try {
    return await chromium.launch({ headless: true });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.log(
      `screenshot: bundled Chromium failed to launch (${message.split('\n')[0]}). Trying Google Chrome.`,
    );
    return chromium.launch({ channel: 'chrome', headless: true });
  }
}

async function preparePage(page) {
  await page
    .waitForFunction(() => document.fonts.check('16px Roboto'), null, { timeout: 8_000 })
    .catch(() => {
      console.warn('screenshot: Roboto did not load; the PNG may use a fallback font');
    });
  await page.evaluate(() => document.fonts.ready);
  const bodyText = await page.locator('body').innerText();
  if (/sign out/i.test(bodyText) || bodyText.includes('admin@localhost')) {
    fail('account chrome is still visible on the page');
  }
  if (/\bmock\b|\bfake\b/i.test(bodyText)) {
    fail(`user-visible page text includes a forbidden word: ${JSON.stringify(bodyText.slice(0, 500))}`);
  }
  await page.addStyleTag({
    content:
      'html,body{overflow:hidden!important}*{scrollbar-width:none!important}*{caret-color:transparent!important}::-webkit-scrollbar{display:none!important}',
  });
}

async function shoot(page, name) {
  const scratch = await mkdtemp(path.join(tmpdir(), 'coppice-docs-shot-'));
  const png = path.join(scratch, `${name}.png`);
  await page.screenshot({ path: png, type: 'png', animations: 'disabled' });
  const webp = path.join(DOCS_ASSETS, `${name}.webp`);
  await mkdir(DOCS_ASSETS, { recursive: true });
  await runFfmpeg([
    '-y',
    '-i',
    png,
    '-c:v',
    'libwebp',
    '-quality',
    '82',
    '-compression_level',
    '6',
    webp,
  ]);
  const dims = await webpSize(webp);
  if (dims.width !== VIEWPORT.width || dims.height !== VIEWPORT.height) {
    fail(`${name} is ${dims.width}x${dims.height}, expected ${VIEWPORT.width}x${VIEWPORT.height}`);
  }
  await rm(scratch, { recursive: true, force: true });
  console.log(`screenshot: wrote ${webp}`);
}

function runFfmpeg(args) {
  return new Promise((resolve, reject) => {
    const child = spawn('ffmpeg', args, { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code !== 0) reject(new Error(`ffmpeg failed (${code}): ${stderr}`));
      else resolve();
    });
  });
}

async function webpSize(file) {
  const bytes = await readFile(file);
  if (bytes.length < 30 || bytes.toString('ascii', 0, 4) !== 'RIFF') {
    fail(`${file} is not a RIFF image`);
  }
  // VP8X extended, VP8 lossy, or VP8L lossless. ffmpeg libwebp writes VP8.
  const chunk = bytes.toString('ascii', 12, 16);
  if (chunk === 'VP8 ') {
    const width = bytes.readUInt16LE(26) & 0x3fff;
    const height = bytes.readUInt16LE(28) & 0x3fff;
    return { width, height };
  }
  if (chunk === 'VP8X') {
    const width = 1 + (bytes[24] | (bytes[25] << 8) | (bytes[26] << 16));
    const height = 1 + (bytes[27] | (bytes[28] << 8) | (bytes[29] << 16));
    return { width, height };
  }
  if (chunk === 'VP8L') {
    const b0 = bytes[21];
    const b1 = bytes[22];
    const b2 = bytes[23];
    const b3 = bytes[24];
    const width = 1 + (((b1 & 0x3f) << 8) | b0);
    const height = 1 + (((b3 & 0x0f) << 10) | (b2 << 2) | ((b1 & 0xc0) >> 6));
    return { width, height };
  }
  fail(`${file} has an unexpected WebP chunk ${chunk}`);
}

async function selectCombobox(page, id, optionName) {
  await page.locator(`#${id}`).click();
  await page.getByRole('option', { name: optionName, exact: true }).click();
  await page.getByRole('listbox').waitFor({ state: 'hidden' }).catch(() => {});
}

async function captureModelPicker(page) {
  await page.goto(`${WEB}/agents`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: 'Agents', level: 1 }).waitFor({ timeout: 30_000 });
  const row = page.getByRole('row', { name: 'Edit PM' });
  await row.waitFor();
  await row.getByRole('button', { name: 'Edit' }).click();
  await page.getByRole('heading', { name: 'Edit agent' }).waitFor();
  await page.locator('#agent-model').waitFor();
  await page.waitForFunction(() => {
    const trigger = document.getElementById('agent-model');
    return trigger && !trigger.disabled && !/Loading/.test(trigger.textContent ?? '');
  });
  // Keep the Model label and Refresh models on screen, with room below for the list.
  await page.locator('#agent-model').evaluate((node) => {
    const dialog = node.closest('[role="dialog"]');
    if (!dialog) return;
    const dialogRect = dialog.getBoundingClientRect();
    const nodeRect = node.getBoundingClientRect();
    dialog.scrollTop += nodeRect.top - dialogRect.top - 180;
  });
  // Confirm the control before the list opens. A modal popover marks the
  // rest of the dialog aria-hidden, so the button is not "visible" to
  // Playwright while the list is open, even though it is on screen.
  await page.getByRole('button', { name: 'Refresh models' }).waitFor();
  await page.locator('#agent-model').click();
  await page.getByRole('option', { name: 'Composer 2.5' }).waitFor();
  await preparePage(page);
  await shoot(page, 'model-picker');
}

async function captureNewAgent(page) {
  await page.goto(`${WEB}/agents`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'New agent' }).click();
  await page.getByRole('heading', { name: 'New agent' }).waitFor();
  await selectCombobox(page, 'agent-preset', 'pm — PM');
  await page.locator('#agent-name').fill('PM');
  await page.locator('#agent-role').waitFor();
  await page.waitForFunction(() => {
    const role = document.getElementById('agent-role');
    return role instanceof HTMLInputElement && role.value === 'PM';
  });
  await page.locator('#agent-connector').waitFor();
  await selectCombobox(page, 'agent-connector', 'Cursor — Ready');
  // The system prompt is 20 rows, so name, role, and connector cannot share a
  // 900px frame until the prompt field is shortened. The control is resizable.
  await page.locator('#agent-system-prompt').evaluate((node) => {
    node.style.height = '7.5rem';
  });
  await page.locator('#agent-name').evaluate((node) => {
    node.scrollIntoView({ block: 'start' });
  });
  const fitted = await page.evaluate(() => {
    const dialog = document.querySelector('[role="dialog"]');
    const name = document.getElementById('agent-name');
    const role = document.getElementById('agent-role');
    const connector = document.getElementById('agent-connector');
    if (!dialog || !name || !role || !connector) return false;
    const box = dialog.getBoundingClientRect();
    const boxes = [name, role, connector].map((node) => node.getBoundingClientRect());
    return boxes.every((rect) => rect.top >= box.top - 1 && rect.bottom <= box.bottom + 1);
  });
  if (!fitted) fail('new agent form does not show the name, role, and CLI together');
  await preparePage(page);
  await shoot(page, 'new-agent');
}

async function captureBacklogTicket(page, scene) {
  // The plan run control sits on the Backlog card, so the board, the new
  // ticket, and the button are all in the frame.
  await page.goto(`${WEB}/boards/${scene.boardId}`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: 'Board', level: 1 }).waitFor({ timeout: 30_000 });
  const backlog = page.getByRole('region', { name: 'Backlog' });
  await backlog.getByText('Add a health check to the API').waitFor();
  await backlog.getByRole('button', { name: 'Run agent', exact: true }).waitFor();
  await preparePage(page);
  await shoot(page, 'backlog-ticket');
}

async function capture(scene) {
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
      localStorage.setItem('coppice.plugins.guideOpen', '0');
    });
    const page = await context.newPage();
    await captureModelPicker(page);
    await captureNewAgent(page);
    await captureBacklogTicket(page, scene);
  } finally {
    await browser.close();
  }
}

async function main() {
  console.log(`screenshot: waiting for ${API}/health and ${WEB}`);
  await waitForOk(`${API}/health`, 'API');
  await waitForOk(WEB, 'web');
  await requireDesktopMode();
  await installCursorCli();
  await requireDesktopMode();
  const auth = await desktopSession();
  const scene = await seedDocsWorkspace(auth);
  await capture(scene);
  for (const shot of SHOTS) {
    console.log(`screenshot: ${shot.name} alt ${JSON.stringify(shot.alt)}`);
  }
}

main().catch((err) => {
  fail(err instanceof Error ? err.stack ?? err.message : String(err));
});
