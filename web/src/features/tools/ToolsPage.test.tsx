import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import { ToolsPage } from './ToolsPage';

vi.mock('../auth/useSession', () => ({
  useSession: () => ({
    user: { id: '1', email: 'admin@localhost', role: 'admin' },
  }),
}));

describe('ToolsPage', () => {
  it('shows export and import sections for admins', () => {
    render(
      <MemoryRouter>
        <ToolsPage />
      </MemoryRouter>,
    );

    expect(screen.getByRole('heading', { name: 'Tools' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Download backup' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Import backup' }),
    ).toBeInTheDocument();
  });
});
