import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const pointer = path.join(root, 'e2e/screenshot/.capture-dir');

describe('screenshot run failure', { concurrency: false }, () => {
  it('returns the Compose check status and does not start containers', () => {
    const bin = mkdtempSync(path.join(tmpdir(), 'coppice-screenshot-bin.'));
    const dockerLog = path.join(bin, 'docker.log');
    const nodeShim = path.join(bin, 'node');
    const composeShim = path.join(bin, 'docker-compose');
    writeFileSync(
      nodeShim,
      `#!/bin/sh
for arg in "$@"; do
  case "$arg" in
    *check-compose.mjs) exit 2 ;;
  esac
done
printf '%s\\n' "unexpected node $*" >> "${dockerLog}"
exit 99
`,
    );
    writeFileSync(
      composeShim,
      `#!/bin/sh
printf '%s\\n' "$*" >> "${dockerLog}"
exit 0
`,
    );
    chmodSync(nodeShim, 0o755);
    chmodSync(composeShim, 0o755);
    const env = {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      DOCKER_COMPOSE: composeShim,
    };
    delete env.COPPICE_SCREENSHOT_TRANSFER;
    const result = spawnSync('sh', ['e2e/screenshot/run.sh', 'up'], {
      cwd: root,
      encoding: 'utf8',
      env,
    });
    try {
      const dockerNotes = existsSync(dockerLog) ? readFileSync(dockerLog, 'utf8') : '';
      assert.equal(result.status, 2, `${result.stdout}\n${result.stderr}\n${dockerNotes}`);
      assert.equal(dockerNotes, '');
      assert.equal(existsSync(pointer), false);
    } finally {
      rmSync(bin, { recursive: true, force: true });
      if (existsSync(pointer)) {
        const recorded = readFileSync(pointer, 'utf8');
        const dir = recorded.match(/COPPICE_SCREENSHOT_DIR='([^']+)'/)?.[1];
        if (dir) rmSync(dir, { recursive: true, force: true });
        rmSync(pointer, { force: true });
      }
    }
  });

  it('copies the hardened config before starting a copy-mode server, and down keeps that mode', () => {
    const bin = mkdtempSync(path.join(tmpdir(), 'coppice-screenshot-bin.'));
    const dockerLog = path.join(bin, 'docker.log');
    const nodeShim = path.join(bin, 'node');
    const composeShim = path.join(bin, 'docker-compose');
    writeFileSync(
      nodeShim,
      `#!/bin/sh
for arg in "$@"; do
  case "$arg" in
    *check-compose.mjs) exit 0 ;;
  esac
done
printf '%s\\n' "unexpected node $*" >> "${dockerLog}"
exit 99
`,
    );
    writeFileSync(
      composeShim,
      `#!/bin/sh
printf '%s\\n' "$*" >> "${dockerLog}"
for arg in "$@"; do
  case "$arg" in
    */config.toml)
      if grep -q desktop_allowed_hosts "$arg"; then
        printf '%s\\n' "hardened-config $arg" >> "${dockerLog}"
      fi
      ;;
  esac
done
exit 0
`,
    );
    chmodSync(nodeShim, 0o755);
    chmodSync(composeShim, 0o755);
    const result = spawnSync('sh', ['e2e/screenshot/run.sh', 'stack'], {
      cwd: root,
      encoding: 'utf8',
      env: {
        ...process.env,
        PATH: `${bin}:${process.env.PATH}`,
        DOCKER_COMPOSE: composeShim,
        COPPICE_SCREENSHOT_TRANSFER: 'copy',
      },
    });
    try {
      const dockerNotes = existsSync(dockerLog) ? readFileSync(dockerLog, 'utf8') : '';
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${dockerNotes}`);
      const lines = dockerNotes.split('\n').filter(Boolean);
      const created = lines.findIndex((line) => line.includes(' up ') && line.includes('--no-start'));
      const copied = lines.findIndex((line) => line.startsWith('hardened-config '));
      const started = lines.findIndex(
        (line, index) => index > copied && line.includes(' up ') && !line.includes('--no-start'),
      );
      assert.ok(created >= 0, dockerNotes);
      assert.ok(copied > created, dockerNotes);
      assert.ok(started > copied, dockerNotes);
      assert.equal(
        lines.slice(0, copied).some((line) => line.includes(' up ') && !line.includes('--no-start')),
        false,
        dockerNotes,
      );
      const recorded = readFileSync(pointer, 'utf8');
      assert.match(recorded, /COPPICE_SCREENSHOT_TRANSFER='copy'/);
      const dir = recorded.match(/COPPICE_SCREENSHOT_DIR='([^']+)'/)?.[1];
      assert.ok(dir);
      const downEnv = {
        ...process.env,
        PATH: `${bin}:${process.env.PATH}`,
        DOCKER_COMPOSE: composeShim,
      };
      delete downEnv.COPPICE_SCREENSHOT_TRANSFER;
      const down = spawnSync('sh', ['e2e/screenshot/run.sh', 'down'], {
        cwd: root,
        encoding: 'utf8',
        env: downEnv,
      });
      const after = existsSync(dockerLog) ? readFileSync(dockerLog, 'utf8') : '';
      assert.equal(down.status, 0, `${down.stdout}\n${down.stderr}\n${after}`);
      const downLine = after.split('\n').filter((line) => line.includes(' down ')).at(-1) ?? '';
      assert.match(downLine, /docker-compose\.screenshot-copy\.yml/);
      assert.match(downLine, /down -v/);
      assert.equal(after.includes('prune'), false, after);
      assert.equal(existsSync(pointer), false);
      assert.equal(existsSync(dir), false);
    } finally {
      rmSync(bin, { recursive: true, force: true });
      if (existsSync(pointer)) {
        const recorded = readFileSync(pointer, 'utf8');
        const dir = recorded.match(/COPPICE_SCREENSHOT_DIR='([^']+)'/)?.[1];
        if (dir) rmSync(dir, { recursive: true, force: true });
        rmSync(pointer, { force: true });
      }
    }
  });

  it('leaves the server stopped and cleans the run when the hardened config copy fails', () => {
    const bin = mkdtempSync(path.join(tmpdir(), 'coppice-screenshot-bin.'));
    const dockerLog = path.join(bin, 'docker.log');
    const nodeShim = path.join(bin, 'node');
    const composeShim = path.join(bin, 'docker-compose');
    writeFileSync(
      nodeShim,
      `#!/bin/sh
for arg in "$@"; do
  case "$arg" in
    *check-compose.mjs) exit 0 ;;
  esac
done
printf '%s\\n' "unexpected node $*" >> "${dockerLog}"
exit 99
`,
    );
    writeFileSync(
      composeShim,
      `#!/bin/sh
printf '%s\\n' "$*" >> "${dockerLog}"
for arg in "$@"; do
  case "$arg" in
    */config.toml)
      if grep -q desktop_allowed_hosts "$arg"; then
        printf '%s\\n' "hardened-config $arg" >> "${dockerLog}"
      fi
      ;;
  esac
done
for arg in "$@"; do
  if [ "$arg" = cp ]; then
    exit 1
  fi
done
exit 0
`,
    );
    chmodSync(nodeShim, 0o755);
    chmodSync(composeShim, 0o755);
    const result = spawnSync('sh', ['e2e/screenshot/run.sh', 'stack'], {
      cwd: root,
      encoding: 'utf8',
      env: {
        ...process.env,
        PATH: `${bin}:${process.env.PATH}`,
        DOCKER_COMPOSE: composeShim,
        COPPICE_SCREENSHOT_TRANSFER: 'copy',
      },
    });
    try {
      const dockerNotes = existsSync(dockerLog) ? readFileSync(dockerLog, 'utf8') : '';
      assert.equal(result.status, 1, `${result.stdout}\n${result.stderr}\n${dockerNotes}`);
      const lines = dockerNotes.split('\n').filter(Boolean);
      const created = lines.findIndex((line) => line.includes(' up ') && line.includes('--no-start'));
      const copied = lines.findIndex((line) => line.startsWith('hardened-config '));
      assert.ok(created >= 0 && copied > created, dockerNotes);
      assert.equal(
        lines.some((line) => line.includes(' up ') && !line.includes('--no-start')),
        false,
        dockerNotes,
      );
      assert.equal(lines.some((line) => /(^| )start( |$)/.test(line)), false, dockerNotes);
      const cleanup = lines.filter((line) => line.includes(' down ')).at(-1) ?? '';
      assert.match(cleanup, /docker-compose\.screenshot-copy\.yml/);
      assert.match(cleanup, /down -v/);
      assert.equal(dockerNotes.includes('prune'), false);
      const config = lines.find((line) => line.startsWith('hardened-config '))?.split(' ').at(-1);
      assert.ok(config);
      assert.equal(existsSync(path.dirname(config)), false);
      assert.equal(existsSync(pointer), false);
    } finally {
      rmSync(bin, { recursive: true, force: true });
      if (existsSync(pointer)) {
        const recorded = readFileSync(pointer, 'utf8');
        const dir = recorded.match(/COPPICE_SCREENSHOT_DIR='([^']+)'/)?.[1];
        if (dir) rmSync(dir, { recursive: true, force: true });
        rmSync(pointer, { force: true });
      }
    }
  });
});
