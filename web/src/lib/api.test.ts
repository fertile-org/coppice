import { describe, it, expect } from 'vitest';
import { ApiError, parseApiErrorMessage, withCsrf } from './api';

describe('withCsrf', () => {
  it('adds X-CSRF-Token when token set', () => {
    const headers = withCsrf('abc', { 'Content-Type': 'application/json' });
    expect(headers['X-CSRF-Token']).toBe('abc');
  });
});

describe('parseApiErrorMessage', () => {
  it('reads the message field', () => {
    const err = new ApiError(400, '{"message":"bad input","error":"ignored"}');
    expect(parseApiErrorMessage(err)).toBe('bad input');
  });

  it('falls back to the error field when message is absent', () => {
    const err = new ApiError(409, '{"error":"repository not found"}');
    expect(parseApiErrorMessage(err)).toBe('repository not found');
  });

  it('returns plain-text bodies as-is', () => {
    expect(parseApiErrorMessage(new ApiError(500, 'boom'))).toBe('boom');
  });

  it('uses the fallback for empty bodies and non-API errors', () => {
    expect(parseApiErrorMessage(new ApiError(500, ''), 'fallback')).toBe('fallback');
    expect(parseApiErrorMessage(new Error('x'), 'fallback')).toBe('fallback');
  });
});
