import assert from 'node:assert/strict';
import test from 'node:test';

import { DEFAULT_UPDATE_BASE, aptSourceLine, macUpdateFeed, pagesCname, updateBase } from '../src/updateEndpoints.mjs';

test('updateBase accepts an https origin and strips trailing slashes', () => {
  assert.equal(updateBase(undefined), DEFAULT_UPDATE_BASE);
  assert.equal(updateBase(''), DEFAULT_UPDATE_BASE);
  assert.equal(updateBase('   '), DEFAULT_UPDATE_BASE);
  assert.equal(updateBase('https://apt.example.com'), 'https://apt.example.com');
  assert.equal(updateBase('https://apt.example.com/'), 'https://apt.example.com');
  assert.equal(updateBase('https://apt.example.com///'), 'https://apt.example.com');
  assert.equal(updateBase('HTTPS://Apt.Example.com/'), 'https://apt.example.com');
  assert.equal(pagesCname('https://apt.example.com/'), 'apt.example.com');
  assert.equal(macUpdateFeed('https://apt.example.com/'), 'https://apt.example.com/updates/');
  assert.match(aptSourceLine('https://apt.example.com/'), / https:\/\/apt\.example\.com stable main$/);
});

test('updateBase rejects a bad COPPICE_UPDATE_BASE', () => {
  for (const value of [
    'http://apt.example.com',
    'https://apt.example.com/repo',
    'https://apt.example.com/repo/',
    'https://apt.example.com/foo/../',
    'https://apt.example.com/./',
    'https://apt.example.com//updates',
    'https://user:pass@apt.example.com',
    'https://apt.example.com?x=1',
    'https://apt.example.com#frag',
    'not a url',
    'apt.example.com',
  ]) {
    assert.throws(() => updateBase(value), /COPPICE_UPDATE_BASE/, value);
  }
});
