import { spawn } from 'node:child_process';
import { parseReadyLine } from './readyLine.mjs';
import { createRotatingLog } from './rotatingLog.mjs';

const RING_SIZE = 500;
const KILL_WAIT_MS = 2000;

function createLineSplitter(onLine) {
  let pending = '';
  return {
    push(text) {
      pending += text;
      let idx = pending.indexOf('\n');
      while (idx !== -1) {
        onLine(pending.slice(0, idx).replace(/\r$/, ''));
        pending = pending.slice(idx + 1);
        idx = pending.indexOf('\n');
      }
    },
    flush() {
      if (pending) onLine(pending.replace(/\r$/, ''));
      pending = '';
    },
  };
}

export function startServer({
  command,
  args = [],
  env = process.env,
  logFile,
  timeoutMs = 60000,
  graceMs = 15000,
  drainMs = 2000,
}) {
  const log = createRotatingLog(logFile);
  const ring = [];
  const child = spawn(command, args, {
    env,
    detached: true,
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  let settled = false;
  let resolveReady;
  let rejectReady;
  const ready = new Promise((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });
  // stop() rejects `ready` even when no caller is awaiting it.
  ready.catch(() => {});

  function settle(fn, value) {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    fn(value);
  }

  function onLine(line) {
    ring.push(line);
    if (ring.length > RING_SIZE) ring.shift();
    const url = parseReadyLine(line);
    if (url) settle(resolveReady, url);
  }

  const splitters = [child.stdout, child.stderr].map((stream) => {
    const splitter = createLineSplitter(onLine);
    stream.setEncoding('utf8');
    stream.on('data', (text) => {
      log.write(text);
      splitter.push(text);
    });
    return splitter;
  });

  child.stdin.on('error', () => {});

  const timer = setTimeout(() => {
    settle(rejectReady, new Error(`coppice-server timed out after ${timeoutMs} ms without a ready line`));
  }, timeoutMs);

  let childGone = false;
  let resolveChildExit;
  const childExit = new Promise((resolve) => {
    resolveChildExit = resolve;
  });

  // `exited` follows the child's 'exit', then waits up to `drainMs` for 'close' so trailing
  // output is still captured; a grandchild holding the pipes cannot delay it further.
  const exited = new Promise((resolve) => {
    let done = false;
    let drainTimer;
    function finish(code, signal) {
      if (done) return;
      done = true;
      clearTimeout(drainTimer);
      for (const splitter of splitters) splitter.flush();
      const how = code === null ? `signal ${signal}` : `code ${code}`;
      settle(rejectReady, new Error(`coppice-server exited with ${how} before it was ready`));
      log.close();
      resolve(code);
    }
    child.on('error', (err) => {
      onLine(`failed to start ${command}: ${err.message}`);
      settle(rejectReady, new Error(`failed to start coppice-server: ${err.message}`));
      childGone = true;
      resolveChildExit();
      finish(null, null);
    });
    child.on('exit', (code, signal) => {
      childGone = true;
      resolveChildExit();
      drainTimer = setTimeout(() => finish(code, signal), drainMs);
    });
    child.on('close', (code, signal) => finish(code, signal));
  });

  function within(promise, ms) {
    let timer;
    return Promise.race([
      promise.then(() => true),
      new Promise((resolve) => {
        timer = setTimeout(() => resolve(false), ms);
      }),
    ]).finally(() => clearTimeout(timer));
  }

  function killGroup() {
    if (child.pid === undefined) return;
    try {
      process.kill(-child.pid, 'SIGKILL');
    } catch (err) {
      if (err.code !== 'ESRCH') throw err;
    }
  }

  let stopping = null;

  async function doStop() {
    settle(rejectReady, new Error('coppice-server stopped before it was ready'));
    if (!childGone && child.pid !== undefined) {
      child.stdin.end();
      child.kill('SIGTERM');
      if (!(await within(childExit, graceMs))) {
        child.kill('SIGKILL');
        await within(childExit, KILL_WAIT_MS);
      }
    }
    killGroup();
    await within(exited, KILL_WAIT_MS);
  }

  return {
    pid: child.pid,
    ready,
    exited,
    lastLines(n = 50) {
      return ring.slice(-n);
    },
    stop() {
      stopping ??= doStop();
      return stopping;
    },
  };
}
