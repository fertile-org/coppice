import test from 'node:test';
import assert from 'node:assert/strict';
import { parseReadyLine } from '../src/readyLine.mjs';

test('parses the ready line with trailing whitespace', () => {
  assert.equal(parseReadyLine('COPPICE_READY url=http://127.0.0.1:5123\n'), 'http://127.0.0.1:5123');
  assert.equal(parseReadyLine('COPPICE_READY url=http://127.0.0.1:80  \r\n'), 'http://127.0.0.1:80');
});

test('ignores other lines', () => {
  assert.equal(parseReadyLine('INFO listening'), null);
  assert.equal(parseReadyLine('COPPICE_READY url=http://example.com:5123'), null);
  assert.equal(parseReadyLine('x COPPICE_READY url=http://127.0.0.1:5123'), null);
});
