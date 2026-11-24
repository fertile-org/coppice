#!/usr/bin/env node
/**
 * Marketing stills for docs and kit checks (not the looping hero video).
 *
 * Same stack as `make screenshot`: Compose project coppice-screenshot,
 * desktop mode, 1440×900. Seeds the acme-web repo and a small board, then
 * walks the core-flow storyboard and writes `static/screenshots/01-…` through
 * `08-…` with live `page.screenshot` at each beat.
 *
 * Hero mp4 / webm / gif / poster come from Remotion via `make hero-video`.
 * This script must not overwrite those files.
 *
 * Run after the screenshot stack is up (`make screenshot` prepares it).
 * `make screenshot` exports COPPICE_SCREENSHOT_CONFIG to a private file.
 *
 * The seed is fictional (`acme-web`, hero@coppice.local). It does not read
 * host SSH keys or a CLI login, and it does not show a second ticket
 * updating after the first one merges. See docs/development.md.
 */

import { mkdir, readFile, stat } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import {
  DESIGN_OPTIONS,
  DESIGN_SET_TITLE,
  RECOMMENDED_OPTION_ID,
} from './design-options.mjs';
import { composeInvocation } from './compose-cli.mjs';
import { resetMarketingWorkspace } from './seed.mjs';

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const WEB = process.env.COPPICE_WEB_URL ?? 'http://localhost:5001';
const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '../..');
const STILLS_DIR = path.join(REPO_ROOT, 'static', 'screenshots');
const PROJECT = process.env.COPPICE_COMPOSE_PROJECT ?? 'coppice-screenshot';
const COLUMN_RULE =
  '#board{scroll-behavior:auto!important}#board > div > section[aria-label]{width:284px!important;min-width:284px!important;max-width:284px!important}';

const VIEWPORT = { width: 1440, height: 900 };
const REPO_NAME = 'acme-web';
const REPO_PATH = '/tmp/coppice-screenshot-repo';
const TICKET_TITLE = 'Add a dark mode toggle to Settings';
const LINE_COMMENT = 'Use the existing theme hook.';

const STILL_FILES = {
  write: '01-write-ticket.png',
  plan: '02-plan.png',
  approve: '03-approve.png',
  build: '04-build.png',
  diff: '05-diff.png',
  accept: '06-accept.png',
  board: '07-board.png',
  design: '08-design-options.png',
};

const PLAN_BODY = `## Plan

### Approach
Build design option A: a switch in the Settings row. Keep the choice on this machine and apply it through the theme hook the rest of the app already uses.

### Steps
- [ ] Add the Settings toggle
- [ ] Use the existing theme hook
- [ ] Add a settings test

### Risks
None.

### Out of scope
A new color palette.`;

const SUPPORTING = [
  {
    title: 'Remember the last open settings tab',
    description: 'Reopen Settings on the tab the operator was using.',
    status: 'backlog',
    priority: 'low',
    agent: 'PM',
  },
  {
    title: 'Tighten spacing on the settings form',
    description: 'The fields sit too far apart on a laptop window.',
    status: 'in_progress',
    priority: 'high',
    agent: 'Frontend',
    live: true,
  },
  {
    title: 'Review the settings page at a narrow width',
    description: 'Check the layout when the window is about 800 pixels wide.',
    status: 'in_review',
    priority: 'medium',
    agent: 'QA',
  },
  {
    title: 'Cover theme switching with a smoke test',
    description: 'Open Settings, switch the theme, and reload.',
    status: 'in_qa',
    priority: 'medium',
    agent: 'QA',
  },
  {
    title: 'Publish the theme token notes',
    description: 'Write down the color tokens the theme hook reads.',
    status: 'done',
    priority: 'low',
    agent: 'Frontend',
  },
];

const FORBIDDEN =
  /\b(mock|fake|jira|linear|asana|trello|github|gitlab|cursor|claude|codex|opencode|kilo|copilot|devin|windsurf)\b/i;

const MAX_HEALTH_ATTEMPTS = 90;

function fail(message) {
  console.error(`hero: ${message}`);
  process.exit(1);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function run(args, { input, cwd = REPO_ROOT } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(args[0], args.slice(1), {
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
        reject(new Error(`${args.join(' ')} failed (${code}): ${stderr || stdout}`));
        return;
      }
      resolve(stdout);
    });
    if (input !== undefined) child.stdin.end(input);
    else child.stdin.end();
  });
}

function composeArgs(extra) {
  const { bin, args } = composeInvocation(extra, { project: PROJECT });
  return [bin, ...args];
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
    await sleep(1000);
  }
  fail(`${label} not ready at ${url}`);
}

async function requireDesktopMode() {
  const res = await fetch(`${API}/api/auth/capabilities`);
  if (!res.ok) fail(`capabilities failed: ${res.status} ${await res.text()}`);
  const body = await res.json();
  if (body.desktopMode !== true) {
    fail('server is not in desktop_mode. Run `make screenshot` first.');
  }
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

async function psql(sql) {
  await run(
    composeArgs([
      'exec',
      '-T',
      'postgres',
      'psql',
      '-U',
      'coppice',
      '-d',
      'coppice',
      '-v',
      'ON_ERROR_STOP=1',
      '-q',
    ]),
    { input: sql },
  );
}

function sqlQuote(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

async function heroGit(mode, short) {
  const script = await readFile(path.join(HERE, 'seed-hero-repo.sh'), 'utf8');
  const args = ['exec', '-T', '-u', serverUser(), 'server', 'sh', '-s', '--', mode];
  if (short) args.push(short);
  const stdout = await run(composeArgs(args), { input: script });
  return stdout
    .trim()
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .at(-1);
}

async function seedAgents(auth) {
  const existing = await api('GET', '/api/agents', auth);
  for (const agent of existing.items ?? []) {
    await api('DELETE', `/api/agents/${agent.id}`, auth);
  }
  const presets = await api('GET', '/api/agent-presets', auth);
  const byKey = new Map((presets.items ?? []).map((preset) => [preset.key, preset]));
  const wanted = [
    ['PM', 'pm'],
    ['Frontend', 'frontend_engineer'],
    ['QA', 'qc'],
  ];
  const ids = {};
  for (const [name, key] of wanted) {
    const preset = byKey.get(key);
    if (!preset) fail(`preset ${key} is missing`);
    const created = await api('POST', '/api/agents', auth, {
      name,
      presetId: preset.id,
      connector: 'claude-code',
      enabled: true,
    });
    ids[name] = created.id;
  }
  return ids;
}

async function seedSupporting(auth, agentIds, boardId) {
  let liveTicketId = null;
  for (const ticket of SUPPORTING) {
    const created = await api('POST', `/api/boards/${boardId}/tickets`, auth, {
      title: ticket.title,
      description: ticket.description,
      priority: ticket.priority,
    });
    if (ticket.status === 'in_progress') {
      await api('PATCH', `/api/tickets/${created.id}`, auth, { skipPlanning: true });
    }
    if (ticket.status === 'done') {
      // Accepting needs a real branch, and this card is only set dressing.
      await psql(`UPDATE tickets SET status = 'done' WHERE id = ${sqlQuote(created.id)};`);
    } else if (ticket.status !== 'backlog') {
      await api('PATCH', `/api/tickets/${created.id}/status`, auth, {
        status: ticket.status,
      });
    }
    await api('POST', `/api/tickets/${created.id}/assign`, auth, {
      agentId: agentIds[ticket.agent],
    });
    if (ticket.live) liveTicketId = created.id;
  }
  if (!liveTicketId) fail('missing the in-progress ticket');
  await psql(`
    INSERT INTO agent_runs (
      id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
      context_profile, started_at
    ) VALUES (
      gen_random_uuid()::text,
      ${sqlQuote(liveTicketId)},
      ${sqlQuote(agentIds.Frontend)},
      'work_on_ticket',
      'running',
      'permissive-default',
      'full',
      now() - interval '2 minutes'
    );
  `);
}

async function seedWorkspace(auth) {
  await resetMarketingWorkspace();
  const agentIds = await seedAgents(auth);
  await heroGit('init');
  const repo = await api('POST', '/api/repos', auth, {
    name: REPO_NAME,
    localPath: REPO_PATH,
    defaultBranch: 'main',
  });
  if (repo.verificationStatus !== 'ready') {
    fail(`repo is not ready: ${repo.verificationStatus} ${repo.verificationError ?? ''}`);
  }
  const board = await api('POST', '/api/boards', auth, { name: REPO_NAME });
  await seedSupporting(auth, agentIds, board.id);
  console.log(`hero: seeded board ${board.id} repo ${repo.id}`);
  return { agentIds, repoId: repo.id, boardId: board.id };
}

async function writeDesignFile(rel, html) {
  const dest = `/data/artifacts/${rel}`;
  await run(
    composeArgs([
      'exec',
      '-T',
      '-u',
      '0:0',
      'server',
      'sh',
      '-c',
      'mkdir -p "$(dirname "$1")" && cat > "$1"',
      'sh',
      dest,
    ]),
    { input: html },
  );
}

/** What a finished planning run leaves behind: its run, design options, and plan. */
async function attachPlan(ticketId, agentId) {
  const runId = randomUUID();
  const setId = randomUUID();
  const dir = `designs/${setId}`;
  for (const option of DESIGN_OPTIONS) {
    await writeDesignFile(`${dir}/${option.entry}`, option.html);
    for (const file of option.files ?? []) {
      await writeDesignFile(`${dir}/${file.path}`, file.content);
    }
  }
  const options = DESIGN_OPTIONS.map(({ id, title, entry }) => ({ id, title, entry }));
  await psql(`
    INSERT INTO agent_runs (
      id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
      context_profile, started_at, ended_at
    ) VALUES (
      ${sqlQuote(runId)},
      ${sqlQuote(ticketId)},
      ${sqlQuote(agentId)},
      'plan_ticket',
      'succeeded',
      'permissive-default',
      'full',
      now() - interval '2 minutes',
      now()
    );
    INSERT INTO design_sets (
      id, title, ticket_id, run_id, agent_id, dir, options,
      recommended_option_id, published_at
    ) VALUES (
      ${sqlQuote(setId)},
      ${sqlQuote(DESIGN_SET_TITLE)},
      ${sqlQuote(ticketId)},
      ${sqlQuote(runId)},
      ${sqlQuote(agentId)},
      ${sqlQuote(dir)},
      ${sqlQuote(JSON.stringify(options))}::jsonb,
      ${sqlQuote(RECOMMENDED_OPTION_ID)},
      now()
    );
    INSERT INTO ticket_plans (
      id, ticket_id, run_id, agent_id, revision, body, content_version,
      design_set_id, design_option_id
    ) VALUES (
      gen_random_uuid()::text,
      ${sqlQuote(ticketId)},
      ${sqlQuote(runId)},
      ${sqlQuote(agentId)},
      1,
      $plan$${PLAN_BODY}$plan$,
      (SELECT content_version FROM tickets WHERE id = ${sqlQuote(ticketId)}),
      ${sqlQuote(setId)},
      ${sqlQuote(RECOMMENDED_OPTION_ID)}
    );
  `);
}

async function launchBrowser() {
  try {
    return await chromium.launch({
      headless: true,
      args: ['--disable-dev-shm-usage'],
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.log(`hero: bundled Chromium failed (${message.split('\n')[0]}). Trying Google Chrome.`);
    return chromium.launch({
      channel: 'chrome',
      headless: true,
      args: ['--disable-dev-shm-usage'],
    });
  }
}

async function preparePage(page) {
  await page
    .waitForFunction(() => document.fonts.check('16px Roboto'), null, { timeout: 8_000 })
    .catch(() => {});
  await page.evaluate(() => document.fonts.ready);
  await page.addStyleTag({
    content: `html,body{overflow:hidden!important}*{scrollbar-width:none!important}*{caret-color:transparent!important}::-webkit-scrollbar{display:none!important}${COLUMN_RULE}`,
  });
  await page.evaluate(
    () =>
      new Promise((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(resolve));
      }),
  );
}

async function assertClean(page, label) {
  const bodyText = await page.locator('body').innerText();
  if (/sign out/i.test(bodyText) || bodyText.includes('admin@localhost')) {
    fail(`${label}: account chrome is visible`);
  }
  const hit = bodyText.match(FORBIDDEN);
  if (hit) {
    fail(`${label}: forbidden word ${hit[0]} in ${JSON.stringify(bodyText.slice(0, 700))}`);
  }
  if (bodyText.includes('/data/') || /Technical Lead|couldn't start/.test(bodyText)) {
    { const at = bodyText.search(/\/data\/|Technical Lead|couldn't start/); fail(`${label}: internal note is visible: ${JSON.stringify(bodyText.slice(Math.max(0, at - 200), at + 200))}`); }
  }
  if (/board settings|review cap|waiting for review/i.test(bodyText)) {
    fail(`${label}: review cap chrome is visible`);
  }
  const chip = page.locator('[data-testid="review-waiting-chip"]');
  if ((await chip.count()) > 0 && (await chip.first().isVisible())) {
    fail(`${label}: review waiting chip is visible`);
  }
}

async function shoot(page, key) {
  const filename = STILL_FILES[key];
  if (!filename) fail(`unknown still key: ${key}`);
  await mkdir(STILLS_DIR, { recursive: true });
  const file = path.join(STILLS_DIR, filename);
  await page.screenshot({ path: file, type: 'png', animations: 'disabled' });
  console.log(`hero: wrote ${file}`);
  return file;
}

async function pinBoardLeft(page) {
  await pinBoardScroll(page, 0);
}

async function pinBoardScroll(page, scrollLeft) {
  const board = page.locator('#board');
  if ((await board.count()) === 0) return;
  await board.evaluate((el, left) => {
    el.style.scrollBehavior = 'auto';
    el.scrollLeft = left;
  }, scrollLeft);
  await page.evaluate(
    () =>
      new Promise((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(resolve));
      }),
  );
}

async function showColumn(page, label) {
  await pinBoardLeft(page);
  const region = page.getByRole('region', { name: label });
  await region.waitFor({ timeout: 15_000 });
  // Prefer snapScroll when geometry is clean; otherwise scrollIntoView.
  try {
    const geo = await boardGeometry(page);
    if (geo.cols.some((col) => col.label === label)) {
      const scroll = snapScroll(geo, label);
      await pinBoardScroll(page, scroll);
      const after = await boardGeometry(page);
      console.log(`hero: show ${label} scroll ${after.scrollLeft.toFixed(0)}`);
      return after.scrollLeft;
    }
  } catch (error) {
    console.log(`hero: snapScroll fallback for ${label}: ${error instanceof Error ? error.message : error}`);
  }
  await region.evaluate((el) => {
    el.scrollIntoView({ inline: 'nearest', block: 'nearest' });
  });
  await pinBoardScroll(
    page,
    await page.locator('#board').evaluate((board) => board.scrollLeft),
  );
  const left = await page.locator('#board').evaluate((board) => board.scrollLeft);
  console.log(`hero: show ${label} scrollIntoView ${left.toFixed(0)}`);
  return left;
}

async function boardGeometry(page) {
  return page.locator('#board').evaluate((board) => {
    const rect = board.getBoundingClientRect();
    const style = getComputedStyle(board);
    // BoardPage: #board > div.grid > section[aria-label] (headers are siblings, not ancestors).
    const cols = [...board.querySelectorAll(':scope > div > section[aria-label]')].map((el) => {
      const box = el.getBoundingClientRect();
      return { label: el.getAttribute('aria-label'), x: box.x, w: box.width };
    });
    return {
      board: { x: rect.x, y: rect.y, w: rect.width, h: rect.height },
      padLeft: parseFloat(style.paddingLeft) || 0,
      scrollLeft: board.scrollLeft,
      cols,
    };
  });
}

function snapScroll(geo, label) {
  const boardW = geo.board.w;
  const cols = geo.cols.map((col) => {
    const left = col.x - geo.board.x + geo.scrollLeft;
    return { label: col.label, left, right: left + col.w };
  });
  const target = cols.find((col) => col.label === label);
  if (!target) fail(`column ${label} is missing`);
  const candidates = [0, ...cols.map((col) => col.right)];
  const fits = candidates.filter((scroll) => {
    const viewRight = scroll + boardW;
    if (target.left < scroll - 0.6 || target.right > viewRight + 0.6) return false;
    return cols.every((col) => {
      const fullyIn = col.left >= scroll - 0.6 && col.right <= viewRight + 0.6;
      return fullyIn || col.right <= scroll + 0.6 || col.left >= viewRight - 0.6;
    });
  });
  if (!fits.length) fail(`no whole-column view includes ${label}`);
  fits.sort((a, b) => a - b);
  return fits[0];
}


async function captureStills(scene) {
  const browser = await launchBrowser();
  let columnScroll = 0;
  try {
    const context = await browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: 1,
      colorScheme: 'light',
      locale: 'en-US',
      timezoneId: 'UTC',
    });
    await context.addInitScript(() => {
      if (window !== window.top) return;
      localStorage.setItem('coppice.theme', 'light');
      localStorage.setItem('coppice.sidebar.collapsed', '0');
      localStorage.setItem('coppice.plugins.guideOpen', '0');
      const hideChrome = () => {
        for (const el of document.querySelectorAll('dd')) {
          if ((el.textContent || '').includes('/data/')) {
            if (el.parentElement) el.parentElement.style.display = 'none';
          }
        }
        for (const el of document.querySelectorAll('button')) {
          if ((el.textContent || '').trim() === 'Board settings') el.style.display = 'none';
        }
        for (const el of document.querySelectorAll('[data-testid="review-waiting-chip"]')) {
          el.style.display = 'none';
        }
      };
      const boot = () => {
        if (!document.body) return;
        hideChrome();
        new MutationObserver(hideChrome).observe(document.body, { childList: true, subtree: true });
      };
      if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
      else boot();
    });

    const page = await context.newPage();
    page.on('pageerror', (error) => {
      console.log(`hero: pageerror ${error.message}`);
    });
    await page.route(/\/api\/tickets\/[^/]+\/comments(\?|$)/, async (route) => {
      if (route.request().method() !== 'GET') {
        await route.continue();
        return;
      }
      const response = await route.fetch();
      const body = await response.json().catch(() => null);
      const comments = Array.isArray(body) ? body : body?.items;
      if (!Array.isArray(comments)) {
        await route.fulfill({ response });
        return;
      }
      const filtered = comments.filter((comment) => {
        const text = String(comment.body ?? '');
        if (comment.intent === 'system_event') return false;
        if (/Technical Lead|couldn.t start|No PM agent is available/.test(text)) return false;
        return true;
      });
      await route.fulfill({
        status: response.status(),
        json: Array.isArray(body) ? filtered : { ...body, items: filtered },
      });
    });

    const storyCard = (column) => {
      const root = column ? page.getByRole('region', { name: column }) : page;
      return root.getByRole('button', { name: new RegExp(TICKET_TITLE) }).first();
    };
    const reloadBoard = async () => {
      await page.reload({ waitUntil: 'domcontentloaded' });
      await page.getByRole('heading', { name: REPO_NAME, level: 1 }).waitFor({ timeout: 30_000 });
    };

    await page.goto(`${WEB}/boards/${scene.boardId}`, { waitUntil: 'domcontentloaded' });
    await page.getByRole('heading', { name: REPO_NAME, level: 1 }).waitFor({ timeout: 30_000 });
    await page.getByText('Remember the last open settings tab').waitFor();
    await page.getByText('Tighten spacing on the settings form').waitFor();
    await preparePage(page);
    await assertClean(page, 'board');
    await shoot(page, 'board');

    const input = page.locator('[data-add-ticket]');
    await input.click();
    await input.pressSequentially(TICKET_TITLE, { delay: 40 });
    await input.press('Enter');
    await storyCard('Backlog').waitFor({ timeout: 15_000 });
    await pinBoardLeft(page);
    await preparePage(page);
    await assertClean(page, 'write');
    await shoot(page, 'write');

    const created = await api('GET', `/api/boards/${scene.boardId}/tickets`, scene.auth);
    const tickets = Array.isArray(created) ? created : (created.items ?? []);
    const ticket = tickets.find((item) => item.title === TICKET_TITLE);
    if (!ticket) fail('typed ticket was not saved');
    const worktreePromise = heroGit('worktree', ticket.id.split('-')[0]);

    await api('PATCH', `/api/tickets/${ticket.id}`, scene.auth, {
      description:
        'Settings should offer a dark mode toggle and follow the same theme preference as the rest of the app.',
      priority: 'high',
      repoId: scene.repoId,
    });
    await api('POST', `/api/tickets/${ticket.id}/assign`, scene.auth, {
      agentId: scene.agentIds.PM,
    });
    await api('PATCH', `/api/tickets/${ticket.id}/status`, scene.auth, { status: 'plan_review' });
    await attachPlan(ticket.id, scene.agentIds.PM);
    await reloadBoard();
    const planCard = storyCard('Plan Review');
    await planCard.waitFor({ timeout: 15_000 });
    await pinBoardLeft(page);
    await planCard.click();
    const approveBtn = page.getByRole('button', { name: /^Approve( plan| with )/ });
    await approveBtn.waitFor({ timeout: 15_000 });
    await page.getByRole('heading', { name: 'Plan', level: 2 }).scrollIntoViewIfNeeded();
    // Inline design picker (PlanBodyWithDesigns): Open opens the preview dialog.
    const openDesign = page.getByRole('button', { name: 'Open', exact: true }).first();
    await openDesign.waitFor({ timeout: 15_000 });
    await preparePage(page);
    await assertClean(page, 'plan');
    await shoot(page, 'plan');

    const designDialog = page.getByRole('dialog', { name: 'Design options' });
    await openDesign.click();
    await designDialog.waitFor({ timeout: 15_000 });
    const frame = page.frameLocator('iframe[title^="A: "]');
    await frame.getByText('Dark mode', { exact: true }).waitFor({ timeout: 15_000 });
    await preparePage(page);
    await assertClean(page, 'design');
    await shoot(page, 'design');
    await designDialog.getByRole('button', { name: 'Close', exact: true }).click();
    await designDialog.waitFor({ state: 'hidden', timeout: 15_000 });
    // Stay in the drawer; approve with recommended defaults if nothing was picked.
    await approveBtn.waitFor({ timeout: 15_000 });
    await approveBtn.click();
    const detail = page.locator('[aria-label="Ticket detail"]');
    await detail.locator('header').getByText('Ready', { exact: true }).waitFor({ timeout: 15_000 });
    await page.getByRole('button', { name: 'Close', exact: true }).click();
    await page.getByRole('region', { name: 'Ready' }).getByText(TICKET_TITLE).waitFor({ timeout: 15_000 });
    await pinBoardLeft(page);
    await preparePage(page);
    await assertClean(page, 'approve');
    await shoot(page, 'approve');

    const short = ticket.id.split('-')[0];
    await api('PATCH', `/api/tickets/${ticket.id}/status`, scene.auth, {
      status: 'wait_for_final_review',
    });
    await psql(`
      UPDATE tickets
      SET branch_name = ${sqlQuote(`agent/${short}-theme-toggle`)}
      WHERE id = ${sqlQuote(ticket.id)};
    `);
    await reloadBoard();
    await storyCard('Wait for Human Review').waitFor({ timeout: 15_000 });
    columnScroll = await showColumn(page, 'Wait for Human Review');
    await pinBoardScroll(page, columnScroll);
    await preparePage(page);
    await assertClean(page, 'build');
    await shoot(page, 'build');

    const worktree = await worktreePromise;
    if (!worktree?.startsWith('/data/worktrees/')) fail(`worktree path missing: ${worktree}`);
    const review = new URL('/code', WEB);
    review.searchParams.set('repoId', scene.repoId);
    review.searchParams.set('ticketId', ticket.id);
    review.searchParams.set('worktree', worktree);
    review.searchParams.set('baseBranch', 'main');
    await page.goto(review.href, { waitUntil: 'domcontentloaded' });
    await page.getByText('Code review', { exact: true }).waitFor({ timeout: 30_000 });
    await page.getByText('src/settings/ThemeToggle.tsx').waitFor();
    await page.getByRole('button', { name: 'Unified' }).click();
    await page.getByText("useState('light')").first().waitFor({ timeout: 20_000 });
    const line = page.locator('tr').filter({ hasText: "useState('light')" }).first();
    await line.locator('td.diff-gutter').last().click();
    const commentBox = page.getByPlaceholder('Leave a comment…');
    await commentBox.waitFor({ timeout: 10_000 });
    await commentBox.pressSequentially(LINE_COMMENT, { delay: 25 });
    await page.getByRole('button', { name: 'Add comment', exact: true }).click();
    await page.getByText(LINE_COMMENT).waitFor();
    await preparePage(page);
    await assertClean(page, 'diff');
    await shoot(page, 'diff');

    await page.goto(`${WEB}/boards/${scene.boardId}?ticket=${ticket.id}`, {
      waitUntil: 'domcontentloaded',
    });
    const ticketHeading = page.getByRole('heading', { name: TICKET_TITLE }).first();
    await ticketHeading.waitFor({ timeout: 30_000 });
    await page.getByRole('button', { name: 'Accept', exact: true }).waitFor();
    await ticketHeading.click();
    await page.keyboard.press('Control+Shift+Enter');
    const acceptDialog = page.getByRole('dialog', { name: 'Accept this ticket?' });
    await acceptDialog.waitFor({ timeout: 10_000 });
    await acceptDialog.getByRole('button', { name: 'Accept', exact: true }).click();
    await acceptDialog.waitFor({ state: 'hidden', timeout: 15_000 });
    await detail.locator('header').getByText('Done', { exact: true }).waitFor({ timeout: 15_000 });
    await page.getByRole('button', { name: 'Close', exact: true }).click();
    await storyCard('Done').waitFor({ timeout: 15_000 });
    columnScroll = await showColumn(page, 'Done');
    await pinBoardScroll(page, columnScroll);
    await preparePage(page);
    await assertClean(page, 'accept');
    await shoot(page, 'accept');

    await page.close();
    await context.close();
  } finally {
    await browser.close();
  }

  for (const key of Object.keys(STILL_FILES)) {
    const file = path.join(STILLS_DIR, STILL_FILES[key]);
    try {
      await stat(file);
    } catch {
      fail(`missing still ${file}`);
    }
  }
  console.log('hero: stills-only capture complete (video assets unchanged)');
}

async function main() {
  console.log(`hero: waiting for ${API}/health and ${WEB}`);
  await waitForOk(`${API}/health`, 'API');
  await waitForOk(WEB, 'web');
  await requireDesktopMode();
  const auth = await desktopSession();
  const seeded = await seedWorkspace(auth);
  await captureStills({ ...seeded, auth });
}

main().catch((err) => {
  fail(err instanceof Error ? err.stack ?? err.message : String(err));
});
