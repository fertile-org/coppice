#!/usr/bin/env node
/**
 * Marketing screenshots and GIF.
 *
 * Assumes the screenshot Compose project is already up (`make screenshot`).
 * That project forces desktop mode and does not enable extra mock-provider
 * config. The script seeds a board, a final-review diff, a finished live
 * console, chat transcripts, and plugins, then captures five 1440×900 frames:
 *
 *   1. Human Review — code review diff for the Wait for Human Review ticket
 *   2. Board — same crop as the static hero
 *   3. Agent console — Live Console tab
 *   4. Chat
 *   5. Plugins
 *
 * The board frame is also written to static/screenshot.png and
 * website/public/assets/hero-screenshot.png. The GIF is
 * static/marketing.gif and website/public/assets/marketing.gif.
 *
 * Env:
 *   COPPICE_API_URL         default http://localhost:5000
 *   COPPICE_WEB_URL         default http://localhost:5001
 *   COPPICE_SCREENSHOT_OUT  default <repo>/static/screenshot.png
 *   COPPICE_GIF_DELAY_SEC   default 3
 */

import { copyFile, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { resetMarketingWorkspace, seedMarketingScenes } from './seed.mjs';

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const WEB = process.env.COPPICE_WEB_URL ?? 'http://localhost:5001';
const REPO_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..',
);
const OUT =
  process.env.COPPICE_SCREENSHOT_OUT ??
  path.join(REPO_ROOT, 'static', 'screenshot.png');
const HERO = path.join(
  REPO_ROOT,
  'website',
  'public',
  'assets',
  'hero-screenshot.png',
);
const STILLS_DIR = path.join(REPO_ROOT, 'static', 'screenshots');
const GIF_OUT = path.join(REPO_ROOT, 'static', 'marketing.gif');
const GIF_SITE = path.join(
  REPO_ROOT,
  'website',
  'public',
  'assets',
  'marketing.gif',
);
const FRAME_DELAY_SEC = Number(process.env.COPPICE_GIF_DELAY_SEC ?? '3');

const MAX_HEALTH_ATTEMPTS = 90;
const HEALTH_INTERVAL_MS = 1000;

// Normal laptop frame so the marketing PNG stays readable. Column widths
// stay as in the app (w-72); this is the window size, not a stretched board.
const VIEWPORT = { width: 1440, height: 900 };

const AGENTS = [
  { name: 'PM', role: 'pm' },
  { name: 'Backend Engineer', role: 'backend_engineer' },
  { name: 'QA', role: 'qc' },
  { name: 'DBA', role: 'dba' },
];

const TICKETS = [
  {
    title: 'Notice replica lag before it pages',
    description: 'DBA watches standby lag and raises a signal before users feel it.',
    status: 'backlog',
    priority: 'medium',
    agent: 'DBA',
  },
  {
    title: 'Write the repo checkout guide',
    description: 'How an operator registers a local git checkout for agents.',
    status: 'backlog',
    priority: 'low',
    agent: 'PM',
  },
  {
    title: 'Rotate the forge token',
    description: 'Replace the GitHub token stored for pull requests.',
    status: 'ready',
    priority: 'high',
    agent: 'Backend Engineer',
  },
  {
    title: 'Split the billing migration',
    description: 'PM proposes smaller tickets before work starts.',
    status: 'ready',
    priority: 'medium',
    agent: 'PM',
  },
  {
    title: 'Proxy plugin tools through the gateway',
    description: 'Plugin MCP tools show up as gateway tools on the run.',
    status: 'in_progress',
    priority: 'high',
    agent: 'Backend Engineer',
  },
  {
    title: 'Show the live run in the console',
    description: 'Stream the agent console while the ticket is in progress.',
    status: 'in_progress',
    priority: 'medium',
    agent: 'Backend Engineer',
  },
  {
    title: 'Review the desktop session bypass',
    description: 'Confirm the desktop app opens straight onto the board.',
    status: 'in_review',
    priority: 'high',
    agent: 'QA',
  },
  {
    title: 'Check cookie flags on loopback',
    description: 'Session cookie stays httpOnly on the desktop origin.',
    status: 'in_qa',
    priority: 'medium',
    agent: 'QA',
  },
  {
    title: 'Bundle Postgres with the desktop app',
    description: 'Final review of the packaged database bootstrap.',
    status: 'wait_for_final_review',
    priority: 'high',
    agent: 'PM',
  },
  {
    title: 'Drag a card to change its status',
    description: 'Moving a card between columns updates the ticket.',
    status: 'done',
    priority: 'low',
    agent: 'Backend Engineer',
  },
  {
    title: 'Verify the Claude Code connector',
    description: 'Live CLI check is still outstanding.',
    status: 'blocked',
    priority: 'critical',
    agent: 'PM',
  },
];

const COLUMN_LABELS = [
  'Backlog',
  'Ready',
  'In Progress',
  'In Review',
  'In QA',
  'Wait for Human Review',
  'Done',
  'Blocked',
];

const FRAME_ORDER = [
  '01-final-review',
  '02-board',
  '03-agent-console',
  '04-chat',
  '05-plugins',
];

function fail(message) {
  console.error(`screenshot: ${message}`);
  process.exit(1);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
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
  if (!res.ok) {
    fail(`capabilities failed: ${res.status} ${await res.text()}`);
  }
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
  if (!res.ok) {
    fail(`desktop-session failed: ${res.status} ${await res.text()}`);
  }
  const setCookie =
    typeof res.headers.getSetCookie === 'function'
      ? res.headers.getSetCookie()
      : [res.headers.get('set-cookie')].filter(Boolean);
  const sessionToken = parseSessionCookie(setCookie);
  if (!sessionToken) {
    fail('desktop-session did not return coppice_session');
  }
  const body = await res.json();
  if (!body.csrfToken) {
    fail('desktop-session did not return csrfToken');
  }
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

async function ensureAgents(auth) {
  const listed = await api('GET', '/api/agents', auth);
  const byName = new Map((listed.items ?? []).map((agent) => [agent.name, agent]));
  const ids = {};
  for (const agent of AGENTS) {
    const existing = byName.get(agent.name);
    if (existing) {
      if (existing.connector !== 'claude-code') {
        await api('PATCH', `/api/agents/${existing.id}`, auth, {
          connector: 'claude-code',
        });
      }
      ids[agent.name] = existing.id;
      continue;
    }
    const created = await api('POST', '/api/agents', auth, {
      name: agent.name,
      role: agent.role,
      systemPrompt: 'Marketing screenshot agent. Do not run.',
      connector: 'claude-code',
      enabled: true,
    });
    ids[agent.name] = created.id;
    console.log(`screenshot: created agent ${agent.name}`);
  }
  return ids;
}

async function seedBoard(auth, agentIds) {
  const board = await api('POST', '/api/boards', auth, { name: 'Coppice' });
  console.log(`screenshot: created board ${board.id}`);
  const tickets = [];
  for (const ticket of TICKETS) {
    const created = await api('POST', `/api/boards/${board.id}/tickets`, auth, {
      title: ticket.title,
      description: ticket.description,
      priority: ticket.priority,
    });
    if (ticket.status === 'in_progress') {
      const skipped = await api('PATCH', `/api/tickets/${created.id}`, auth, {
        skipPlanning: true,
      });
      if (skipped.skipPlanning !== true) {
        fail('skip planning was not enabled');
      }
    }
    if (ticket.status === 'done') {
      await api('PATCH', `/api/tickets/${created.id}/status`, auth, {
        status: 'wait_for_final_review',
      });
      await api('POST', `/api/tickets/${created.id}/final-approve`, auth, {});
    } else if (ticket.status !== 'backlog') {
      await api('PATCH', `/api/tickets/${created.id}/status`, auth, {
        status: ticket.status,
      });
    }
    const agentId = agentIds[ticket.agent];
    if (!agentId) fail(`missing agent ${ticket.agent}`);
    await api('POST', `/api/tickets/${created.id}/assign`, auth, { agentId });
    tickets.push({ id: created.id, title: ticket.title, status: ticket.status });
  }
  console.log(`screenshot: seeded ${TICKETS.length} tickets`);
  return { boardId: board.id, tickets };
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
    .waitForFunction(() => document.fonts.check('16px Roboto'), null, {
      timeout: 8_000,
    })
    .catch(() => {
      console.warn(
        'screenshot: Roboto did not load; the PNG may use a fallback font',
      );
    });
  await page.evaluate(() => document.fonts.ready);
  await assertDesktopChrome(page);
  await page.addStyleTag({
    content:
      'html,body{overflow:hidden!important}*{scrollbar-width:none!important}*{caret-color:transparent!important}::-webkit-scrollbar{display:none!important}',
  });
}

async function assertDesktopChrome(page) {
  const topbar = page.getByTestId('app-shell-topbar');
  if (await topbar.count()) {
    const topbarText = await topbar.innerText();
    if (/sign out/i.test(topbarText) || topbarText.includes('@')) {
      fail(
        `account chrome is still visible in the top bar: ${JSON.stringify(topbarText)}`,
      );
    }
  }
  const bodyText = await page.locator('body').innerText();
  if (/sign out/i.test(bodyText) || bodyText.includes('admin@localhost')) {
    fail('account chrome is still visible on the page');
  }
  if (/mock-provider|MockProvider/i.test(bodyText)) {
    fail('mock-provider chrome is visible on the page');
  }
  if (await page.getByRole('button', { name: 'Sign in' }).count()) {
    fail('login screen is showing; desktop_mode did not bypass it');
  }
}

async function shoot(page, name) {
  const file = path.join(STILLS_DIR, `${name}.png`);
  await page.screenshot({
    path: file,
    type: 'png',
    animations: 'disabled',
  });
  console.log(`screenshot: wrote ${file}`);
  return file;
}

async function captureFrames(page, scene) {
  const frames = {};

  const review = new URL('/code', WEB);
  review.searchParams.set('repoId', scene.repoId);
  review.searchParams.set('ticketId', scene.finalReviewTicketId);
  review.searchParams.set('worktree', scene.worktree);
  review.searchParams.set('baseBranch', 'main');
  await page.goto(review.href, { waitUntil: 'domcontentloaded' });
  await page.getByText('Code review', { exact: true }).waitFor({ timeout: 30_000 });
  await page
    .getByRole('link', { name: 'Ticket: Bundle Postgres with the desktop app' })
    .waitFor();
  await page.getByRole('heading', { name: 'Changed files' }).waitFor();
  await page.getByText('desktop/src/postgres.ts').waitFor();
  await page.getByText('initdb').first().waitFor();
  if (await page.getByText('Plan Review').count()) {
    fail('Plan Review is visible on the final-review frame');
  }
  await preparePage(page);
  frames['01-final-review'] = await shoot(page, '01-final-review');

  await page.goto(`${WEB}/boards/${scene.boardId}`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: 'Board', level: 1 }).waitFor({
    timeout: 30_000,
  });
  for (const label of COLUMN_LABELS) {
    await page.getByRole('region', { name: label }).waitFor();
  }
  await page.getByText('Proxy plugin tools through the gateway').waitFor();
  if (await page.getByRole('region', { name: 'Plan Review' }).count()) {
    fail('Plan Review column is on the board');
  }
  if (await page.getByText('Plan Review', { exact: true }).count()) {
    fail('Plan Review label is on the board');
  }
  await preparePage(page);
  frames['02-board'] = await shoot(page, '02-board');
  await mkdir(path.dirname(OUT), { recursive: true });
  await copyFile(frames['02-board'], OUT);
  console.log(`screenshot: wrote ${OUT}`);
  if (path.resolve(OUT) !== path.resolve(HERO)) {
    await mkdir(path.dirname(HERO), { recursive: true });
    await copyFile(OUT, HERO);
    console.log(`screenshot: wrote ${HERO}`);
  }

  await page.goto(
    `${WEB}/boards/${scene.boardId}?ticket=${scene.consoleTicketId}`,
    { waitUntil: 'domcontentloaded' },
  );
  await page.getByRole('tab', { name: 'Live Console' }).waitFor({ timeout: 30_000 });
  await page.getByRole('tab', { name: 'Live Console' }).click();
  await page.getByText('Claude Code session started').waitFor({ timeout: 20_000 });
  await page.getByText('Read server/src/api/ws/live.rs').waitFor();
  const consoleText = await page.locator('body').innerText();
  if (/interrupted/i.test(consoleText)) {
    fail(`live console was interrupted: ${consoleText.slice(0, 500)}`);
  }
  await preparePage(page);
  frames['03-agent-console'] = await shoot(page, '03-agent-console');

  await page.goto(`${WEB}/chat/${scene.chatSessionId}`, {
    waitUntil: 'domcontentloaded',
  });
  await page.getByTestId('chat-message-list').waitFor({ timeout: 30_000 });
  await page.getByText('Postgres 16 on 127.0.0.1').waitFor();
  await page.getByText('Split the billing migration').first().waitFor();
  await preparePage(page);
  frames['04-chat'] = await shoot(page, '04-chat');

  await page.goto(`${WEB}/settings/plugins`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: 'Plugins', level: 1 }).waitFor({
    timeout: 30_000,
  });
  await page.getByRole('heading', { name: 'desktop-packaging' }).waitFor();
  await page.getByRole('heading', { name: 'review-checklist' }).waitFor();
  await page.getByText('stdio').waitFor();
  await preparePage(page);
  frames['05-plugins'] = await shoot(page, '05-plugins');

  return FRAME_ORDER.map((name) => frames[name]);
}

function ffmpeg(args) {
  return new Promise((resolve, reject) => {
    const child = spawn('ffmpeg', args, { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', (error) => {
      if (error.code === 'ENOENT') {
        reject(new Error('ffmpeg is required to assemble the marketing GIF'));
        return;
      }
      reject(error);
    });
    child.on('close', (code) => {
      if (code !== 0) reject(new Error(`ffmpeg failed (${code}): ${stderr}`));
      else resolve();
    });
  });
}

async function writeGif(framePaths) {
  if (!Number.isFinite(FRAME_DELAY_SEC) || FRAME_DELAY_SEC <= 0) {
    fail(`invalid COPPICE_GIF_DELAY_SEC: ${process.env.COPPICE_GIF_DELAY_SEC}`);
  }
  const scratch = await mkdtemp(path.join(tmpdir(), 'coppice-gif-'));
  const listPath = path.join(scratch, 'frames.txt');
  const lines = [];
  for (const frame of framePaths) {
    lines.push(`file '${frame}'`);
    lines.push(`duration ${FRAME_DELAY_SEC}`);
  }
  // concat demuxer holds each duration until the next file, so repeat the last.
  lines.push(`file '${framePaths[framePaths.length - 1]}'`);
  await writeFile(listPath, `${lines.join('\n')}\n`);

  const palette = path.join(scratch, 'palette.png');
  await ffmpeg([
    '-y',
    '-f',
    'concat',
    '-safe',
    '0',
    '-i',
    listPath,
    '-vf',
    'fps=10,scale=1440:900:flags=lanczos,palettegen=max_colors=192',
    palette,
  ]);
  await mkdir(path.dirname(GIF_OUT), { recursive: true });
  await ffmpeg([
    '-y',
    '-f',
    'concat',
    '-safe',
    '0',
    '-i',
    listPath,
    '-i',
    palette,
    '-lavfi',
    'fps=10,scale=1440:900:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3',
    '-loop',
    '0',
    GIF_OUT,
  ]);
  await mkdir(path.dirname(GIF_SITE), { recursive: true });
  await copyFile(GIF_OUT, GIF_SITE);
  await rm(scratch, { recursive: true, force: true });
  console.log(`screenshot: wrote ${GIF_OUT}`);
  console.log(`screenshot: wrote ${GIF_SITE}`);
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
    await mkdir(STILLS_DIR, { recursive: true });
    const frames = await captureFrames(page, scene);
    await writeGif(frames);
  } finally {
    await browser.close();
  }
}

async function main() {
  console.log(`screenshot: waiting for ${API}/health and ${WEB}`);
  await waitForOk(`${API}/health`, 'API');
  await waitForOk(WEB, 'web');
  await requireDesktopMode();
  const auth = await desktopSession();
  await resetMarketingWorkspace();
  const agentIds = await ensureAgents(auth);
  const { boardId, tickets } = await seedBoard(auth, agentIds);
  const scene = await seedMarketingScenes(api, auth, agentIds, boardId, tickets);
  await capture({ ...scene, boardId });
}

main().catch((err) => {
  fail(err instanceof Error ? err.stack ?? err.message : String(err));
});
