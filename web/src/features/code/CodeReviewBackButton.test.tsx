import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { describe, expect, it } from 'vitest';
import { CodeReviewBackButton } from './CodeReviewBackButton';

function LocationProbe() {
  const location = useLocation();
  return (
    <output data-testid="location">
      {location.pathname}{location.search}{location.hash}
    </output>
  );
}

describe('CodeReviewBackButton', () => {
  it('returns to the page that opened Code Review', () => {
    render(
      <MemoryRouter
        initialEntries={[
          {
            pathname: '/code',
            state: { codeReviewFrom: '/boards/board-1?ticket=alpha#comments' },
          },
        ]}
      >
        <Routes>
          <Route
            path="/code"
            element={<CodeReviewBackButton fallbackHref="/settings/repositories" />}
          />
          <Route path="*" element={<LocationProbe />} />
        </Routes>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Back' }));

    expect(screen.getByTestId('location')).toHaveTextContent(
      '/boards/board-1?ticket=alpha#comments',
    );
  });

  it('uses the supplied ticket fallback for a direct link', () => {
    render(
      <MemoryRouter initialEntries={['/code']}>
        <Routes>
          <Route
            path="/code"
            element={
              <CodeReviewBackButton fallbackHref="/boards/board-1?ticket=alpha" />
            }
          />
          <Route path="*" element={<LocationProbe />} />
        </Routes>
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Back' }));

    expect(screen.getByTestId('location')).toHaveTextContent(
      '/boards/board-1?ticket=alpha',
    );
  });
});