/**
 * Seed data for the marketing screenshot harness.
 *
 * HTTP for boards, tickets, repos, plugins, and chat sessions. SQL and files
 * inside the screenshot Compose project for the git diff, a finished agent
 * run's console events, and chat transcripts (posting a chat message would
 * start a run).
 *
 * Call `resetMarketingWorkspace` before creating the board. It deletes boards,
 * chats, repos, and plugin rows in the screenshot database only.
 */

import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(HERE, '../..');

const COMPOSE_FILES = [
  'deploy/docker-compose.yml',
  'deploy/docker-compose.screenshot.yml',
];
const PROJECT = process.env.COPPICE_COMPOSE_PROJECT ?? 'coppice-screenshot';

const REPO_NAME = 'Coppice Desktop';
const REPO_PATH = '/tmp/coppice-screenshot-repo';
const PLUGIN_DIR = '/home/coppice/plugins';

const FINAL_REVIEW_TITLE = 'Bundle Postgres with the desktop app';
const CONSOLE_TITLE = 'Show the live run in the console';

export function slugify(input) {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

export function ticketShort(id) {
  return id.split('-')[0];
}

function run(args, { input } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn('docker', args, {
      cwd: REPO_ROOT,
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
        reject(
          new Error(
            `docker ${args.join(' ')} failed (${code}): ${stderr || stdout}`,
          ),
        );
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
  for (const file of COMPOSE_FILES) {
    args.push('-f', file);
  }
  args.push(...extra);
  return args;
}

function serverUser() {
  const uid = process.env.COPPICE_UID ?? String(process.getuid?.() ?? 1000);
  const gid = process.env.COPPICE_GID ?? String(process.getgid?.() ?? 1000);
  return `${uid}:${gid}`;
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

async function serverScript(script, { user } = {}) {
  const args = ['exec', '-T'];
  if (user) args.push('-u', user);
  args.push('server', 'sh', '-s');
  return run(composeArgs(args), { input: script });
}

function sqlQuote(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

export async function resetMarketingWorkspace() {
  await psql(`
    DELETE FROM chat_sessions;
    DELETE FROM notifications;
    DELETE FROM boards;
    DELETE FROM repos;
    DELETE FROM agent_plugins;
    DELETE FROM plugin_settings;
    DELETE FROM plugin_installs;
    DELETE FROM plugins;
  `);
  await serverScript(
    `
    set -eu
    rm -rf ${REPO_PATH}
    rm -rf /data/worktrees/TICKET-*
    mkdir -p ${PLUGIN_DIR}
    find ${PLUGIN_DIR} -mindepth 1 -maxdepth 1 -exec rm -rf {} +
    `,
    { user: '0:0' },
  );
  console.log('screenshot: cleared previous marketing rows');
}

async function seedRepo(ticketId) {
  const short = ticketShort(ticketId);
  const slug = slugify(REPO_NAME);
  const script = await readFile(path.join(HERE, 'seed-repo.sh'), 'utf8');
  const stdout = await run(
    composeArgs([
      'exec',
      '-T',
      '-u',
      serverUser(),
      'server',
      'sh',
      '-s',
      '--',
      short,
      slug,
    ]),
    { input: script },
  );
  const worktree = stdout
    .trim()
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .at(-1);
  if (!worktree?.startsWith('/data/worktrees/')) {
    throw new Error(`seed-repo did not print a worktree path: ${stdout}`);
  }
  return {
    worktree,
    branch: `agent/TICKET-${short}`,
  };
}

async function copyPlugins() {
  const fixtures = path.join(HERE, 'fixtures', 'plugins');
  for (const name of ['desktop-packaging', 'review-checklist']) {
    await run(
      composeArgs([
        'cp',
        path.join(fixtures, name),
        `server:${PLUGIN_DIR}/${name}`,
      ]),
    );
  }
  await serverScript(`chown -R ${serverUser()} ${PLUGIN_DIR}`, { user: '0:0' });
}

const CONSOLE_EVENTS = [
  {
    type: 'claude-code.console.session',
    model: 'claude-sonnet-4-5',
  },
  {
    type: 'claude-code.console.thinking',
    text: 'The ticket wants the live run on the card. I will stream structured console events into the ticket drawer and keep the plain terminal log for connectors that only emit text.',
  },
  {
    type: 'claude-code.console.text',
    markdown:
      'I am wiring the Live Console tab to the run websocket. Tool calls stay readable while the ticket is In Progress.',
  },
  {
    type: 'claude-code.console.tool',
    id: 'tool-read-live',
    status: 'completed',
    variant: 'action',
    title: 'Read server/src/api/ws/live.rs',
    output:
      'pub async fn live_ws_handler(\n    replay console.events.jsonl when the process is gone\n)',
  },
  {
    type: 'claude-code.console.tool',
    id: 'tool-edit-console',
    status: 'completed',
    variant: 'shell',
    title: 'cargo test -p coppice-server live_console -- --test-threads 1',
    output:
      'running 4 tests\ntest structured_console_replays_events ... ok\ntest plain_log_falls_back ... ok\n\ntest result: ok. 4 passed; 0 failed',
  },
  {
    type: 'claude-code.console.result',
    contract: {
      status: 'done',
      summary:
        'The ticket drawer Live Console tab replays thinking, tool calls, and the result card for this run.',
      changedFiles: [
        'web/src/features/runs/ClaudeLiveConsole.tsx',
        'server/src/api/ws/live.rs',
      ],
      testsRun: ['cargo test -p coppice-server live_console'],
    },
  },
];

async function seedConsoleRun(ticketId, agentId) {
  const runId = randomUUID();
  const jsonl = `${CONSOLE_EVENTS.map((event) => JSON.stringify(event)).join('\n')}\n`;
  await serverScript(
    `
    set -eu
    dir="/data/artifacts/runs/${runId}"
    mkdir -p "$dir"
    cat > "$dir/console.events.jsonl" <<'END_JSONL'
${jsonl}END_JSONL
    `,
    { user: serverUser() },
  );
  await psql(`
    INSERT INTO agent_runs (
      id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
      context_profile, started_at, ended_at
    ) VALUES (
      ${sqlQuote(runId)}::uuid,
      ${sqlQuote(ticketId)}::uuid,
      ${sqlQuote(agentId)}::uuid,
      'work_on_ticket',
      'succeeded',
      'permissive-default',
      'full',
      now() - interval '6 minutes',
      now() - interval '2 minutes'
    );
  `);
  return runId;
}

const CHATS = [
  {
    agent: 'Backend Engineer',
    messages: [
      [
        'human',
        'Before I final-approve the desktop bundle, walk me through what happens on first launch.',
      ],
      [
        'agent',
        'First launch runs initdb into the app data directory, starts Postgres 16 on 127.0.0.1, and writes the connection string into the desktop config. Later launches reuse that cluster.',
      ],
      [
        'human',
        'What if the data directory was created by a newer major version?',
      ],
      [
        'agent',
        'The server checks the cluster major version and stops with a clear error instead of starting the wrong binaries. The ticket stays in Wait for Final Review until you accept that.',
      ],
    ],
  },
  {
    agent: 'PM',
    messages: [
      ['human', 'Split the billing migration before anyone starts coding.'],
      [
        'agent',
        'I left “Split the billing migration” in Ready: one ticket for the ledger tables and one for the invoice backfill. Nothing moves to In Progress until you approve the split.',
      ],
    ],
  },
];

async function seedChats(api, auth, agentIds, boardId) {
  let primaryId = null;
  for (const [index, chat] of CHATS.entries()) {
    const session = await api('POST', '/api/chat/sessions', auth, {
      agentId: agentIds[chat.agent],
      boardId,
    });
    if (index === 0) primaryId = session.id;
    const values = chat.messages
      .map(
        ([role, body], seq) =>
          `(${sqlQuote(randomUUID())}::uuid, ${sqlQuote(session.id)}::uuid, ${seq + 1}, ${sqlQuote(role)}, ${sqlQuote(body)}, '{}')`,
      )
      .join(',\n');
    const age = index === 0 ? '0 minutes' : '35 minutes';
    await psql(`
      INSERT INTO chat_messages (id, session_id, seq, role, body, attachment_ids)
      VALUES ${values};
      UPDATE chat_sessions
      SET updated_at = now() - interval '${age}'
      WHERE id = ${sqlQuote(session.id)}::uuid;
    `);
  }
  return primaryId;
}

/**
 * @param {Function} api
 * @param {{ cookie: string, csrfToken: string }} auth
 * @param {Record<string, string>} agentIds
 * @param {string} boardId
 * @param {Array<{ title: string, id: string }>} tickets
 */
export async function seedMarketingScenes(api, auth, agentIds, boardId, tickets) {
  const finalReview = tickets.find((ticket) => ticket.title === FINAL_REVIEW_TITLE);
  const consoleTicket = tickets.find((ticket) => ticket.title === CONSOLE_TITLE);
  if (!finalReview || !consoleTicket) {
    throw new Error('screenshot seed is missing the final-review or console ticket');
  }

  const git = await seedRepo(finalReview.id);
  const repo = await api('POST', '/api/repos', auth, {
    name: REPO_NAME,
    localPath: REPO_PATH,
    defaultBranch: 'main',
  });
  if (repo.verificationStatus !== 'ready') {
    throw new Error(
      `screenshot repo is not ready: ${repo.verificationStatus} ${repo.verificationError ?? ''}`,
    );
  }
  await api('PATCH', `/api/tickets/${finalReview.id}`, auth, {
    repoId: repo.id,
    branchName: git.branch,
    description: [
      'Ship Postgres 16 inside the desktop app so installers do not need Docker.',
      '',
      '- First launch runs `initdb` into the app data directory.',
      '- The server listens on `127.0.0.1` only.',
      '- A cluster from a different major version is refused.',
      '',
      'The diff is ready for final review.',
    ].join('\n'),
  });
  await api('POST', `/api/tickets/${finalReview.id}/comments`, auth, {
    body: 'Initdb stays on loopback and the cluster version is checked before start. Ready for final review.',
  });

  const runId = await seedConsoleRun(
    consoleTicket.id,
    agentIds['Backend Engineer'],
  );
  await copyPlugins();
  await api('POST', '/api/plugins/rescan', auth);
  const plugins = await api('GET', '/api/plugins', auth);
  if (!Array.isArray(plugins)) {
    throw new Error('plugin list was not an array');
  }
  const wanted = ['desktop-packaging', 'review-checklist'];
  for (const name of wanted) {
    const plugin = plugins.find((item) => item.name === name);
    if (!plugin) {
      throw new Error(
        `missing plugin ${name}; found ${plugins.map((item) => item.name).join(', ') || '(none)'}`,
      );
    }
    if (plugin.status !== 'ok') {
      throw new Error(
        `plugin ${plugin.name} status ${plugin.status}: ${plugin.error ?? ''}`,
      );
    }
    if (!plugin.enabled) {
      await api('PATCH', `/api/plugins/${plugin.id}`, auth, { enabled: true });
    }
  }

  const chatSessionId = await seedChats(api, auth, agentIds, boardId);

  console.log(`screenshot: seeded diff ${git.worktree}`);
  console.log(`screenshot: seeded console run ${runId}`);
  console.log(`screenshot: seeded chat ${chatSessionId}`);

  return {
    finalReviewTicketId: finalReview.id,
    consoleTicketId: consoleTicket.id,
    repoId: repo.id,
    worktree: git.worktree,
    chatSessionId,
  };
}
