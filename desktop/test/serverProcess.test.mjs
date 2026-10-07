import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { SERVER_STOP_GRACE_MS, startServer } from '../src/serverProcess.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FAKE = path.join(__dirname, 'fixtures', 'fake-server.mjs');

function tmpLog() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'coppice-server-test-'));
  return path.join(dir, 'logs', 'server.log');
}

function start(mode, opts = {}) {
  return startServer({
    command: process.execPath,
    args: [FAKE, mode],
    env: process.env,
    logFile: tmpLog(),
    ...opts,
  });
}

function isAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    assert.equal(err.code, 'ESRCH');
    return false;
  }
}

test('stop grace leaves the server time to stop agent process groups', () => {
  assert.equal(SERVER_STOP_GRACE_MS, 15000);
});

test('ready resolves to the URL; stop exits 0 and logs are teed', async () => {
  const logFile = tmpLog();
  const server = start('ready', { logFile });
  assert.equal(await server.ready, 'http://127.0.0.1:5123');
  await server.stop();
  assert.equal(await server.exited, 0);
  const lines = server.lastLines();
  assert.ok(lines.includes('INFO starting fake server'));
  assert.ok(lines.includes('WARN fake stderr line'));
  assert.ok(lines.includes('COPPICE_READY url=http://127.0.0.1:5123'));
  assert.match(fs.readFileSync(logFile, 'utf8'), /INFO starting fake server/);
});

test('ready rejects with the exit code on early exit', async () => {
  const server = start('exit-early');
  await assert.rejects(server.ready, /3/);
  assert.equal(await server.exited, 3);
  assert.ok(server.lastLines().includes('Error: fake failure'));
});

test('ready rejects when no ready line arrives in time', async () => {
  const server = start('never-ready', { timeoutMs: 200 });
  await assert.rejects(server.ready, /timed out/);
  await server.stop();
});

test('stop before ready kills the child', async () => {
  const server = start('never-ready');
  server.ready.catch(() => {});
  await server.stop();
  assert.equal(isAlive(server.pid), false);
  await server.stop();
});

test('stop escalates to SIGKILL when SIGTERM is ignored', async () => {
  const server = start('ignore-term', { graceMs: 100 });
  await server.ready;
  await server.stop();
  assert.equal(isAlive(server.pid), false);
  assert.equal(await server.exited, null);
});

test('ready rejects when the command cannot be spawned', async () => {
  const server = startServer({
    command: path.join(__dirname, 'fixtures', 'does-not-exist'),
    logFile: tmpLog(),
  });
  await assert.rejects(server.ready, /failed to start/);
  assert.equal(await server.exited, null);
  await server.stop();
});

/** A SIGKILLed orphan stays visible to kill(pid, 0) until init reaps it. */
async function waitGone(pid, ms = 2000) {
  const deadline = Date.now() + ms;
  while (isAlive(pid) && Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 20));
  }
  return !isAlive(pid);
}

function grandchildPid(server) {
  const line = server.lastLines(500).find((l) => l.startsWith('GRANDCHILD pid='));
  assert.ok(line, 'fake server reported its grandchild');
  return Number(line.slice('GRANDCHILD pid='.length));
}

test('stop is prompt even when a grandchild holds the output pipes', async () => {
  const server = start('grandchild');
  await server.ready;
  const sleeper = grandchildPid(server);
  const started = Date.now();
  await server.stop();
  assert.ok(Date.now() - started < 1500, `stop took ${Date.now() - started} ms`);
  assert.equal(await server.exited, 0);
  assert.equal(await waitGone(sleeper), true);
});

test('exited resolves when the child exits but a grandchild keeps the pipes open', async () => {
  const server = start('grandchild-exit', { drainMs: 200 });
  const started = Date.now();
  await assert.rejects(server.ready, /5/);
  assert.equal(await server.exited, 5);
  assert.ok(Date.now() - started < 1500, `exit detection took ${Date.now() - started} ms`);
  const sleeper = grandchildPid(server);
  await server.stop();
  assert.equal(await waitGone(sleeper), true);
});

test('an unwritable log location does not break the server', async (t) => {
  t.mock.method(console, 'error', () => {});
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'coppice-server-test-'));
  fs.writeFileSync(path.join(dir, 'file'), '');
  const server = start('ready', { logFile: path.join(dir, 'file', 'logs', 'server.log') });
  assert.equal(await server.ready, 'http://127.0.0.1:5123');
  assert.ok(server.lastLines().includes('INFO starting fake server'));
  await server.stop();
});

test('lastLines returns the most recent n lines', async () => {
  const server = start('exit-early');
  await assert.rejects(server.ready);
  await server.exited;
  assert.deepEqual(server.lastLines(1), ['Error: fake failure']);
});
