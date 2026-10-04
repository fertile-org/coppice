#!/usr/bin/env node
// Checks that an extracted Postgres bundle has everything Coppice needs.
//
//   node scripts/verify-postgres.mjs <dir>

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

const REQUIRED_BINARIES = ['initdb', 'pg_ctl', 'postgres', 'pg_dump', 'psql'];
const REQUIRED_MAJOR = 16;

function verify(dir) {
  const missing = [
    ...REQUIRED_BINARIES.map((name) => join('bin', name)),
    join('share', 'extension', 'unaccent.control'),
  ].filter((rel) => !existsSync(join(dir, rel)));
  if (missing.length > 0) {
    throw new Error(`postgres bundle at ${dir} is missing: ${missing.join(', ')}`);
  }

  const version = execFileSync(join(dir, 'bin', 'postgres'), ['--version'], {
    encoding: 'utf8',
  }).trim();
  const major = Number(version.match(/\(PostgreSQL\)\s+(\d+)/)?.[1]);
  if (major !== REQUIRED_MAJOR) {
    throw new Error(`expected PostgreSQL ${REQUIRED_MAJOR}, got: ${version}`);
  }
  console.log(`ok: ${version}`);
}

const dir = process.argv[2];
if (!dir) {
  console.error('usage: verify-postgres.mjs <dir>');
  process.exit(1);
}
try {
  verify(resolve(dir));
} catch (err) {
  console.error(err.message);
  process.exit(1);
}
