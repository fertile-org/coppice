import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Plugin } from '../../lib/schemas/plugin';
import {
  AgentForm,
  agentToFormValues,
  type AgentFormValues,
  type PluginAssignmentState,
} from './AgentForm';
import type { Agent } from './useAgents';

const mocks = vi.hoisted(() => ({
  plugins: [] as Plugin[],
  modelProviders: [] as { id: string }[],
  models: [] as { id: string; name: string }[],
  modelsLoading: false,
}));

vi.mock('../plugins/usePlugins', () => ({
  usePlugins: () => ({ data: mocks.plugins, isLoading: false }),
}));

vi.mock('./useAgents', () => ({
  useModelProviders: () => ({ data: mocks.modelProviders, isLoading: false }),
  useModels: () => ({ data: mocks.models, isLoading: mocks.modelsLoading }),
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
  mode = 'edit',
  connectorOptions = [{ id: 'mock' }],
}: {
  initial: AgentFormValues;
  onSubmit: (values: AgentFormValues) => void;
  pluginAssignment?: PluginAssignmentState;
  mode?: 'create' | 'edit';
  connectorOptions?: { id: string; displayName?: string }[];
}) {
  const [values, setValues] = useState(initial);
  return (
    <MemoryRouter>
      <AgentForm
        mode={mode}
        values={values}
        onChange={setValues}
        onSubmit={onSubmit}
        onCancel={() => {}}
        connectorOptions={connectorOptions}
        pluginAssignment={pluginAssignment}
      />
    </MemoryRouter>
  );
}

describe('AgentForm model choice', () => {
  beforeEach(() => {
    mocks.plugins = [];
    mocks.modelProviders = [];
    mocks.models = [];
    mocks.modelsLoading = false;
  });

  it('hides an empty provider list and preselects the connector default', () => {
    mocks.modelProviders = [];
    render(
      <Harness
        mode="create"
        initial={baseValues({ connector: 'cursor' })}
        onSubmit={vi.fn()}
        connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
      />,
    );

    expect(screen.queryByLabelText('Model provider')).toBeNull();
    expect(screen.getByRole('combobox', { name: 'Model' })).toHaveTextContent(
      "Cursor's default",
    );
  });

  it('hides the provider field for one provider and preselects that connector default', () => {
    mocks.modelProviders = [{ id: 'cursor' }];
    mocks.models = [{ id: 'composer-2.5', name: 'Composer 2.5' }];
    render(
      <Harness
        mode="create"
        initial={baseValues({ connector: 'cursor' })}
        onSubmit={vi.fn()}
        connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
      />,
    );

    expect(screen.queryByLabelText('Model provider')).toBeNull();
    expect(screen.getByRole('combobox', { name: 'Model' })).toHaveTextContent(
      "Cursor's default",
    );
  });

  it('shows the provider field when several providers are available', () => {
    mocks.modelProviders = [{ id: 'sonnet' }, { id: 'opus' }, { id: 'haiku' }];
    render(
      <Harness
        mode="create"
        initial={baseValues({ connector: 'claude-code' })}
        onSubmit={vi.fn()}
        connectorOptions={[{ id: 'claude-code', displayName: 'Claude Code' }]}
      />,
    );

    expect(screen.getByLabelText('Model provider')).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Model' })).toHaveTextContent(
      "Claude Code's default",
    );
  });

  it('treats a blank saved model as the connector default', () => {
    const agent = {
      id: 'a',
      name: 'Builder',
      role: 'Developer',
      skills: [],
      responsibilities: [],
      systemPrompt: '',
      connector: 'cursor',
      modelProvider: '   ',
      model: '   ',
      health: 'healthy',
      enabled: true,
      createdAt: '',
      updatedAt: '',
    } satisfies Agent;
    expect(agentToFormValues(agent).model).toBe('');
    expect(agentToFormValues(agent).modelProvider).toBe('');
  });

  it('shows the connector default for an agent with no saved model', () => {
    mocks.modelProviders = [{ id: 'cursor' }];
    render(
      <Harness
        initial={baseValues({ connector: 'cursor', model: '', modelProvider: '' })}
        onSubmit={vi.fn()}
        connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
      />,
    );

    expect(screen.getByRole('combobox', { name: 'Model' })).toHaveTextContent(
      "Cursor's default",
    );
  });

  it('submits a blank model when the default stays selected', async () => {
    mocks.modelProviders = [{ id: 'cursor' }];
    const onSubmit = vi.fn();
    render(
      <Harness
        mode="create"
        initial={baseValues({ connector: 'cursor', systemPrompt: 'Help' })}
        onSubmit={onSubmit}
        connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].model).toBe('');
    expect(onSubmit.mock.calls[0][0].modelProvider).toBe('');
  });
});

describe('AgentForm plugins picker', () => {
  beforeEach(() => {
    mocks.plugins = [pluginA, pluginB, pluginC];
    mocks.modelProviders = [];
    mocks.models = [];
  });

  it('empty plugin list links to the Plugins page', () => {
    mocks.plugins = [pluginB];
    render(<Harness initial={baseValues()} onSubmit={vi.fn()} />);

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
      'https://getcoppice.vercel.app/docs/providers',
    );
    expect(screen.getByRole('link', { name: 'Install guide' })).toHaveAttribute('target', '_blank');
    expect(screen.getByText(/No agent CLI is ready yet/)).toBeInTheDocument();
  });
});
