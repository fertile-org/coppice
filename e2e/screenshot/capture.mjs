#!/usr/bin/env node
/**
 * Marketing board screenshot.
 *
 * Assumes the default Compose stack is already up with desktop mode forced
 * (`make screenshot` does that via deploy/docker-compose.screenshot.yml).
 * Seeds a board through the HTTP API — no agent runs — then opens the SPA
 * and writes static/screenshot.png. The same PNG is copied to the Astro
 * hero at website/public/assets/hero-screenshot.png.
 *
 * The shot must match the Electron app: login is bypassed, and the top bar
 * must not show the bootstrap admin email or Sign out.
 *
 * Env:
 *   COPPICE_API_URL         default http://localhost:5000
 *   COPPICE_WEB_URL         default http://localhost:5001
 *   COPPICE_SCREENSHOT_OUT  default <repo>/static/screenshot.png
 */

import { copyFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

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
  'Wait for Final Review',
  'Done',
  'Blocked',
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
  return res.json();
}

async function ensureAgents(auth) {
  const listed = await api('GET', '/api/agents', auth);
  const byName = new Map((listed.items ?? []).map((agent) => [agent.name, agent.id]));
  const ids = {};
  for (const agent of AGENTS) {
    const existing = byName.get(agent.name);
    if (existing) {
      ids[agent.name] = existing;
      continue;
    }
    const created = await api('POST', '/api/agents', auth, {
      name: agent.name,
      role: agent.role,
      systemPrompt: 'Marketing screenshot agent. Do not run.',
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
  for (const ticket of TICKETS) {
    const created = await api('POST', `/api/boards/${board.id}/tickets`, auth, {
      title: ticket.title,
      description: ticket.description,
      priority: ticket.priority,
    });
    if (ticket.status !== 'backlog') {
      await api('PATCH', `/api/tickets/${created.id}/status`, auth, {
        status: ticket.status,
      });
    }
    const agentId = agentIds[ticket.agent];
    if (!agentId) fail(`missing agent ${ticket.agent}`);
    await api('POST', `/api/tickets/${created.id}/assign`, auth, { agentId });
  }
  console.log(`screenshot: seeded ${TICKETS.length} tickets`);
  return board.id;
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

async function capture(boardId) {
  const browser = await launchBrowser();
  try {
    const context = await browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: 1,
      colorScheme: 'light',
      reducedMotion: 'reduce',
    });
    await context.addInitScript(() => {
      localStorage.setItem('coppice.theme', 'light');
      localStorage.setItem('coppice.sidebar.collapsed', '0');
    });
    const page = await context.newPage();
    await page.goto(`${WEB}/boards/${boardId}`, { waitUntil: 'domcontentloaded' });

    await page.getByRole('heading', { name: 'Board', level: 1 }).waitFor({
      timeout: 30_000,
    });
    for (const label of COLUMN_LABELS) {
      await page.getByRole('region', { name: label }).waitFor();
    }
    await page.getByText('Proxy plugin tools through the gateway').waitFor();

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

    const topbar = page.getByTestId('app-shell-topbar');
    const topbarText = await topbar.innerText();
    if (/sign out/i.test(topbarText) || topbarText.includes('@')) {
      fail(
        `account chrome is still visible in the top bar: ${JSON.stringify(topbarText)}`,
      );
    }
    const bodyText = await page.locator('body').innerText();
    if (/sign out/i.test(bodyText) || bodyText.includes('admin@localhost')) {
      fail('account chrome is still visible on the page');
    }
    if (await page.getByRole('button', { name: 'Sign in' }).count()) {
      fail('login screen is showing; desktop_mode did not bypass it');
    }

    await page.addStyleTag({
      content:
        'html,body{overflow:hidden!important}*{scrollbar-width:none!important}*{caret-color:transparent!important}',
    });

    await mkdir(path.dirname(OUT), { recursive: true });
    await page.screenshot({
      path: OUT,
      type: 'png',
      animations: 'disabled',
    });
    console.log(`screenshot: wrote ${OUT}`);
    if (path.resolve(OUT) !== path.resolve(HERO)) {
      await mkdir(path.dirname(HERO), { recursive: true });
      await copyFile(OUT, HERO);
      console.log(`screenshot: wrote ${HERO}`);
    }
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
  const agentIds = await ensureAgents(auth);
  const boardId = await seedBoard(auth, agentIds);
  await capture(boardId);
}

main().catch((err) => {
  fail(err instanceof Error ? err.stack ?? err.message : String(err));
});
