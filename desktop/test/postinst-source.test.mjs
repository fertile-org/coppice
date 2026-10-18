import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const script = readFileSync(new URL('../build/linux/after-install.sh', import.meta.url), 'utf8');
const block = script.slice(script.indexOf('# BEGIN apt source'), script.indexOf('# END apt source'));
const LINE = 'deb [arch=amd64,arm64 signed-by=/usr/share/keyrings/coppice-archive-keyring.gpg] https://apt.getcoppice.app stable main\n';

function install(root, appDir) {
  const run = spawnSync('bash', ['-c', `ROOT=${root}\nAPP_DIR=${appDir}\n${block}`], { encoding: 'utf8' });
  assert.equal(run.status, 0, run.stderr);
}

test('postinst writes the source only when absent or still Coppice\'s copy', () => {
  const root = mkdtempSync(join(tmpdir(), 'coppice-postinst-'));
  try {
    const app = join(root, 'opt/Coppice');
    mkdirSync(join(app, 'resources/apt'), { recursive: true });
    writeFileSync(join(app, 'resources/apt/coppice-archive-keyring.gpg'), 'key');
    writeFileSync(join(app, 'resources/apt/coppice.list'), LINE);
    const list = join(root, 'etc/apt/sources.list.d/coppice.list');

    install(root, app);
    assert.equal(readFileSync(list, 'utf8'), LINE);

    // An upgrade with a new line replaces Coppice's own copy.
    const next = LINE.replace('stable', 'stable ');
    writeFileSync(join(app, 'resources/apt/coppice.list'), next);
    install(root, app);
    assert.equal(readFileSync(list, 'utf8'), next);

    // A source the user disabled stays disabled.
    writeFileSync(list, `# ${next}`);
    install(root, app);
    assert.equal(readFileSync(list, 'utf8'), `# ${next}`);

    // Older install with no record: an active Coppice line is replaced.
    rmSync(join(root, 'var'), { recursive: true });
    writeFileSync(list, LINE.replace('getcoppice.app', 'old.example'));
    install(root, app);
    assert.equal(readFileSync(list, 'utf8'), next);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
