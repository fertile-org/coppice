import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { ClaudeConsoleEntry } from './claude-console-state';
import { CONSOLE_RENDER_WINDOW, ClaudeConsoleView } from './ClaudeConsoleView';

describe('ClaudeConsoleView render window', () => {
  it('mounts only the newest entries and shows earlier ones on request', () => {
    const entries: ClaudeConsoleEntry[] = Array.from({ length: 1000 }, (_, i) => ({
      kind: 'text',
      id: `e${i}`,
      markdown: `Entry ${i}`,
    }));
    render(<ClaudeConsoleView entries={entries} legacyText="" />);
    expect(screen.getAllByText(/^Entry \d+$/)).toHaveLength(CONSOLE_RENDER_WINDOW);
    expect(screen.getByText('Entry 999')).toBeInTheDocument();
    expect(screen.queryByText('Entry 0')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Show earlier output (700)' }));
    expect(screen.getAllByText(/^Entry \d+$/)).toHaveLength(2 * CONSOLE_RENDER_WINDOW);
  });
});
