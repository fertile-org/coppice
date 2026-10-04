import test from 'node:test';
import assert from 'node:assert/strict';
import { isExternalHttpUrl, isSameOrigin } from '../src/navigation.mjs';

test('isSameOrigin matches scheme, host and port exactly', () => {
  const origin = 'http://127.0.0.1:5123';
  assert.equal(isSameOrigin('http://127.0.0.1:5123/boards/1', origin), true);
  assert.equal(isSameOrigin('http://127.0.0.1:5124/', origin), false);
  assert.equal(isSameOrigin('http://localhost:5123/', origin), false);
  assert.equal(isSameOrigin('file:///etc/passwd', origin), false);
  assert.equal(isSameOrigin('not a url', origin), false);
});

test('isExternalHttpUrl accepts only http(s)', () => {
  assert.equal(isExternalHttpUrl('https://github.com/fertile-org/coppice'), true);
  assert.equal(isExternalHttpUrl('http://example.com'), true);
  assert.equal(isExternalHttpUrl('file:///etc/passwd'), false);
  assert.equal(isExternalHttpUrl('javascript:alert(1)'), false);
  assert.equal(isExternalHttpUrl('smb://host/share'), false);
  assert.equal(isExternalHttpUrl(''), false);
});
