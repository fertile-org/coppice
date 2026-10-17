#!/usr/bin/env node
/**
 * Homepage and README hero.
 *
 * Same stack as `make screenshot`: Compose project coppice-screenshot,
 * desktop mode, 1440×900. Seeds the acme-web repo and a small board, then
 * records the core-flow storyboard. Card moves stay instant. Shots hold
 * still. A later column is a straight cut to a whole-column scroll, never a
 * pan or zoom. A crossfade joins only shots that share framing and scale,
 * and the edit pulses the card that moved. Any other join is a straight cut.
 * A caption is gone before the next shot, and the next caption starts there.
 * Writes:
 *
 *   website/public/assets/marketing.mp4   1440×900
 *   website/public/assets/marketing.webm  1440×900
 *   website/public/assets/marketing.gif   960px wide
 *   static/marketing.gif                  same GIF
 *   website/public/assets/hero-screenshot.png
 *   static/screenshots/01-… through 07-…
 *
 * Run after the screenshot stack is up:
 *   node e2e/screenshot/capture-hero.mjs
 */

import { copyFile, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
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
const ASSETS = path.join(REPO_ROOT, 'website', 'public', 'assets');
const STILLS_DIR = path.join(REPO_ROOT, 'static', 'screenshots');
const GIF_SITE = path.join(ASSETS, 'marketing.gif');
const GIF_README = path.join(REPO_ROOT, 'static', 'marketing.gif');
const MP4_OUT = path.join(ASSETS, 'marketing.mp4');
const WEBM_OUT = path.join(ASSETS, 'marketing.webm');
const POSTER = path.join(ASSETS, 'hero-screenshot.png');
const PROJECT = process.env.COPPICE_COMPOSE_PROJECT ?? 'coppice-screenshot';
const COMPOSE_FILES = [
  'deploy/docker-compose.yml',
  'deploy/docker-compose.screenshot.yml',
];

const VIEWPORT = { width: 1440, height: 900 };
const GIF_WIDTH = 960;
const REPO_NAME = 'acme-web';
const REPO_PATH = '/tmp/coppice-screenshot-repo';
const TICKET_TITLE = 'Add a dark mode toggle to Settings';
const LINE_COMMENT = 'Use the existing theme hook.';

const CAPTIONS = [
  'Write a ticket.',
  'Your PM agent drafts a plan.',
  'You approve it.',
  'Agents build, review, and test.',
  'Review the diff. Comment on any line.',
  'Nothing is done until you accept it.',
];

const PLAN_BODY = `## Plan

### Approach
Add a dark mode toggle on the Settings page. Keep the choice on this machine and apply it through the theme hook the rest of the app already uses.

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
  const args = ['docker', 'compose', '-p', PROJECT];
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
      await api('PATCH', `/api/tickets/${created.id}/status`, auth, {
        status: 'wait_for_final_review',
      });
      await api('POST', `/api/tickets/${created.id}/final-approve`, auth, {});
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
      gen_random_uuid(),
      ${sqlQuote(liveTicketId)}::uuid,
      ${sqlQuote(agentIds.Frontend)}::uuid,
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

async function attachPlan(ticketId, agentId) {
  await psql(`
    INSERT INTO ticket_comments (
      id, ticket_id, author_type, author_id, body, intent,
      mentions, attachment_ids, plan_content_version
    ) VALUES (
      gen_random_uuid(),
      ${sqlQuote(ticketId)}::uuid,
      'agent',
      ${sqlQuote(agentId)}::uuid,
      $plan$${PLAN_BODY}$plan$,
      'plan',
      '[]'::jsonb,
      '{}',
      (SELECT content_version FROM tickets WHERE id = ${sqlQuote(ticketId)}::uuid)
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
    fail(`${label}: internal note is visible: ${JSON.stringify(bodyText.slice(0, 700))}`);
  }
  if (/board settings|review cap|waiting for review/i.test(bodyText)) {
    fail(`${label}: review cap chrome is visible`);
  }
  const chip = page.locator('[data-testid="review-waiting-chip"]');
  if ((await chip.count()) > 0 && (await chip.first().isVisible())) {
    fail(`${label}: review waiting chip is visible`);
  }
}

async function setCaption(page, text) {
  if (text && !CAPTIONS.includes(text)) fail(`caption is not in the storyboard: ${text}`);
  await page.evaluate(
    ({ caption, styleText }) => {
      sessionStorage.setItem('coppice.hero.caption', caption ?? '');
      let style = document.getElementById('coppice-hero-caption-style');
      if (!style) {
        style = document.createElement('style');
        style.id = 'coppice-hero-caption-style';
        style.textContent = styleText;
        document.head.appendChild(style);
      }
      let bar = document.getElementById('coppice-hero-caption');
      if (!bar) {
        bar = document.createElement('div');
        bar.id = 'coppice-hero-caption';
        document.body.appendChild(bar);
      }
      bar.textContent = caption ?? '';
      bar.classList.toggle('is-on', Boolean(caption));
    },
    { caption: text ?? '', styleText: CAPTION_CSS },
  );
  if (text) {
    const shown = await page.locator('#coppice-hero-caption').innerText();
    if (shown !== text) fail(`caption on screen was ${JSON.stringify(shown)}`);
    const box = await page.locator('#coppice-hero-caption').boundingBox();
    if (!box || box.width < 80 || box.y < 700) {
      fail(`caption box is ${JSON.stringify(box)}`);
    }
  }
}

const CAPTION_STYLE = `
  #coppice-hero-caption {
    position: fixed;
    left: 50%;
    bottom: 28px;
    transform: translateX(-50%);
    z-index: 2147483646;
    max-width: calc(100% - 64px);
    margin: 0;
    padding: 12px 22px;
    border-radius: 999px;
    background: rgba(28, 25, 23, 0.94);
    color: #faf7f2;
    font: 600 28px/1.25 Roboto, ui-sans-serif, system-ui, sans-serif;
    letter-spacing: -0.015em;
    box-shadow: 0 10px 28px rgba(28, 25, 23, 0.28);
    pointer-events: none;
    white-space: nowrap;
    opacity: 0;
    transition: opacity 0.45s ease;
  }
  #coppice-hero-caption.is-on { opacity: 1; }
`;

const COLUMN_RULE =
  '#board{scroll-behavior:auto!important}#board > div > section{width:284px!important;min-width:284px!important;max-width:284px!important}';
const CAPTION_CSS = `${CAPTION_STYLE}\n${COLUMN_RULE}`;
const CROSSFADE_SECONDS = 0.5;
const CAPTION_MIN_MS = 2500;
// Marker edges are measured on discrete frames. Keep this much of the real
// shot on either side of a veil so a color frame never survives into the cut.
const MARKER_GUARD = 0.08;
const PLAN_SHOT_SECONDS = 1.55;
const PULSE_GAP = 4;
const PULSE_STROKE = 3;
const MARKERS = [
  { name: 'start', css: 'rgb(255, 0, 96)', rgb: [255, 0, 96] },
  { name: 'draft', css: 'rgb(120, 0, 255)', rgb: [120, 0, 255] },
  { name: 'plan', css: 'rgb(0, 255, 170)', rgb: [0, 255, 170] },
  { name: 'preapprove', css: 'rgb(0, 90, 255)', rgb: [0, 90, 255] },
  { name: 'ready', css: 'rgb(255, 128, 0)', rgb: [255, 128, 0] },
  { name: 'build', css: 'rgb(255, 220, 0)', rgb: [255, 220, 0] },
  { name: 'diff', css: 'rgb(80, 0, 255)', rgb: [80, 0, 255] },
  { name: 'accept', css: 'rgb(0, 180, 255)', rgb: [0, 180, 255] },
  { name: 'done', css: 'rgb(255, 80, 180)', rgb: [255, 80, 180] },
];
const SEGMENT_NAMES = [
  'write',
  'draft',
  'plan',
  'preapprove',
  'approve',
  'build',
  'diff',
  'accept',
  'done',
];
const SEGMENT_FRAMES = {
  write: 'board-left',
  draft: 'board-left',
  plan: 'plan',
  preapprove: 'board-left',
  approve: 'board-left',
  build: 'board-waiting',
  diff: 'diff',
  accept: 'dialog',
  done: 'board-done',
};

async function installCaption(page) {
  const current = await page.evaluate(
    () => sessionStorage.getItem('coppice.hero.caption') ?? '',
  );
  await setCaption(page, current || null);
}

async function shoot(page, name) {
  await mkdir(STILLS_DIR, { recursive: true });
  const file = path.join(STILLS_DIR, `${name}.png`);
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
  const geo = await boardGeometry(page);
  const scroll = snapScroll(geo, label);
  await pinBoardScroll(page, scroll);
  const after = await boardGeometry(page);
  const boardLeft = after.board.x;
  const boardRight = after.board.x + after.board.w;
  const visible = [];
  for (const col of after.cols) {
    const right = col.x + col.w;
    const overlaps = right > boardLeft + 1 && col.x < boardRight - 1;
    const fully = col.x >= boardLeft - 1 && right <= boardRight + 1;
    if (overlaps && !fully) fail(`column ${col.label} is cut in half`);
    if (fully) visible.push(col.label);
  }
  if (!visible.includes(label)) fail(`column ${label} is not fully in frame`);
  console.log(
    `hero: show ${label} scroll ${after.scrollLeft.toFixed(0)} columns ${visible.join(', ')}`,
  );
  return after.scrollLeft;
}

async function boardGeometry(page) {
  return page.locator('#board').evaluate((board) => {
    const rect = board.getBoundingClientRect();
    const style = getComputedStyle(board);
    const cols = [...board.querySelectorAll(':scope > div > section')].map((el) => {
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

async function measureCard(page, locator) {
  const read = () =>
    locator.evaluate((el) => {
      const card = el.parentElement;
      const rect = card.getBoundingClientRect();
      const radius = parseFloat(getComputedStyle(card).borderRadius) || 8;
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, radius };
    });
  let box = await read();
  await page.evaluate(
    () =>
      new Promise((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(resolve));
      }),
  );
  const again = await read();
  const drift =
    Math.abs(again.x - box.x) +
    Math.abs(again.y - box.y) +
    Math.abs(again.width - box.width) +
    Math.abs(again.height - box.height);
  if (drift > 1) {
    await sleep(250);
    box = await read();
  } else {
    box = again;
  }
  if (!box || box.width < 40 || box.height < 36) fail(`card box missing: ${JSON.stringify(box)}`);
  return box;
}

async function hold(page, ms) {
  const start = Date.now();
  let step = 0;
  while (Date.now() - start < ms) {
    await page.mouse.move(16 + (step % 4), 12).catch(() => {});
    step += 1;
    const left = ms - (Date.now() - start);
    if (left <= 0) break;
    await sleep(Math.min(350, left));
  }
}

function ffmpegBuffer(args) {
  return new Promise((resolve, reject) => {
    const child = spawn('ffmpeg', args, { stdio: ['ignore', 'pipe', 'pipe'] });
    const chunks = [];
    let stderr = '';
    child.stdout.on('data', (chunk) => chunks.push(chunk));
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code !== 0) reject(new Error(`ffmpeg failed (${code}): ${stderr}`));
      else resolve(Buffer.concat(chunks));
    });
  });
}

function nearColor(r, g, b, rgb, tol) {
  return Math.abs(r - rgb[0]) <= tol && Math.abs(g - rgb[1]) <= tol && Math.abs(b - rgb[2]) <= tol;
}

async function samplePixels(file, fps, crop) {
  const buffer = await ffmpegBuffer([
    '-v',
    'error',
    '-i',
    file,
    '-vf',
    `fps=${fps},crop=${crop},format=rgb24`,
    '-f',
    'rawvideo',
    'pipe:1',
  ]);
  const [w, h] = crop.split(':').slice(0, 2).map(Number);
  const frameBytes = w * h * 3;
  const frames = [];
  for (let offset = 0; offset + frameBytes <= buffer.length; offset += frameBytes) {
    frames.push(buffer.subarray(offset, offset + frameBytes));
  }
  return { frames, w, h, fps };
}

function markerRuns(sample) {
  const { frames, w, h, fps } = sample;
  const runs = [];
  let current = null;
  frames.forEach((frame, index) => {
    const mid = (Math.floor(h / 2) * w + Math.floor(w / 2)) * 3;
    const hit = MARKERS.find((marker) =>
      nearColor(frame[mid], frame[mid + 1], frame[mid + 2], marker.rgb, 36),
    );
    if (hit && current?.name === hit.name) {
      current.end = (index + 1) / fps;
      return;
    }
    if (current) runs.push(current);
    current = hit ? { name: hit.name, start: index / fps, end: (index + 1) / fps } : null;
  });
  if (current) runs.push(current);
  const merged = [];
  for (const run of runs) {
    const prev = merged.at(-1);
    if (prev && prev.name === run.name && run.start - prev.end < 0.5) {
      prev.end = Math.max(prev.end, run.end);
      continue;
    }
    merged.push({ ...run });
  }
  return merged;
}

function pipCues(sample) {
  const { frames, w, h, fps } = sample;
  const cues = [];
  let open = null;
  frames.forEach((frame, index) => {
    let magenta = 0;
    let green = 0;
    for (let i = 0; i < frame.length; i += 3) {
      const r = frame[i];
      const g = frame[i + 1];
      const b = frame[i + 2];
      if (r > 220 && b > 220 && g < 80) magenta += 1;
      if (g > 220 && r < 80 && b < 80) green += 1;
    }
    const need = w * h * 0.25;
    const kind = magenta > need ? 'pulse' : green > need ? 'still' : null;
    if (kind && kind !== open) cues.push({ kind, time: index / fps });
    open = kind;
  });
  return cues;
}

async function writePulse(file, width, height, radius) {
  const margin = PULSE_GAP + PULSE_STROKE;
  const w = Math.max(48, Math.round(width + margin * 2));
  const h = Math.max(36, Math.round(height + margin * 2));
  const rad = Math.max(4, Number(radius) + PULSE_GAP + PULSE_STROKE / 2);
  await new Promise((resolve, reject) => {
    const child = spawn(
      'python3',
      [
        '-c',
        `from PIL import Image, ImageDraw
import sys
w, h, radius, stroke, path = int(sys.argv[1]), int(sys.argv[2]), float(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
image = Image.new('RGBA', (w, h), (0, 0, 0, 0))
draw = ImageDraw.Draw(image)
inset = stroke / 2
draw.rounded_rectangle((inset, inset, w - 1 - inset, h - 1 - inset), radius=radius, outline=(231, 177, 90, 230), width=stroke)
image.save(path)
`,
        String(w),
        String(h),
        String(rad),
        String(PULSE_STROKE),
        file,
      ],
      { stdio: ['ignore', 'ignore', 'pipe'] },
    );
    let stderr = '';
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code !== 0) reject(new Error(`pulse image failed (${code}): ${stderr}`));
      else resolve();
    });
  });
  return { w, h, margin };
}

function ffmpeg(args) {
  return new Promise((resolve, reject) => {
    const child = spawn('ffmpeg', args, { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', (error) => {
      if (error.code === 'ENOENT') reject(new Error('ffmpeg is required'));
      else reject(error);
    });
    child.on('close', (code) => {
      if (code !== 0) reject(new Error(`ffmpeg failed (${code}): ${stderr}`));
      else resolve();
    });
  });
}

function ffprobe(file) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      'ffprobe',
      [
        '-v',
        'error',
        '-select_streams',
        'v:0',
        '-show_entries',
        'format=duration:stream=width,height,duration',
        '-of',
        'json',
        file,
      ],
      { stdio: ['ignore', 'pipe', 'pipe'] },
    );
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
      if (code !== 0) reject(new Error(`ffprobe failed (${code}): ${stderr}`));
      else {
        const parsed = JSON.parse(stdout);
        const stream = parsed.streams?.[0] ?? {};
        const duration = stream.duration ?? parsed.format?.duration;
        resolve({ ...stream, duration });
      }
    });
  });
}

function sameFraming(left, right) {
  return left.endFrame === right.startFrame;
}

function outputTime(segments, segmentIndex, at) {
  let start = 0;
  for (let index = 0; index < segmentIndex; index += 1) {
    start += segments[index].end - segments[index].start;
    if (sameFraming(segments[index], segments[index + 1])) start -= CROSSFADE_SECONDS;
  }
  return start + at;
}

async function renderSegment(clean, segment, dest) {
  const start = segment.start.toFixed(3);
  const end = segment.end.toFixed(3);
  const vf = `trim=start=${start}:end=${end},setpts=PTS-STARTPTS,fps=25,format=yuv420p`;
  await ffmpeg([
    '-y',
    '-i',
    clean,
    '-an',
    '-vf',
    vf,
    '-c:v',
    'libx264',
    '-pix_fmt',
    'yuv420p',
    '-crf',
    '16',
    '-preset',
    'veryfast',
    dest,
  ]);
}

async function overlayPulses(source, pulses, scratch, dest) {
  if (!pulses.length) {
    await copyFile(source, dest);
    return;
  }
  const args = ['-y', '-i', source];
  const filters = [];
  let label = '0:v';
  for (const [index, pulse] of pulses.entries()) {
    const png = path.join(scratch, `pulse-${index}-${pulse.at.toFixed(2)}.png`);
    const size = await writePulse(png, pulse.width, pulse.height, pulse.radius);
    pulse.margin = size.margin;
    args.push('-loop', '1', '-t', '0.75', '-i', png);
    const faded = `f${index}`;
    const next = `q${index}`;
    const x = Math.round(pulse.x - size.margin);
    const y = Math.round(pulse.y - size.margin);
    filters.push(
      `[${index + 1}:v]fps=25,format=rgba,fade=t=in:st=0:d=0.12:alpha=1,fade=t=out:st=0.4:d=0.22:alpha=1,setpts=PTS-STARTPTS+${pulse.at.toFixed(3)}/TB[${faded}]`,
    );
    filters.push(
      `[${label}][${faded}]overlay=${x}:${y}:eof_action=pass:format=auto[${next}]`,
    );
    label = next;
  }
  await ffmpeg([
    ...args,
    '-filter_complex',
    filters.join(';'),
    '-map',
    `[${label}]`,
    '-an',
    '-c:v',
    'libx264',
    '-pix_fmt',
    'yuv420p',
    '-crf',
    '16',
    '-preset',
    'veryfast',
    dest,
  ]);
}

async function assertPulseFrames(frames) {
  const script = `
import json, sys
from PIL import Image
frames = json.loads(sys.argv[1])
def gold(r, g, b):
    return r > 175 and 120 < g < 215 and b < 160 and r > g > b and (r - b) > 70
for item in frames:
    image = Image.open(item["file"]).convert("RGB")
    px = image.load()
    w, h = image.size
    pulse = item["pulse"]
    margin = pulse["margin"]
    left = pulse["x"] - margin
    top = pulse["y"] - margin
    right = pulse["x"] + pulse["width"] + margin
    bottom = pulse["y"] + pulse["height"] + margin
    xs, ys = [], []
    for y in range(max(0, int(top) - 6), min(h, int(bottom) + 8)):
        for x in range(max(0, int(left) - 6), min(w, int(right) + 8), 2):
            near = min(abs(x - left), abs(x - right), abs(y - top), abs(y - bottom)) <= 12
            if near and gold(*px[x, y]):
                xs.append(x); ys.append(y)
    if len(xs) < 80:
        raise SystemExit(f'{item["scene"]} pulse has too little highlight ({len(xs)})')
    box = (min(xs), min(ys), max(xs), max(ys))
    if abs(box[0] - left) > 10 or abs(box[1] - top) > 10 or abs(box[2] - right) > 10 or abs(box[3] - bottom) > 10:
        raise SystemExit(f'{item["scene"]} pulse box {box} != card outline {(round(left), round(top), round(right), round(bottom))}')
    title = 0
    x0, y0 = int(pulse["x"] + 18), int(pulse["y"] + 8)
    x1, y1 = int(pulse["x"] + pulse["width"] - 18), int(pulse["y"] + 32)
    for y in range(max(0, y0), min(h, y1)):
        for x in range(max(0, x0), min(w, x1), 2):
            if gold(*px[x, y]):
                title += 1
    if title > 8:
        raise SystemExit(f'{item["scene"]} pulse crosses the title ({title} pixels)')
    print(f'pulse {item["scene"]} {box} ok')
`;
  return run(
    [
      'python3',
      '-c',
      script,
      JSON.stringify(
        frames.map((item) => ({
          file: item.file,
          scene: item.scene,
          pulse: {
            x: item.pulse.x,
            y: item.pulse.y,
            width: item.pulse.width,
            height: item.pulse.height,
            margin: item.pulse.margin,
          },
        })),
      ),
    ],
    { cwd: REPO_ROOT },
  );
}

async function assertCaptionFrames(mp4, acceptTime) {
  const script = `
import subprocess, sys
from PIL import Image

mp4 = sys.argv[1]
accept_time = float(sys.argv[2])
width, height = 1440, 900
frame_bytes = width * height * 3

def pills(image):
    pixels = image.load()
    frame_w, frame_h = image.size
    cols = []
    for x in range(frame_w):
        ys = []
        for y in range(780, frame_h):
            red, green, blue = pixels[x, y]
            neutral = abs(red - green) < 18 and abs(green - blue) < 18 and abs(red - blue) < 18
            if red < 70 and green < 66 and blue < 62 and neutral:
                ys.append(y)
        if len(ys) >= 24:
            cols.append((x, min(ys), max(ys), len(ys)))
    runs = []
    for x, y0, y1, count in cols:
        if not runs or x - runs[-1]["x1"] > 20:
            runs.append({"x0": x, "x1": x, "y0": y0, "y1": y1, "n": count})
        else:
            run = runs[-1]
            run["x1"] = x
            run["y0"] = min(run["y0"], y0)
            run["y1"] = max(run["y1"], y1)
            run["n"] += count
    found = []
    for run in runs:
        box_w = run["x1"] - run["x0"] + 1
        box_h = run["y1"] - run["y0"] + 1
        # A fading pill can leave a narrow dark fragment. Real captions are wider.
        if box_w < 140 or box_h < 36 or box_h > 90:
            continue
        if run["n"] / (box_w * box_h) < 0.45:
            continue
        found.append((run["x0"], run["x1"]))
    return found

proc = subprocess.Popen(
    [
        "ffmpeg", "-v", "error", "-i", mp4,
        "-vf", "fps=5",
        "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
    ],
    stdout=subprocess.PIPE,
)
index = 0
previous = None
accept_times = []
try:
    while True:
        blob = proc.stdout.read(frame_bytes)
        if len(blob) < frame_bytes:
            break
        time = index / 5
        found = pills(Image.frombytes("RGB", (width, height), blob))
        if len(found) > 1:
            raise SystemExit(f"caption frame {time:.2f}s has {len(found)} pills {found}")
        if found:
            center = (found[0][0] + found[0][1]) / 2
            if abs(center - 720) > 100:
                raise SystemExit(f"caption frame {time:.2f}s is off center at {center:.0f} {found}")
            if previous is not None and abs(center - previous) > 50:
                raise SystemExit(
                    f"caption frame {time:.2f}s slid from {previous:.0f} to {center:.0f}"
                )
            previous = center
        else:
            previous = None
        if accept_time - 0.05 <= time <= accept_time + 2.7:
            if len(found) != 1:
                raise SystemExit(f"accept caption missing at {time:.2f}s")
            accept_times.append(time)
        index += 1
finally:
    proc.stdout.close()
    proc.wait()
if index == 0:
    raise SystemExit("caption check decoded no frames")
if not accept_times or accept_times[-1] - accept_times[0] < 2.5:
    span = 0 if not accept_times else accept_times[-1] - accept_times[0]
    raise SystemExit(f"accept caption lasted {span:.2f}s, need at least 2.5s")

for offset in (0, 0.8, 1.6, 2.5):
    shot = f"/tmp/hero-caption-check-{offset:.1f}.png"
    subprocess.check_call(
        ["ffmpeg", "-y", "-v", "error", "-ss", f"{accept_time + offset:.3f}", "-i", mp4, "-frames:v", "1", shot]
    )
    text = subprocess.check_output(
        ["tesseract", shot, "stdout", "--psm", "6"],
        text=True,
        stderr=subprocess.DEVNULL,
    ).lower()
    folded = " ".join(text.replace(",", " ").split())
    folded = folded.replace("accented", "accepted").replace("accepted", "accept")
    if "nothing is done" not in folded or "accept not merged" not in folded:
        raise SystemExit(f"accept +{offset:.1f}s missing caption or accepted state:\\n{text}")
    print(f"accept +{offset:.1f}s caption overlaps Accepted, not merged")
span = accept_times[-1] - accept_times[0]
print(f"captions stay centered ({index} frames, accept hold {span:.2f}s)")
`;
  return run(['python3', '-c', script, mp4, acceptTime.toFixed(3)], { cwd: REPO_ROOT });
}

async function assertHoldStill(mp4, start, end) {
  const script = `
import subprocess, sys
from PIL import Image

mp4, start, end = sys.argv[1], float(sys.argv[2]), float(sys.argv[3])
proc = subprocess.Popen(
    [
        "ffmpeg", "-v", "error", "-ss", f"{start:.3f}", "-to", f"{end:.3f}", "-i", mp4,
        "-vf", "fps=5", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
    ],
    stdout=subprocess.PIPE,
)
width, height = 1440, 900
frame_bytes = width * height * 3
frames = []
while True:
    blob = proc.stdout.read(frame_bytes)
    if len(blob) < frame_bytes:
        break
    frames.append(blob)
proc.wait()
if len(frames) < 8:
    raise SystemExit(f"static hold decoded {len(frames)} frames, need at least 3s")

def mad(a, b, dx):
    total = 0
    count = 0
    row = width * 3
    for y in range(0, height, 2):
        for x in range(40, width - 40, 4):
            src = y * row + x * 3
            dst_x = x + dx
            if dst_x < 0 or dst_x >= width:
                continue
            dst = y * row + dst_x * 3
            total += abs(a[src] - b[dst]) + abs(a[src + 1] - b[dst + 1]) + abs(a[src + 2] - b[dst + 2])
            count += 3
    return total / count

worst = 0
for index in range(1, len(frames)):
    scores = {dx: mad(frames[index - 1], frames[index], dx) for dx in range(-4, 5)}
    best = min(scores, key=scores.get)
    if best != 0:
        raise SystemExit(
            f"static hold frame {index} aligns at {best}px (mad {scores[best]:.2f}, center {scores[0]:.2f})"
        )
    if scores[0] > 3:
        raise SystemExit(f"static hold frame {index} changed by mad {scores[0]:.2f}")
    worst = max(worst, scores[0])
print(f"static hold {start:.2f}-{end:.2f}s still ({len(frames)} frames, mad {worst:.2f})")
`;
  return run(['python3', '-c', script, mp4, start.toFixed(3), end.toFixed(3)], { cwd: REPO_ROOT });
}

async function assertNoMarkerFlash(mp4) {
  const colors = MARKERS.map((marker) => `${marker.name}:${marker.rgb.join(',')}`).join(' ');
  const script = `
import subprocess, sys
mp4 = sys.argv[1]
markers = []
for item in sys.argv[2].split():
    name, rgb = item.split(":")
    markers.append((name, tuple(int(part) for part in rgb.split(","))))
proc = subprocess.Popen(
    ["ffmpeg", "-v", "error", "-i", mp4, "-vf", "fps=25,crop=8:8:716:446,format=rgb24", "-f", "rawvideo", "pipe:1"],
    stdout=subprocess.PIPE,
)
frame_bytes = 8 * 8 * 3
index = 0
hits = []
while True:
    blob = proc.stdout.read(frame_bytes)
    if len(blob) < frame_bytes:
        break
    mid = (4 * 8 + 4) * 3
    rgb = (blob[mid], blob[mid + 1], blob[mid + 2])
    for name, target in markers:
        if all(abs(channel - target[i]) <= 36 for i, channel in enumerate(rgb)):
            hits.append(f"{name}@{index / 25:.2f}")
            break
    index += 1
proc.wait()
if index == 0:
    raise SystemExit("marker scan decoded no frames")
if hits:
    raise SystemExit("marker color leaked into the cut: " + " ".join(hits[:12]))
print(f"no marker frames in {index} output frames")
`;
  return run(['python3', '-c', script, mp4, colors], { cwd: REPO_ROOT });
}

async function assemble(raw, pulses, stills) {
  const probed = await ffprobe(raw);
  const duration = Number(probed.duration);
  if (!Number.isFinite(duration)) fail('raw video has no duration');
  console.log(`hero: raw ${duration.toFixed(2)}s`);

  const center = await samplePixels(raw, 25, '8:8:716:446');
  const runs = markerRuns(center);
  console.log(
    `hero: markers ${runs.map((run) => `${run.name}@${run.start.toFixed(2)}-${run.end.toFixed(2)}`).join(' ')}`,
  );
  if (runs.length !== MARKERS.length || runs.some((run, index) => run.name !== MARKERS[index].name)) {
    fail(`marker sequence was ${runs.map((run) => run.name).join(',') || 'empty'}`);
  }

  const corner = await samplePixels(raw, 12, '22:22:0:0');
  const cues = pipCues(corner);
  const pulseCues = cues.filter((cue) => cue.kind === 'pulse');
  const stillCues = cues.filter((cue) => cue.kind === 'still');
  console.log(
    `hero: pulse cues ${pulseCues.map((cue) => cue.time.toFixed(2)).join(' ') || 'none'}`,
  );
  console.log(
    `hero: still cues ${stillCues.map((cue) => cue.time.toFixed(2)).join(' ') || 'none'}`,
  );
  if (pulseCues.length !== pulses.length) {
    fail(`found ${pulseCues.length} pulse cues, expected ${pulses.length}`);
  }
  if (stillCues.length !== stills.length) {
    fail(`found ${stillCues.length} still cues, expected ${stills.length}`);
  }

  const segments = SEGMENT_NAMES.map((name, index) => {
    const start = runs[index].end + MARKER_GUARD;
    const next =
      index === SEGMENT_NAMES.length - 1
        ? Math.max(start + 0.4, duration - 0.12)
        : runs[index + 1].start - MARKER_GUARD;
    const end = name === 'plan' ? Math.min(next, start + PLAN_SHOT_SECONDS) : next;
    if (!(end > start)) fail(`scene ${name} has no frames after the marker guard`);
    return {
      name,
      start,
      end,
      startFrame: SEGMENT_FRAMES[name],
      endFrame: SEGMENT_FRAMES[name],
      pulses: [],
    };
  });
  const placeCue = (time, label) => {
    const index = segments.findIndex((item) => time >= item.start - 0.05 && time <= item.end + 0.05);
    const segment = segments[index];
    if (!segment) fail(`${label} at ${time.toFixed(2)}s sits outside a scene`);
    return { segment, index, at: Math.max(0, time - segment.start) };
  };
  pulseCues.forEach((cue, index) => {
    const placed = placeCue(cue.time, 'pulse');
    placed.segment.pulses.push({ ...pulses[index], at: placed.at });
  });
  const stillFrames = stillCues.map((cue, index) => {
    const placed = placeCue(cue.time, 'still');
    return { name: stills[index], time: outputTime(segments, placed.index, placed.at) };
  });
  for (const segment of segments) {
    const length = segment.end - segment.start;
    console.log(
      `hero: scene ${segment.name} ${length.toFixed(2)}s pulses ${segment.pulses.length}`,
    );
    if (length < 1.2) fail(`scene ${segment.name} is only ${length.toFixed(2)}s`);
  }

  const scratch = await mkdtemp(path.join(tmpdir(), 'coppice-hero-edit-'));
  const clean = path.join(scratch, 'clean.mp4');
  await ffmpeg([
    '-y',
    '-i',
    raw,
    '-an',
    '-vf',
    'split[base][patch];[patch]crop=22:22:28:0[cover];[base][cover]overlay=0:0',
    '-c:v',
    'libx264',
    '-pix_fmt',
    'yuv420p',
    '-crf',
    '16',
    '-preset',
    'veryfast',
    clean,
  ]);

  for (const [index, segment] of segments.entries()) {
    const cut = path.join(scratch, `seg-${index}.mp4`);
    await renderSegment(clean, segment, cut);
    const pulsed = path.join(scratch, `pulsed-${index}.mp4`);
    await overlayPulses(cut, segment.pulses, scratch, pulsed);
    segment.file = pulsed;
  }

  const filters = [];
  let label = '0:v';
  let elapsed = segments[0].end - segments[0].start;
  segments.slice(1).forEach((segment, index) => {
    const prev = segments[index];
    const out = index === segments.length - 2 ? 'vout' : `j${index}`;
    const at = outputTime(segments, index, prev.end - prev.start);
    const left = `n${index}a`;
    const right = `n${index}b`;
    filters.push(`[${label}]settb=AVTB[${left}]`);
    filters.push(`[${index + 1}:v]settb=AVTB[${right}]`);
    if (sameFraming(prev, segment)) {
      const offset = elapsed - CROSSFADE_SECONDS;
      if (offset <= 0.05) fail(`crossfade does not fit after ${prev.name}`);
      filters.push(
        `[${left}][${right}]xfade=transition=fade:duration=${CROSSFADE_SECONDS}:offset=${offset.toFixed(3)}[${out}]`,
      );
      elapsed = elapsed + (segment.end - segment.start) - CROSSFADE_SECONDS;
      console.log(`hero: join ${prev.name}->${segment.name} crossfade at ${at.toFixed(2)}s`);
    } else {
      filters.push(`[${left}][${right}]concat=n=2:v=1:a=0[${out}]`);
      elapsed += segment.end - segment.start;
      console.log(`hero: join ${prev.name}->${segment.name} cut at ${at.toFixed(2)}s`);
    }
    label = out;
  });

  await mkdir(ASSETS, { recursive: true });
  await ffmpeg([
    '-y',
    ...segments.flatMap((segment) => ['-i', segment.file]),
    '-filter_complex',
    filters.join(';'),
    '-map',
    '[vout]',
    '-an',
    '-c:v',
    'libx264',
    '-pix_fmt',
    'yuv420p',
    '-crf',
    '20',
    '-preset',
    'medium',
    '-movflags',
    '+faststart',
    MP4_OUT,
  ]);
  await ffmpeg([
    '-y',
    '-i',
    MP4_OUT,
    '-an',
    '-c:v',
    'libvpx-vp9',
    '-crf',
    '34',
    '-b:v',
    '0',
    '-pix_fmt',
    'yuv420p',
    '-row-mt',
    '1',
    WEBM_OUT,
  ]);
  const palette = path.join(scratch, 'palette.png');
  const gifFilters = `fps=5,scale=${GIF_WIDTH}:-1:flags=lanczos`;
  await ffmpeg(['-y', '-i', MP4_OUT, '-vf', `${gifFilters},palettegen=max_colors=48:stats_mode=diff`, palette]);
  await ffmpeg([
    '-y',
    '-i',
    MP4_OUT,
    '-i',
    palette,
    '-lavfi',
    `${gifFilters}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5`,
    '-loop',
    '0',
    GIF_SITE,
  ]);
  await copyFile(GIF_SITE, GIF_README);

  const grab = async (time, dest) => {
    await ffmpeg([
      '-y',
      '-ss',
      time.toFixed(3),
      '-i',
      MP4_OUT,
      '-frames:v',
      '1',
      dest,
    ]);
  };
  const stillNames = {
    write: '01-write-ticket.png',
    plan: '02-plan.png',
    diff: '05-diff.png',
    accept: '06-accept.png',
  };
  await mkdir(STILLS_DIR, { recursive: true });
  for (const still of stillFrames) {
    const file = path.join(STILLS_DIR, stillNames[still.name] ?? `${still.name}.png`);
    await grab(still.time, file);
    console.log(`hero: still ${still.name} at ${still.time.toFixed(2)}s -> ${file}`);
  }
  const pulseFrames = [];
  for (const [index, segment] of segments.entries()) {
    for (const pulse of segment.pulses) {
      const time = outputTime(segments, index, pulse.at + 0.22);
      const file = path.join(scratch, `check-pulse-${index}-${pulse.at.toFixed(2)}.png`);
      await grab(time, file);
      pulseFrames.push({ file, pulse, time, scene: segment.name });
    }
  }
  const pulseStill = async (name, file) => {
    const index = segments.findIndex((segment) => segment.name === name);
    const pulse = segments[index]?.pulses[0];
    if (!pulse) fail(`missing pulse still for ${name}`);
    await grab(outputTime(segments, index, pulse.at + 0.22), path.join(STILLS_DIR, file));
  };
  await pulseStill('approve', '03-approve.png');
  await pulseStill('build', '04-build.png');
  const writeIndex = segments.findIndex((segment) => segment.name === 'write');
  const write = segments[writeIndex];
  // Caption has cleared, and the crossfade into the next shot has not started.
  const posterAt = Math.max(0.4, write.end - write.start - CROSSFADE_SECONDS - 0.35);
  const posterTime = outputTime(segments, writeIndex, posterAt);
  const posterFile = path.join(STILLS_DIR, '07-board.png');
  await grab(posterTime, posterFile);
  await copyFile(posterFile, POSTER);
  console.log((await assertPulseFrames(pulseFrames)).trim());
  const acceptStill = stillFrames.find((still) => still.name === 'accept');
  if (!acceptStill) fail('accept still is missing');
  console.log((await assertNoMarkerFlash(MP4_OUT)).trim());
  console.log((await assertCaptionFrames(MP4_OUT, acceptStill.time)).trim());
  const stillHoldStart = acceptStill.time + 1;
  const stillHoldEnd = acceptStill.time + 4;
  console.log((await assertHoldStill(MP4_OUT, stillHoldStart, stillHoldEnd)).trim());
  for (const [index, segment] of segments.entries()) {
    const start = outputTime(segments, index, 0);
    const end = outputTime(segments, index, segment.end - segment.start);
    console.log(`hero: beat ${segment.name} ${start.toFixed(2)}-${end.toFixed(2)} ${segment.startFrame}`);
  }
  await rm(scratch, { recursive: true, force: true });

  const mp4 = await ffprobe(MP4_OUT);
  const gif = await ffprobe(GIF_SITE);
  if (mp4.width !== 1440 || mp4.height !== 900) {
    fail(`mp4 is ${mp4.width}x${mp4.height}, expected 1440x900`);
  }
  if (gif.width !== GIF_WIDTH) fail(`gif width is ${gif.width}, expected ${GIF_WIDTH}`);
  const seconds = Number(mp4.duration);
  if (seconds < 54 || seconds > 70) fail(`hero duration ${seconds.toFixed(2)}s is outside 54–70s`);
  const gifBytes = (await stat(GIF_SITE)).size;
  if (gifBytes > 10 * 1024 * 1024) {
    fail(`gif is ${(gifBytes / (1024 * 1024)).toFixed(1)}MB, over 10MB`);
  }
  const webmBytes = (await stat(WEBM_OUT)).size;
  const mp4Bytes = (await stat(MP4_OUT)).size;
  console.log(`hero: mp4 ${mp4.width}x${mp4.height} ${seconds.toFixed(2)}s ${mp4Bytes} bytes`);
  console.log(`hero: webm ${webmBytes} bytes`);
  console.log(`hero: gif ${gif.width}x${gif.height} ${gifBytes} bytes`);
}

async function record(scene) {
  const browser = await launchBrowser();
  const videoDir = await mkdtemp(path.join(tmpdir(), 'coppice-hero-video-'));
  const pulses = [];
  const stills = [];
  let columnScroll = 0;
  let raw = null;
  try {
    const context = await browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: 1,
      colorScheme: 'light',
      locale: 'en-US',
      timezoneId: 'UTC',
      recordVideo: { dir: videoDir, size: VIEWPORT },
    });
    await context.addInitScript((styleText) => {
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
        const marker = sessionStorage.getItem('coppice.hero.marker') || '';
        let veil = document.getElementById('coppice-hero-marker');
        if (marker) {
          if (!veil && document.body) {
            veil = document.createElement('div');
            veil.id = 'coppice-hero-marker';
            veil.style.cssText = 'position:fixed;inset:0;z-index:2147483646;pointer-events:none;';
            document.body.appendChild(veil);
          }
          if (veil) {
            veil.style.background = marker;
            veil.style.display = 'block';
          }
        } else if (veil) {
          veil.style.display = 'none';
        }
      };
      const install = () => {
        const root = document.head || document.documentElement;
        if (!root || !document.body) return;
        if (!document.getElementById('coppice-hero-caption-style')) {
          const style = document.createElement('style');
          style.id = 'coppice-hero-caption-style';
          style.textContent = styleText;
          root.appendChild(style);
        }
        let bar = document.getElementById('coppice-hero-caption');
        if (!bar) {
          bar = document.createElement('div');
          bar.id = 'coppice-hero-caption';
          document.body.appendChild(bar);
        }
        const caption = sessionStorage.getItem('coppice.hero.caption') ?? '';
        bar.textContent = caption;
        const visible = sessionStorage.getItem('coppice.hero.caption.visible') === '1' && caption;
        bar.classList.toggle('is-on', visible);
        hideChrome();
      };
      const boot = () => {
        install();
        if (!document.body) return;
        new MutationObserver(() => {
          hideChrome();
          if (!document.getElementById('coppice-hero-caption')) install();
        }).observe(document.body, { childList: true, subtree: true });
      };
      if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
      else boot();
    }, CAPTION_CSS);

    const page = await context.newPage();
    page.on('pageerror', (error) => {
      console.log(`hero: pageerror ${error.message}`);
    });
    await page.route('**/api/tickets/*/comments', async (route) => {
      if (route.request().method() !== 'GET') {
        await route.continue();
        return;
      }
      const response = await route.fetch();
      const body = await response.json().catch(() => null);
      if (!Array.isArray(body)) {
        await route.fulfill({ response });
        return;
      }
      const filtered = body.filter((comment) => {
        const text = String(comment.body ?? '');
        if (comment.intent === 'system_event') return false;
        if (/Technical Lead|couldn.t start|No PM agent is available/.test(text)) return false;
        return true;
      });
      await route.fulfill({ status: response.status(), json: filtered });
    });

    let captionAt = 0;
    const storyCard = (column) => {
      const root = column ? page.getByRole('region', { name: column }) : page;
      return root.getByRole('button', { name: new RegExp(TICKET_TITLE) }).first();
    };
    const ensureCaption = async () => {
      if (!captionAt) return;
      const left = CAPTION_MIN_MS - (Date.now() - captionAt);
      if (left > 0) await hold(page, left);
    };
    const fadeCaption = async (text) => {
      await ensureCaption();
      await page.evaluate(() => {
        sessionStorage.setItem('coppice.hero.caption.visible', '0');
        document.getElementById('coppice-hero-caption')?.classList.remove('is-on');
      });
      await sleep(480);
      await setCaption(page, text);
      await page.evaluate((on) => {
        sessionStorage.setItem('coppice.hero.caption.visible', on ? '1' : '0');
      }, Boolean(text));
      captionAt = text ? Date.now() : 0;
      // The pill only transitions opacity, in place. Wait until that fade-in
      // finishes so the next cut cannot land while the caption is still appearing.
      if (text) await sleep(520);
    };
    const hideCaption = async () => {
      await page.evaluate(() => {
        sessionStorage.setItem('coppice.hero.caption.visible', '0');
        document.getElementById('coppice-hero-caption')?.classList.remove('is-on');
      });
      await sleep(500);
      captionAt = 0;
    };
    const cutScene = async (marker, settle) => {
      await page.evaluate((css) => {
        sessionStorage.setItem('coppice.hero.marker', css);
        let veil = document.getElementById('coppice-hero-marker');
        if (!veil) {
          veil = document.createElement('div');
          veil.id = 'coppice-hero-marker';
          veil.style.cssText = 'position:fixed;inset:0;z-index:2147483646;pointer-events:none;';
          document.body.appendChild(veil);
        }
        veil.style.zIndex = '2147483646';
        veil.style.background = css;
        veil.style.display = 'block';
      }, marker.css);
      await sleep(180);
      await settle();
      await sleep(220);
      await page.evaluate(() => {
        sessionStorage.removeItem('coppice.hero.marker');
        const veil = document.getElementById('coppice-hero-marker');
        if (veil) veil.style.display = 'none';
      });
      await sleep(120);
    };
    const flashPip = async (css) => {
      await page.evaluate((background) => {
        let pip = document.getElementById('coppice-hero-pip');
        if (!pip) {
          pip = document.createElement('div');
          pip.id = 'coppice-hero-pip';
          pip.style.cssText =
            'position:fixed;left:0;top:0;width:22px;height:22px;z-index:2147483647;pointer-events:none;';
          document.body.appendChild(pip);
        }
        document.body.appendChild(pip);
        pip.style.setProperty('background', background, 'important');
        pip.style.setProperty('display', 'block', 'important');
      }, css);
      await sleep(360);
      await page.evaluate(() => {
        const pip = document.getElementById('coppice-hero-pip');
        if (pip) pip.style.display = 'none';
      });
    };
    const markStill = async (name) => {
      stills.push(name);
      await flashPip('rgb(0, 255, 0)');
    };
    const pulseCue = async (locator) => {
      const box = await measureCard(page, locator);
      if (box.x < -1 || box.x + box.width > VIEWPORT.width + 1) {
        fail(`card is outside the frame: ${JSON.stringify(box)}`);
      }
      pulses.push({
        x: box.x,
        y: box.y,
        width: box.width,
        height: box.height,
        radius: box.radius,
      });
      await flashPip('#ff00ff');
    };

    await page.goto(`${WEB}/boards/${scene.boardId}`, { waitUntil: 'domcontentloaded' });
    await page.getByRole('heading', { name: 'Board', level: 1 }).waitFor({ timeout: 30_000 });
    await page.getByText('Remember the last open settings tab').waitFor();
    await page.getByText('Tighten spacing on the settings form').waitFor();
    await preparePage(page);
    await assertClean(page, 'board');

    await cutScene(MARKERS[0], async () => {
      await fadeCaption(CAPTIONS[0]);
      await sleep(250);
    });
    const input = page.locator('[data-add-ticket]');
    await input.click();
    await sleep(250);
    await input.pressSequentially(TICKET_TITLE, { delay: 62 });
    await sleep(350);
    await input.press('Enter');
    await storyCard('Backlog').waitFor({ timeout: 15_000 });
    await pinBoardLeft(page);
    await assertClean(page, 'scene 1');
    await hold(page, 1600);
    await markStill('write');
    await hold(page, 3800);

    const created = await api('GET', `/api/boards/${scene.boardId}/tickets`, scene.auth);
    const tickets = Array.isArray(created) ? created : (created.items ?? []);
    const ticket = tickets.find((item) => item.title === TICKET_TITLE);
    if (!ticket) fail('typed ticket was not saved');
    const worktreePromise = heroGit('worktree', ticket.id.split('-')[0]);
    // The crossfade overlaps the last half-second. That tail has no caption,
    // so two pills cannot blend.
    await fadeCaption(null);
    await hold(page, 800);

    await cutScene(MARKERS[1], async () => {
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
      const landed = storyCard('Plan Review');
      await landed.waitFor({ timeout: 15_000 });
      await pinBoardLeft(page);
      await preparePage(page);
    });
    await sleep(700);
    await pulseCue(storyCard('Plan Review'));
    await fadeCaption(CAPTIONS[1]);
    await hold(page, 3000);

    const planCard = storyCard('Plan Review');
    await cutScene(MARKERS[2], async () => {
      await planCard.click();
      await page.getByRole('button', { name: 'Approve plan' }).waitFor({ timeout: 15_000 });
      await page.getByText('acme-web', { exact: true }).first().waitFor();
      await page.getByRole('heading', { name: 'Plan', level: 2 }).scrollIntoViewIfNeeded();
      await page.getByText('Add the Settings toggle').waitFor();
      await preparePage(page);
    });
    await assertClean(page, 'scene plan');
    await hold(page, 200);
    await markStill('plan');
    await hold(page, 600);

    const detail = page.locator('[aria-label="Ticket detail"]');
    await cutScene(MARKERS[3], async () => {
      await hideCaption();
      await page.getByRole('button', { name: 'Close', exact: true }).click();
      await planCard.waitFor({ timeout: 15_000 });
      await pinBoardLeft(page);
      await preparePage(page);
    });
    await assertClean(page, 'scene preapprove');
    await hold(page, 1600);

    await cutScene(MARKERS[4], async () => {
      await planCard.click();
      await page.getByRole('button', { name: 'Approve plan' }).waitFor({ timeout: 15_000 });
      await page.getByRole('button', { name: 'Approve plan' }).click();
      await detail.locator('header').getByText('Ready', { exact: true }).waitFor({ timeout: 15_000 });
      await page.getByRole('button', { name: 'Close', exact: true }).click();
      await page.getByRole('region', { name: 'Ready' }).getByText(TICKET_TITLE).waitFor({ timeout: 15_000 });
      await pinBoardLeft(page);
      await preparePage(page);
    });
    await sleep(700);
    await pulseCue(storyCard('Ready'));
    await fadeCaption(CAPTIONS[2]);
    await assertClean(page, 'scene approve');
    await hold(page, 3800);
    await fadeCaption(null);
    await hold(page, 250);

    const short = ticket.id.split('-')[0];
    await cutScene(MARKERS[5], async () => {
      await api('PATCH', `/api/tickets/${ticket.id}/status`, scene.auth, {
        status: 'wait_for_final_review',
      });
      await psql(`
        UPDATE tickets
        SET branch_name = ${sqlQuote(`agent/TICKET-${short}`)}
        WHERE id = ${sqlQuote(ticket.id)}::uuid;
      `);
      const waiting = storyCard('Wait for Human Review');
      await waiting.waitFor({ timeout: 15_000 });
      columnScroll = await showColumn(page, 'Wait for Human Review');
      await preparePage(page);
    });
    await pinBoardScroll(page, columnScroll);
    await sleep(200);
    await fadeCaption(CAPTIONS[3]);
    await pulseCue(storyCard('Wait for Human Review'));
    await assertClean(page, 'scene build');
    await hold(page, 3800);
    await fadeCaption(null);
    await hold(page, 250);

    const worktree = await worktreePromise;
    if (!worktree?.startsWith('/data/worktrees/')) fail(`worktree path missing: ${worktree}`);
    await cutScene(MARKERS[6], async () => {
      const review = new URL('/code', WEB);
      review.searchParams.set('repoId', scene.repoId);
      review.searchParams.set('ticketId', ticket.id);
      review.searchParams.set('worktree', worktree);
      review.searchParams.set('baseBranch', 'main');
      await page.goto(review.href, { waitUntil: 'domcontentloaded' });
      await page.getByText('Code review', { exact: true }).waitFor({ timeout: 30_000 });
      await page.getByText('acme-web', { exact: true }).first().waitFor();
      await page.getByText('src/settings/ThemeToggle.tsx').waitFor();
      await page.getByRole('button', { name: 'Unified' }).click();
      await page.getByText("useState('light')").first().waitFor({ timeout: 20_000 });
      await preparePage(page);
    });
    await fadeCaption(CAPTIONS[4]);
    await assertClean(page, 'scene diff');
    await hold(page, 1800);
    const line = page.locator('tr').filter({ hasText: "useState('light')" }).first();
    await line.locator('td.diff-gutter').last().click();
    const commentBox = page.getByPlaceholder('Leave a comment…');
    await commentBox.waitFor({ timeout: 10_000 });
    await commentBox.pressSequentially(LINE_COMMENT, { delay: 38 });
    await page.getByRole('button', { name: 'Add comment', exact: true }).click();
    await page.getByText(LINE_COMMENT).waitFor();
    await assertClean(page, 'scene diff comment');
    await hold(page, 1400);
    await markStill('diff');
    await hold(page, 4400);
    await fadeCaption(null);
    await hold(page, 250);

    await cutScene(MARKERS[7], async () => {
      await page.goto(`${WEB}/boards/${scene.boardId}?ticket=${ticket.id}`, {
        waitUntil: 'domcontentloaded',
      });
      const ticketHeading = page.getByRole('heading', { name: TICKET_TITLE }).first();
      await ticketHeading.waitFor({ timeout: 30_000 });
      await page.getByRole('button', { name: 'Accept', exact: true }).waitFor();
      await preparePage(page);
      await ticketHeading.click();
      await page.keyboard.press('Control+Shift+Enter');
      await page.getByRole('dialog', { name: 'Accept this ticket?' }).waitFor({ timeout: 10_000 });
    });
    await fadeCaption(CAPTIONS[5]);
    await assertClean(page, 'scene accept');
    await hold(page, 2800);
    await fadeCaption(null);
    await hold(page, 250);
    const acceptDialog = page.getByRole('dialog', { name: 'Accept this ticket?' });
    await cutScene(MARKERS[8], async () => {
      await acceptDialog.getByRole('button', { name: 'Accept', exact: true }).click();
      await acceptDialog.waitFor({ state: 'hidden', timeout: 15_000 });
      await detail.locator('header').getByText('Done', { exact: true }).waitFor({ timeout: 15_000 });
      const drawerText = await detail.innerText();
      await page.getByRole('button', { name: 'Close', exact: true }).click();
      const doneCard = storyCard('Done');
      await doneCard.waitFor({ timeout: 15_000 });
      columnScroll = await showColumn(page, 'Done');
      await page
        .getByText('Accepted, not merged', { exact: true })
        .waitFor({ timeout: 6_000 })
        .catch(() => {});
      const cardText = await doneCard.evaluate((el) => el.parentElement.innerText);
      const acceptReport = `drawer:\n${drawerText}\n\ncard:\n${cardText}\n`;
      await writeFile('/tmp/hero-accept-state.txt', acceptReport);
      console.log(`hero: post-accept\n${acceptReport}`);
      await preparePage(page);
    });
    await pinBoardScroll(page, columnScroll);
    await sleep(300);
    await fadeCaption(CAPTIONS[5]);
    await pulseCue(storyCard('Done'));
    await markStill('accept');
    await hold(page, 4700);

    const video = page.video();
    await page.close();
    await context.close();
    raw = await video.path();
    console.log(`hero: raw video ${raw}`);
  } finally {
    await browser.close();
  }
  if (!raw) fail('recording did not finish');
  await assemble(raw, pulses, stills);
  await rm(videoDir, { recursive: true, force: true });
}

async function main() {
  console.log(`hero: waiting for ${API}/health and ${WEB}`);
  await waitForOk(`${API}/health`, 'API');
  await waitForOk(WEB, 'web');
  await requireDesktopMode();
  const auth = await desktopSession();
  const seeded = await seedWorkspace(auth);
  await record({ ...seeded, auth });
  for (const caption of CAPTIONS) console.log(`hero: caption ${JSON.stringify(caption)}`);
}

main().catch((err) => {
  fail(err instanceof Error ? err.stack ?? err.message : String(err));
});
