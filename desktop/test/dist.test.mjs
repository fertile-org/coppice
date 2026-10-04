import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { builderArgs } from '../scripts/dist.mjs';

describe('builderArgs', () => {
  it('passes --dir through and builds unsigned without CSC_LINK', () => {
    const { args, env } = builderArgs({}, { dir: true, platform: 'linux' });
    assert.ok(args.includes('--dir'));
    assert.equal(env.CSC_IDENTITY_AUTO_DISCOVERY, 'false');
  });

  it('notarizes on macOS when APPLE_API_KEY is set and keeps signing auto-discovery with CSC_LINK', () => {
    const { args, env } = builderArgs(
      { CSC_LINK: 'x', APPLE_API_KEY: 'k' },
      { dir: false, platform: 'darwin' },
    );
    assert.ok(args.includes('-c.mac.notarize=true'));
    assert.ok(!args.includes('--dir'));
    assert.equal(env.CSC_IDENTITY_AUTO_DISCOVERY, undefined);
    assert.equal(env.CSC_LINK, 'x');
  });

  it('does not notarize off macOS or without APPLE_API_KEY', () => {
    assert.ok(
      !builderArgs({ APPLE_API_KEY: 'k' }, { dir: false, platform: 'linux' }).args.includes(
        '-c.mac.notarize=true',
      ),
    );
    assert.ok(
      !builderArgs({ CSC_LINK: 'x' }, { dir: false, platform: 'darwin' }).args.includes(
        '-c.mac.notarize=true',
      ),
    );
  });

  it('never publishes from electron-builder', () => {
    const { args } = builderArgs({}, { dir: false, platform: 'linux' });
    assert.ok(args.includes('--publish=never'));
  });

  for (const empty of ['', '   ', '\n']) {
    it(`treats ${JSON.stringify(empty)} signing secrets as absent`, () => {
      const secrets = {
        CSC_LINK: empty,
        CSC_KEY_PASSWORD: empty,
        APPLE_API_KEY: empty,
        APPLE_API_KEY_ID: empty,
        APPLE_API_ISSUER: empty,
      };
      const { args, env } = builderArgs({ ...secrets, HOME: '/h' }, { dir: false, platform: 'darwin' });
      for (const key of Object.keys(secrets)) assert.ok(!(key in env), `${key} must be removed`);
      assert.equal(env.CSC_IDENTITY_AUTO_DISCOVERY, 'false');
      assert.equal(env.HOME, '/h');
      assert.ok(!args.includes('-c.mac.notarize=true'));
    });
  }

  it('notarizes only when both APPLE_API_KEY and CSC_LINK are set', () => {
    const notarize = (env) =>
      builderArgs(env, { dir: false, platform: 'darwin' }).args.includes('-c.mac.notarize=true');
    assert.equal(notarize({ APPLE_API_KEY: '/k.p8' }), false);
    assert.equal(notarize({ APPLE_API_KEY: '/k.p8', CSC_LINK: '' }), false);
    assert.equal(notarize({ APPLE_API_KEY: '/k.p8', CSC_LINK: 'x' }), true);
  });

  it('drops undefined env values', () => {
    const { env } = builderArgs({ CSC_LINK: undefined, HOME: '/h' }, { dir: false, platform: 'linux' });
    assert.ok(!('CSC_LINK' in env));
    assert.equal(env.HOME, '/h');
    assert.equal(env.CSC_IDENTITY_AUTO_DISCOVERY, 'false');
  });
});
