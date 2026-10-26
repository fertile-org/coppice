import type { NavigateFunction } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { codeReviewOrigin, openCodeReview } from './openCodeReview';

afterEach(() => {
  delete window.coppiceDesktop;
  window.history.replaceState({}, '', '/');
  vi.restoreAllMocks();
});

describe('openCodeReview', () => {
  it('records the current local URL when navigating inside the desktop app', () => {
    window.coppiceDesktop = { pickDirectory: async () => null };
    window.history.replaceState({}, '', '/boards/board-1?ticket=alpha#comments');
    const navigate = vi.fn();

    openCodeReview(
      '/code?repoId=repo-1&ticketId=alpha',
      navigate as unknown as NavigateFunction,
    );

    expect(navigate).toHaveBeenCalledWith('/code?repoId=repo-1&ticketId=alpha', {
      state: { codeReviewFrom: '/boards/board-1?ticket=alpha#comments' },
    });
  });

  it('keeps opening a new tab in the browser', () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null);
    const navigate = vi.fn();

    openCodeReview(
      '/code?repoId=repo-1',
      navigate as unknown as NavigateFunction,
    );

    expect(open).toHaveBeenCalledWith('/code?repoId=repo-1', '_blank', 'noopener,noreferrer');
    expect(navigate).not.toHaveBeenCalled();
  });
});

describe('codeReviewOrigin', () => {
  it('accepts local origins and rejects missing or external values', () => {
    expect(codeReviewOrigin({ codeReviewFrom: '/boards/board-1?ticket=alpha' })).toBe(
      '/boards/board-1?ticket=alpha',
    );
    expect(codeReviewOrigin(null)).toBeUndefined();
    expect(codeReviewOrigin({})).toBeUndefined();
    expect(codeReviewOrigin({ codeReviewFrom: 42 })).toBeUndefined();
    expect(codeReviewOrigin({ codeReviewFrom: 'https://example.com' })).toBeUndefined();
    expect(codeReviewOrigin({ codeReviewFrom: '//example.com' })).toBeUndefined();
    expect(codeReviewOrigin({ codeReviewFrom: '/\\evil.example' })).toBeUndefined();
    expect(codeReviewOrigin({ codeReviewFrom: '/\n/evil.example' })).toBeUndefined();
    expect(codeReviewOrigin({ codeReviewFrom: '/\t/evil.example' })).toBeUndefined();
  });
});
