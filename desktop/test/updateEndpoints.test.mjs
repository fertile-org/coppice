import assert from 'node:assert/strict';
import test from 'node:test';

import { DEFAULT_UPDATE_BASE, aptSourceLine, macUpdateFeed, pagesCname, updateBase } from '../src/updateEndpoints.mjs';

test('updateBase accepts an https origin and strips trailing slashes', () => {
  assert.equal(updateBase(undefined), DEFAULT_UPDATE_BASE);
  assert.equal(DEFAULT_UPDATE_BASE, 'https://apt.getcoppice.app');
  assert.equal(updateBase('https://apt.getcoppice.app'), 'https://apt.getcoppice.app');
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
    'https://evil.com\\@x.com',
    'https://a\\b',
    'https://apt.example.com\\',
    'https://apt.example.com\\updates',
  ]) {
    assert.throws(() => updateBase(value), /COPPICE_UPDATE_BASE/, value);
  }
});

test('updateBase output is always a bare https origin', () => {
  for (const value of ['https://apt.example.com', 'https://apt.example.com:8443/', 'https://xn--bcher-kva.example/']) {
    const base = updateBase(value);
    const url = new URL(base);
    assert.equal(base, `https://${url.host}`, value);
    assert.equal(url.pathname, '/');
    assert.equal(url.search + url.hash + url.username + url.password, '');
  }
});
