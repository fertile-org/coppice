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

  it('drops undefined env values', () => {
    const { env } = builderArgs({ CSC_LINK: undefined, HOME: '/h' }, { dir: false, platform: 'linux' });
    assert.ok(!('CSC_LINK' in env));
    assert.equal(env.HOME, '/h');
    assert.equal(env.CSC_IDENTITY_AUTO_DISCOVERY, 'false');
  });
});
