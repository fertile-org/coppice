import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRotatingLog } from '../src/rotatingLog.mjs';

function tmpDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'coppice-log-test-'));
}

test('rotates once the file exceeds maxBytes', () => {
  const dir = tmpDir();
  const file = path.join(dir, 'logs', 'server.log');
  const log = createRotatingLog(file, { maxBytes: 10 });
  for (let i = 0; i < 5; i += 1) log.write('abcd\n');
  log.close();
  assert.ok(fs.existsSync(`${file}.1`));
  assert.ok(fs.existsSync(file));
});

test('never keeps more than `keep` rotated files', () => {
  const dir = tmpDir();
  const file = path.join(dir, 'server.log');
  const log = createRotatingLog(file, { maxBytes: 10, keep: 3 });
  for (let i = 0; i < 40; i += 1) log.write(`line${i}\n`);
  log.close();
  const names = fs.readdirSync(dir).sort();
  assert.deepEqual(names, ['server.log', 'server.log.1', 'server.log.2', 'server.log.3']);
});

test('appends to an existing log across instances', () => {
  const dir = tmpDir();
  const file = path.join(dir, 'server.log');
  const a = createRotatingLog(file);
  a.write('first\n');
  a.close();
  const b = createRotatingLog(file);
  b.write(Buffer.from('second\n'));
  b.close();
  assert.equal(fs.readFileSync(file, 'utf8'), 'first\nsecond\n');
});

function silenceConsoleError(t) {
  const calls = [];
  t.mock.method(console, 'error', (...args) => calls.push(args));
  return calls;
}

test('unwritable directory yields a no-op logger instead of throwing', (t) => {
  const calls = silenceConsoleError(t);
  const dir = tmpDir();
  const notADir = path.join(dir, 'file');
  fs.writeFileSync(notADir, '');
  const log = createRotatingLog(path.join(notADir, 'logs', 'server.log'));
  log.write('hello\n');
  log.close();
  assert.equal(calls.length, 1);
});

test('rotation failure disables file logging after one error', (t) => {
  const calls = silenceConsoleError(t);
  const dir = tmpDir();
  const file = path.join(dir, 'server.log');
  const log = createRotatingLog(file, { maxBytes: 5 });
  fs.rmSync(dir, { recursive: true, force: true });
  log.write('0123456789\n');
  log.write('more\n');
  log.write('and more\n');
  log.close();
  assert.equal(calls.length, 1);
});

test('write after close is ignored', () => {
  const dir = tmpDir();
  const file = path.join(dir, 'server.log');
  const log = createRotatingLog(file);
  log.close();
  log.write('late\n');
  assert.equal(fs.readFileSync(file, 'utf8'), '');
});
