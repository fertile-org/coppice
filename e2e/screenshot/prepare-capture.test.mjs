import assert from 'node:assert/strict';
import { lstatSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';

const here = path.dirname(fileURLToPath(import.meta.url));
const prepare = path.join(here, 'prepare-capture.sh');
const remove = path.join(here, 'remove-capture.sh');
const example = path.resolve(here, '../../deploy/config/config.example.toml');

function captureDir() {
  return mkdtempSync(path.join(tmpdir(), 'coppice-screenshot.'));
}

function run(script, args) {
  return spawnSync(script, args, { encoding: 'utf8' });
}

describe('prepare-capture', () => {
  it('writes a new config and empty repos dir, and turns the host guard on', () => {
    const dir = captureDir();
    try {
      const result = run(prepare, [dir, example]);
      assert.equal(result.status, 0, result.stderr);
      const [repos, config] = result.stdout.trim().split('\n');
      assert.equal(repos, path.join(dir, 'repos'));
      assert.equal(config, path.join(dir, 'config.toml'));
      assert.equal(lstatSync(config).isSymbolicLink(), false);
      const text = readFileSync(config, 'utf8');
      assert.match(text, /desktop_allowed_hosts = \["127\.0\.0\.1:5000", "localhost:5000", "127\.0\.0\.1", "localhost", "127\.0\.0\.1:5001", "localhost:5001"\]/);
      assert.equal(text.includes('desktop_allowed_hosts'), true);
      const second = run(prepare, [dir, example]);
      assert.notEqual(second.status, 0);
      assert.match(second.stderr, /refusing to reuse/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('refuses a symlink config source and a symlink destination', () => {
    const dir = captureDir();
    const outside = mkdtempSync(path.join(tmpdir(), 'coppice-screenshot-outside.'));
    try {
      const linkedSource = path.join(outside, 'example-link.toml');
      symlinkSync(example, linkedSource);
      const fromLink = run(prepare, [dir, linkedSource]);
      assert.notEqual(fromLink.status, 0);
      assert.match(fromLink.stderr, /refusing symlink config source/);

      const dest = path.join(dir, 'config.toml');
      writeFileSync(path.join(outside, 'elsewhere.toml'), 'secret\n');
      symlinkSync(path.join(outside, 'elsewhere.toml'), dest);
      const intoLink = run(prepare, [dir, example]);
      assert.notEqual(intoLink.status, 0);
      assert.match(intoLink.stderr, /refusing to reuse/);
      assert.equal(readFileSync(path.join(outside, 'elsewhere.toml'), 'utf8'), 'secret\n');
    } finally {
      rmSync(dir, { recursive: true, force: true });
      rmSync(outside, { recursive: true, force: true });
    }
  });
});

describe('remove-capture', () => {
  it('removes a private capture directory and refuses other paths', () => {
    const dir = captureDir();
    const result = run(remove, [dir]);
    assert.equal(result.status, 0, result.stderr);
    assert.throws(() => lstatSync(dir));

    const kept = mkdtempSync(path.join(tmpdir(), 'not-a-capture.'));
    try {
      const refused = run(remove, [kept]);
      assert.notEqual(refused.status, 0);
      assert.match(refused.stderr, /unexpected directory name/);
      assert.equal(lstatSync(kept).isDirectory(), true);
    } finally {
      rmSync(kept, { recursive: true, force: true });
    }

    const linkParent = mkdtempSync(path.join(tmpdir(), 'coppice-screenshot-link.'));
    const target = mkdtempSync(path.join(tmpdir(), 'coppice-screenshot-target.'));
    try {
      const link = path.join(linkParent, 'coppice-screenshot.linked');
      symlinkSync(target, link);
      const refused = run(remove, [link]);
      assert.notEqual(refused.status, 0);
      assert.match(refused.stderr, /symlink/);
      assert.equal(lstatSync(target).isDirectory(), true);
    } finally {
      rmSync(linkParent, { recursive: true, force: true });
      rmSync(target, { recursive: true, force: true });
    }
  });
});
