import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, before, describe, it } from 'node:test';

import { targetForHost, verifySha256 } from '../scripts/fetch-postgres.mjs';

describe('verifySha256', () => {
  let dir;
  let file;
  const contents = 'coppice postgres bundle';
  const digest = createHash('sha256').update(contents).digest('hex');

  before(async () => {
    dir = await mkdtemp(join(tmpdir(), 'fetch-postgres-test-'));
    file = join(dir, 'archive.tar.gz');
    await writeFile(file, contents);
  });

  after(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('resolves when the hash matches', async () => {
    await verifySha256(file, digest);
  });

  it('rejects with checksum mismatch otherwise', async () => {
    await assert.rejects(verifySha256(file, '0'.repeat(64)), /checksum mismatch/);
  });
});

describe('targetForHost', () => {
  it('maps supported hosts to target triples', () => {
    assert.equal(targetForHost('darwin', 'arm64'), 'aarch64-apple-darwin');
    assert.equal(targetForHost('darwin', 'x64'), 'x86_64-apple-darwin');
    assert.equal(targetForHost('linux', 'x64'), 'x86_64-unknown-linux-gnu');
    assert.equal(targetForHost('linux', 'arm64'), 'aarch64-unknown-linux-gnu');
  });

  it('throws for unknown hosts', () => {
    assert.throws(() => targetForHost('win32', 'x64'));
    assert.throws(() => targetForHost('linux', 'ia32'));
  });
});
