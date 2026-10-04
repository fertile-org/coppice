#!/usr/bin/env node
// Headless smoke against packaged resources: start `coppice-server desktop` on a
// temp data dir, check /health and the SPA, SIGTERM, expect exit 0 and Postgres
// stopped; then repeat on the same data dir to prove restart.
//
//   node scripts/headless-smoke.mjs --resources <dir>

import { access, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

import { startServer } from '../src/serverProcess.mjs';

const READY_TIMEOUT_MS = 120000;
const EXIT_TIMEOUT_MS = 20000;
const SPA_MARKER = '<div id="root">';

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

function withTimeout(promise, ms, what) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${what} timed out after ${ms} ms`)), ms);
    }),
  ]).finally(() => clearTimeout(timer));
}

async function runOnce(resources, dataDir, logFile, round) {
  const server = startServer({
    command: join(resources, 'bin', 'coppice-server'),
    args: ['desktop', '--data-dir', dataDir, '--resources', resources],
    logFile,
    timeoutMs: READY_TIMEOUT_MS,
  });
  try {
    const url = await server.ready;
    console.log(`round ${round}: ready at ${url}`);

    const health = await fetch(`${url}/health`);
    if (health.status !== 200) throw new Error(`GET /health returned ${health.status}`);

    const index = await fetch(`${url}/`);
    const body = await index.text();
    if (index.status !== 200 || !body.includes(SPA_MARKER)) {
      throw new Error(`GET / returned ${index.status} without ${SPA_MARKER}`);
    }

    process.kill(server.pid, 'SIGTERM');
    const code = await withTimeout(server.exited, EXIT_TIMEOUT_MS, 'shutdown after SIGTERM');
    if (code !== 0) throw new Error(`coppice-server exited with code ${code} after SIGTERM`);

    const pidFile = join(dataDir, 'pg', 'data', 'postmaster.pid');
    if (await exists(pidFile)) throw new Error(`${pidFile} still present after shutdown`);
    console.log(`round ${round}: clean shutdown`);
  } catch (err) {
    await server.stop();
    err.serverLines = server.lastLines(50);
    throw err;
  }
}

async function main() {
  const { values } = parseArgs({ options: { resources: { type: 'string' } } });
  if (!values.resources) throw new Error('usage: headless-smoke.mjs --resources <dir>');
  const resources = resolve(values.resources);

  const root = await mkdtemp(join(tmpdir(), 'coppice-smoke-'));
  const dataDir = join(root, 'data');
  try {
    for (const round of [1, 2]) {
      await runOnce(resources, dataDir, join(root, `server-${round}.log`), round);
    }
    console.log('headless smoke PASS');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((err) => {
    console.error(`headless smoke FAIL: ${err.message}`);
    if (err.serverLines?.length) {
      console.error('--- last coppice-server output ---');
      console.error(err.serverLines.join('\n'));
    }
    process.exit(1);
  });
}
