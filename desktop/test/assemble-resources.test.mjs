import assert from 'node:assert/strict';
import { access, chmod, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, before, describe, it } from 'node:test';

import {
  assembleResources,
  isBaseLib,
  missingLibsMessage,
  parseLdd,
  planLibCopies,
} from '../scripts/assemble-resources.mjs';

const LDD_POSTGRES = `\tlinux-vdso.so.1 (0x00007ffd0b5f2000)
\tlibzstd.so.1 => /usr/lib/x86_64-linux-gnu/libzstd.so.1 (0x00007f1c2a000000)
\tlibxml2.so.2 => not found
\tlibpq.so.5 => /opt/pg/bin/../lib/libpq.so.5 (0x00007f1c29e00000)
\tlibm.so.6 => /usr/lib/x86_64-linux-gnu/libm.so.6 (0x00007f1c29d00000)
\tlibc.so.6 => /usr/lib/x86_64-linux-gnu/libc.so.6 (0x00007f1c29a00000)
\t/lib64/ld-linux-x86-64.so.2 (0x00007f1c2a200000)
`;

describe('parseLdd', () => {
  it('splits resolved and missing libraries and ignores vdso and the loader', () => {
    assert.deepEqual(parseLdd(LDD_POSTGRES), {
      resolved: [
        { name: 'libzstd.so.1', path: '/usr/lib/x86_64-linux-gnu/libzstd.so.1' },
        { name: 'libpq.so.5', path: '/opt/pg/bin/../lib/libpq.so.5' },
        { name: 'libm.so.6', path: '/usr/lib/x86_64-linux-gnu/libm.so.6' },
        { name: 'libc.so.6', path: '/usr/lib/x86_64-linux-gnu/libc.so.6' },
      ],
      missing: ['libxml2.so.2'],
    });
  });

  it('returns nothing for statically linked objects', () => {
    assert.deepEqual(parseLdd('\tstatically linked\n'), { resolved: [], missing: [] });
  });
});

describe('isBaseLib', () => {
  it('treats glibc and toolchain runtime libraries as base', () => {
    for (const name of [
      'linux-vdso.so.1',
      'ld-linux-x86-64.so.2',
      'ld-linux-aarch64.so.1',
      'libc.so.6',
      'libm.so.6',
      'libpthread.so.0',
      'libdl.so.2',
      'librt.so.1',
      'libutil.so.1',
      'libresolv.so.2',
      'libgcc_s.so.1',
      'libstdc++.so.6',
    ]) {
      assert.ok(isBaseLib(name), name);
    }
  });

  it('bundles everything else', () => {
    for (const name of ['libxml2.so.2', 'libreadline.so.8', 'libicuuc.so.70', 'libcrypto.so.3', 'libcom_err.so.2']) {
      assert.ok(!isBaseLib(name), name);
    }
  });
});

describe('planLibCopies', () => {
  it('copies non-base libs not already bundled, once each, and collects missing ones', () => {
    const plan = planLibCopies(
      [
        { file: 'bin/postgres', ldd: parseLdd(LDD_POSTGRES) },
        {
          file: 'bin/psql',
          ldd: parseLdd(
            '\tlibreadline.so.8 => /usr/lib/libreadline.so.8 (0x1)\n\tlibzstd.so.1 => /usr/lib/x86_64-linux-gnu/libzstd.so.1 (0x2)\n',
          ),
        },
      ],
      new Set(['libpq.so.5']),
    );
    assert.deepEqual(plan.copy, [
      { name: 'libzstd.so.1', path: '/usr/lib/x86_64-linux-gnu/libzstd.so.1' },
      { name: 'libreadline.so.8', path: '/usr/lib/libreadline.so.8' },
    ]);
    assert.deepEqual(plan.missing, [{ file: 'bin/postgres', name: 'libxml2.so.2' }]);
  });

  it('does not report a missing lib that is already bundled', () => {
    const plan = planLibCopies(
      [{ file: 'lib/dblink.so', ldd: parseLdd('\tlibpq.so.5 => not found\n') }],
      new Set(['libpq.so.5']),
    );
    assert.deepEqual(plan, { copy: [], missing: [] });
  });
});

describe('missingLibsMessage', () => {
  it('names each unresolved library and the file that needs it', () => {
    const msg = missingLibsMessage([
      { file: 'bin/postgres', name: 'libxml2.so.2' },
      { file: 'bin/psql', name: 'libreadline.so.8' },
    ]);
    assert.match(msg, /libxml2\.so\.2 \(needed by bin\/postgres\)/);
    assert.match(msg, /libreadline\.so\.8 \(needed by bin\/psql\)/);
  });
});

describe('assembleResources', () => {
  let dir;
  let out;

  async function exists(path) {
    try {
      await access(path);
      return true;
    } catch {
      return false;
    }
  }

  before(async () => {
    dir = await mkdtemp(join(tmpdir(), 'assemble-test-'));
    const repo = join(dir, 'repo');
    await mkdir(join(repo, 'server', 'agent_templates'), { recursive: true });
    await writeFile(join(repo, 'server', 'agent_templates', 'pm.md'), 'pm');
    await mkdir(join(repo, 'fixtures', 'agent-responses'), { recursive: true });
    await writeFile(join(repo, 'fixtures', 'agent-responses', 'done.json'), '{}');

    const serverBin = join(dir, 'coppice-server');
    await writeFile(serverBin, '#!/bin/sh\n');
    await chmod(serverBin, 0o644);

    const web = join(dir, 'web-dist');
    await mkdir(web);
    await writeFile(join(web, 'index.html'), '<div id="root"></div>');

    const pg = join(dir, 'pg');
    for (const sub of ['bin', 'lib', 'share/extension', 'include']) {
      await mkdir(join(pg, sub), { recursive: true });
    }
    await writeFile(join(pg, 'bin', 'postgres'), '#!/bin/sh\n');
    await chmod(join(pg, 'bin', 'postgres'), 0o755);
    for (const name of ['unaccent.so', 'plpython3.so', 'jsonb_plpython3.so', 'uuid-ossp.so', 'libpq.a']) {
      await writeFile(join(pg, 'lib', name), '');
    }
    for (const name of ['unaccent.control', 'plpython3u.control', 'plpython3u--1.0.sql', 'uuid-ossp.control']) {
      await writeFile(join(pg, 'share', 'extension', name), '');
    }

    out = join(dir, 'out');
    await mkdir(join(out, 'stale'), { recursive: true });
    await assembleResources({ serverBin, webDist: web, postgres: pg, out, repoRoot: repo, bundleLibs: false });
  });

  after(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('lays out the resources the desktop runtime expects', async () => {
    assert.equal(await readFile(join(out, 'web', 'index.html'), 'utf8'), '<div id="root"></div>');
    assert.equal(await readFile(join(out, 'agent-templates', 'pm.md'), 'utf8'), 'pm');
    assert.equal(await readFile(join(out, 'fixtures', 'agent-responses', 'done.json'), 'utf8'), '{}');
    assert.ok(await exists(join(out, 'postgres', 'lib', 'unaccent.so')));
    assert.ok(await exists(join(out, 'postgres', 'share', 'extension', 'unaccent.control')));
    assert.ok(!(await exists(join(out, 'stale'))));
  });

  it('keeps binaries executable', async () => {
    assert.equal((await stat(join(out, 'bin', 'coppice-server'))).mode & 0o777, 0o755);
    assert.equal((await stat(join(out, 'postgres', 'bin', 'postgres'))).mode & 0o111, 0o111);
  });

  it('drops build-only files and modules whose runtime is not bundled', async () => {
    for (const rel of [
      'postgres/lib/plpython3.so',
      'postgres/lib/jsonb_plpython3.so',
      'postgres/lib/uuid-ossp.so',
      'postgres/lib/libpq.a',
      'postgres/include',
      'postgres/share/extension/plpython3u.control',
      'postgres/share/extension/plpython3u--1.0.sql',
      'postgres/share/extension/uuid-ossp.control',
    ]) {
      assert.ok(!(await exists(join(out, rel))), rel);
    }
  });
});
