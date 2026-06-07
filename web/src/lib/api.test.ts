import { describe, it, expect } from 'vitest';
import { withCsrf } from './api';

describe('withCsrf', () => {
  it('adds X-CSRF-Token when token set', () => {
    const headers = withCsrf('abc', { 'Content-Type': 'application/json' });
    expect(headers['X-CSRF-Token']).toBe('abc');
  });
});
