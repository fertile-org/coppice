import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const pointer = path.join(root, 'e2e/screenshot/.capture-dir');

describe('screenshot run failure', () => {
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
    const result = spawnSync('sh', ['e2e/screenshot/run.sh', 'up'], {
      cwd: root,
      encoding: 'utf8',
      env: {
        ...process.env,
        PATH: `${bin}:${process.env.PATH}`,
        DOCKER_COMPOSE: composeShim,
      },
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
});
