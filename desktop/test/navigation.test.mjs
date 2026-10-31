import test from 'node:test';
import assert from 'node:assert/strict';
import { isExternalHttpUrl, isSameOrigin, windowOpenAction } from '../src/navigation.mjs';

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

test('windowOpenAction keeps same-origin pages in the app window', () => {
  const origin = 'http://127.0.0.1:5123';
  assert.equal(windowOpenAction('http://127.0.0.1:5123/code?repoId=r1', origin), 'in-app');
  assert.equal(windowOpenAction('http://127.0.0.1:5124/code', origin), 'external');
  assert.equal(windowOpenAction('https://github.com/fertile-org/coppice', origin), 'external');
});

test('windowOpenAction opens ticket file content in a separate window', () => {
  const origin = 'http://127.0.0.1:5123';
  assert.equal(
    windowOpenAction(
      'http://127.0.0.1:5123/api/tickets/172edbce-6bee-4a43-8613-baa14b6c98d4/files/content?path=AppShell.test.tsx',
      origin,
    ),
    'file-content',
  );
  assert.equal(
    windowOpenAction('http://127.0.0.1:5123/api/tickets/abc/files', origin),
    'in-app',
  );
});

test('windowOpenAction denies non-http targets and windows without an origin', () => {
  assert.equal(windowOpenAction('file:///etc/passwd', 'http://127.0.0.1:5123'), 'deny');
  assert.equal(windowOpenAction('javascript:alert(1)', 'http://127.0.0.1:5123'), 'deny');
  assert.equal(windowOpenAction('http://127.0.0.1:5123/', null), 'external');
  assert.equal(windowOpenAction('file:///etc/passwd', null), 'deny');
});
