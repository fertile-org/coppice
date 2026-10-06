import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Plugin } from '../../lib/schemas/plugin';
import {
  AgentForm,
  type AgentFormValues,
  type PluginAssignmentState,
} from './AgentForm';

const mocks = vi.hoisted(() => ({
  plugins: [] as Plugin[],
}));

vi.mock('../plugins/usePlugins', () => ({
  usePlugins: () => ({ data: mocks.plugins, isLoading: false }),
}));

vi.mock('./useAgents', () => ({
  useModelProviders: () => ({ data: [], isLoading: false }),
  useModels: () => ({ data: [], isLoading: false }),
}));

function plugin(overrides: Partial<Plugin>): Plugin {
  return {
    id: '00000000-0000-4000-8000-000000000010',
    pluginDirId: '00000000-0000-4000-8000-000000000001',
    relPath: 'plugin',
    name: 'plugin',
    version: '1.0.0',
    description: '',
    source: 'local',
    gitUrl: null,
    gitRef: null,
    gitCommit: null,
    status: 'ok',
    error: null,
    enabled: true,
    skills: [],
    mcpServers: [],
    settings: [],
    unsupported: [],
    marketplace: null,
    external: null,
    gitRoot: null,
    ...overrides,
  };
}

const pluginA = plugin({
  id: '00000000-0000-4000-8000-00000000000a',
  name: 'alpha-plugin',
});
const pluginB = plugin({
  id: '00000000-0000-4000-8000-00000000000b',
  name: 'beta-plugin',
  enabled: false,
});
const pluginC = plugin({
  id: '00000000-0000-4000-8000-00000000000c',
  name: 'gamma-plugin',
  status: 'shadowed',
});

function baseValues(overrides: Partial<AgentFormValues> = {}): AgentFormValues {
  return {
    name: 'Builder',
    role: 'Developer',
    skills: '',
    responsibilities: '',
    systemPrompt: '',
    connector: 'mock',
    modelProvider: '',
    model: '',
    enabled: true,
    pluginIds: [],
    ...overrides,
  };
}

function Harness({
  initial,
  onSubmit,
  pluginAssignment,
}: {
  initial: AgentFormValues;
  onSubmit: (values: AgentFormValues) => void;
  pluginAssignment?: PluginAssignmentState;
}) {
  const [values, setValues] = useState(initial);
  return (
    <AgentForm
      mode="edit"
      values={values}
      onChange={setValues}
      onSubmit={onSubmit}
      onCancel={() => {}}
      connectorOptions={[{ id: 'mock' }]}
      pluginAssignment={pluginAssignment}
    />
  );
}

describe('AgentForm plugins picker', () => {
  beforeEach(() => {
    mocks.plugins = [pluginA, pluginB, pluginC];
  });

  it('empty plugin list links to the Plugins page', () => {
    mocks.plugins = [pluginB];
    render(
      <MemoryRouter>
        <Harness initial={baseValues()} onSubmit={vi.fn()} />
      </MemoryRouter>,
    );

    expect(screen.getByRole('link', { name: 'Plugins' })).toHaveAttribute(
      'href',
      '/settings/plugins',
    );
  });

  it('lists only enabled ok plugins', () => {
    render(<Harness initial={baseValues()} onSubmit={vi.fn()} />);

    expect(screen.getByRole('checkbox', { name: /alpha-plugin/ })).toBeEnabled();
    expect(screen.queryByRole('checkbox', { name: /beta-plugin/ })).toBeNull();
    expect(screen.queryByRole('checkbox', { name: /gamma-plugin/ })).toBeNull();
  });

  it('agent picker counts enabled skills only', () => {
    const skill = (name: string, enabled: boolean) => ({
      name,
      description: '',
      relPath: `skills/${name}/SKILL.md`,
      error: null,
      enabled,
    });
    mocks.plugins = [
      { ...pluginA, skills: [skill('a', true), skill('b', false), skill('c', true)] },
    ];
    render(<Harness initial={baseValues()} onSubmit={vi.fn()} />);

    expect(screen.getByText('(2 skills)')).toBeInTheDocument();
  });

  it('submits selected plugin ids', async () => {
    const onSubmit = vi.fn();
    render(<Harness initial={baseValues()} onSubmit={onSubmit} />);

    fireEvent.click(screen.getByRole('checkbox', { name: /alpha-plugin/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].pluginIds).toEqual([pluginA.id]);
  });

  it('unavailable assigned plugin is flagged and dropped on save', async () => {
    const onSubmit = vi.fn();
    render(
      <Harness
        initial={baseValues({ pluginIds: [pluginA.id, pluginC.id] })}
        onSubmit={onSubmit}
      />,
    );

    const gamma = screen.getByRole('checkbox', { name: /gamma-plugin/ });
    expect(gamma).toBeDisabled();
    expect(screen.getByText(/will be removed on save/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].pluginIds).toEqual([pluginA.id]);
  });

  it('disables plugin picks while assigned plugins are loading', () => {
    render(
      <Harness
        initial={baseValues()}
        onSubmit={vi.fn()}
        pluginAssignment={{ status: 'loading' }}
      />,
    );

    expect(screen.getByRole('checkbox', { name: /alpha-plugin/ })).toBeDisabled();
    expect(screen.getByText(/Loading assigned plugins/)).toBeInTheDocument();
  });

  it('disables plugin picks and shows the error when assigned plugins fail to load', () => {
    render(
      <Harness
        initial={baseValues()}
        onSubmit={vi.fn()}
        pluginAssignment={{ status: 'error', message: 'boom' }}
      />,
    );

    expect(screen.getByRole('checkbox', { name: /alpha-plugin/ })).toBeDisabled();
    expect(
      screen.getByText(/Could not load this agent's plugins: boom/),
    ).toBeInTheDocument();
  });
});

describe('AgentForm connector hints', () => {
  it('shows the install hint and the no-ready hint', () => {
    render(
      <MemoryRouter>
        <AgentForm
          mode="create"
          values={baseValues({ connector: 'claude-code', name: 'Ada' })}
          onChange={() => {}}
          onSubmit={vi.fn()}
          onCancel={() => {}}
          connectorOptions={[
            {
              id: 'claude-code',
              displayName: 'Claude Code',
              readiness: 'not_on_path',
            },
            {
              id: 'codex',
              displayName: 'Codex',
              readiness: 'found_not_signed_in',
            },
          ]}
        />
      </MemoryRouter>,
    );

    expect(screen.getByText('Claude Code — Not on your PATH')).toBeInTheDocument();
    expect(
      screen.getByText(
        "Claude Code isn't installed on this machine yet. Install it and sign in, then this agent can run tickets. You can still save now.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Install guide' })).toHaveAttribute(
      'href',
      'https://fertile-org.github.io/coppice/docs/providers',
    );
    expect(screen.getByRole('link', { name: 'Install guide' })).toHaveAttribute('target', '_blank');
    expect(screen.getByText(/No agent CLI is ready yet/)).toBeInTheDocument();
  });
});
