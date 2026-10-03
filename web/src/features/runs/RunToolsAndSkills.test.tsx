import '@testing-library/jest-dom/vitest';
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RunToolsAndSkills } from './RunToolsAndSkills';

const { useRunToolCalls } = vi.hoisted(() => ({
  useRunToolCalls: vi.fn(),
}));

vi.mock('./useRunToolCalls', () => ({
  useRunToolCalls,
}));

const RUN_ID = '00000000-0000-4000-8000-000000000010';
const PLUGIN_ID = '00000000-0000-4000-8000-000000000020';

function call(overrides: Record<string, unknown>) {
  return {
    id: crypto.randomUUID(),
    tool: 'ticket_get',
    source: 'core',
    pluginId: null,
    pluginName: null,
    status: 'ok',
    error: null,
    durationMs: 3,
    argsSummary: '{}',
    createdAt: '2026-10-02T12:00:00Z',
    ...overrides,
  };
}

describe('RunToolsAndSkills', () => {
  it('lists calls and skills', () => {
    useRunToolCalls.mockReturnValue({
      data: {
        items: [
          call({ tool: 'skill_load', source: 'skill', durationMs: 12, argsSummary: '{"name":"coppice-git"}' }),
          call({
            tool: 'mcp-fake__echo',
            source: 'plugin',
            pluginId: PLUGIN_ID,
            pluginName: 'mcp-fake',
            status: 'error',
            error: 'plugin "mcp-fake" unavailable',
          }),
          call({ tool: 'comment_post', status: 'denied', error: 'denied: not allowed' }),
          call({ tool: 'orphan__tool', source: 'plugin', pluginId: PLUGIN_ID, pluginName: null }),
        ],
        skillsUsed: ['coppice-git', 'sample-plugin:hello'],
      },
      isLoading: false,
      isError: false,
    });

    render(<RunToolsAndSkills runId={RUN_ID} enabled />);

    expect(useRunToolCalls).toHaveBeenCalledWith(RUN_ID, true);
    const rows = screen.getAllByRole('listitem', { name: /tool call/i });
    expect(rows).toHaveLength(4);

    const skillRow = within(rows[0]);
    expect(skillRow.getByText('skill_load')).toBeVisible();
    expect(skillRow.getByText('Skill')).toBeVisible();
    expect(skillRow.getByText('ok')).toBeVisible();
    expect(skillRow.getByText('12 ms')).toBeVisible();

    const pluginRow = within(rows[1]);
    expect(pluginRow.getByText('mcp-fake__echo')).toBeVisible();
    expect(pluginRow.getByText('Plugin')).toBeVisible();
    expect(pluginRow.getByText('mcp-fake')).toBeVisible();
    expect(pluginRow.getByText('error')).toBeVisible();
    expect(pluginRow.getByText('plugin "mcp-fake" unavailable')).toBeVisible();

    const deniedRow = within(rows[2]);
    expect(deniedRow.getByText('Core')).toBeVisible();
    expect(deniedRow.getByText('denied')).toBeVisible();
    expect(deniedRow.getByText('denied: not allowed')).toBeVisible();

    expect(within(rows[3]).getByText(PLUGIN_ID)).toBeVisible();

    const skills = screen.getByRole('list', { name: 'Skills used' });
    expect(within(skills).getByText('coppice-git')).toBeVisible();
    expect(within(skills).getByText('sample-plugin:hello')).toBeVisible();
  });

  it('empty state', () => {
    useRunToolCalls.mockReturnValue({
      data: { items: [], skillsUsed: [] },
      isLoading: false,
      isError: false,
    });

    render(<RunToolsAndSkills runId={RUN_ID} enabled />);

    expect(screen.getByText('No tool calls recorded.')).toBeVisible();
    expect(screen.queryByRole('list', { name: 'Skills used' })).toBeNull();
  });

  it('shows loading and error states', () => {
    useRunToolCalls.mockReturnValue({ data: undefined, isLoading: true, isError: false });
    const { rerender } = render(<RunToolsAndSkills runId={RUN_ID} enabled />);
    expect(screen.getByText('Loading tool calls…')).toBeVisible();
    expect(screen.queryByText('No tool calls recorded.')).toBeNull();

    useRunToolCalls.mockReturnValue({ data: undefined, isLoading: false, isError: true });
    rerender(<RunToolsAndSkills runId={RUN_ID} enabled />);
    expect(screen.getByText('Unable to load tool calls.')).toBeVisible();
  });

  it('renders nothing and does not fetch while disabled', () => {
    useRunToolCalls.mockReturnValue({ data: undefined, isLoading: false, isError: false });
    const { container } = render(<RunToolsAndSkills runId={RUN_ID} enabled={false} />);
    expect(useRunToolCalls).toHaveBeenLastCalledWith(RUN_ID, false);
    expect(container).toBeEmptyDOMElement();
  });
});
