import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { ToolPart as ToolPartType } from '../sync/types';
import { ToolPart } from './ToolPart';

function part(tool: string, input: Record<string, unknown>): ToolPartType {
  return {
    id: 'p1',
    type: 'tool',
    tool,
    messageID: 'm1',
    state: { status: 'completed', input },
  };
}

describe('ToolPart', () => {
  it('renders gateway tool title', () => {
    render(<ToolPart part={part('coppice_github__create_issue', { title: 'Bug' })} />);
    expect(screen.getByText(/github · create_issue/)).toBeVisible();
  });

  it('keeps the Bash renderer for bash', () => {
    render(<ToolPart part={part('bash', { command: 'cargo test' })} />);
    expect(screen.getByText(/cargo test/)).toBeVisible();
    expect(screen.queryByText(/·/)).not.toBeInTheDocument();
  });
});
