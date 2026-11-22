import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Plugin } from '../../lib/schemas/plugin';
import { openCombobox, selectComboboxOption } from '../../test/combobox';
import {
  AgentForm,
  agentToFormValues,
  type AgentFormValues,
  type PluginAssignmentState,
} from './AgentForm';
import { NOT_COVERED_BY_PLAN } from '../runs/reportedModel';
import type { Agent, ConnectorOption, ModelGroup } from './useAgents';

const mocks = vi.hoisted(() => ({
  plugins: [] as Plugin[],
  groups: [] as ModelGroup[],
  modelsLoading: false,
  modelsError: false,
  modelsDetail: null as string | null,
  modelsErrorMessage: null as string | null,
  refreshModels: vi.fn(),
  uncoveredIds: [] as string[],
}));

vi.mock('../plugins/usePlugins', () => ({
  usePlugins: () => ({ data: mocks.plugins, isLoading: false }),
}));

vi.mock('../tools/useUncoveredModels', () => ({
  useUncoveredModels: () => ({
    data: mocks.uncoveredIds,
    isLoading: false,
    isError: false,
  }),
}));

vi.mock('./useAgents', () => ({
  useConnectorModels: () => ({
    groups: mocks.groups,
    isLoading: mocks.modelsLoading,
    isFetching: false,
    isError: mocks.modelsError,
    detail: mocks.modelsDetail,
    errorMessage: mocks.modelsErrorMessage,
    refresh: mocks.refreshModels,
  }),
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
    agentAccess: 'explicit',
    agentIds: [],
    settings: [],
    unsupported: [],
    marketplace: null,
    external: null,
    gitRoot: null,
    builderMade: false,
    publishTarget: null,
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
    responsibilities: '',
    systemPrompt: '',
    connector: 'mock',
    modelProvider: '',
    model: '',
    thinkingLevel: '',
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
  connectorOptions?: ConnectorOption[];
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
    mocks.groups = [];
    mocks.modelsLoading = false;
    mocks.modelsError = false;
    mocks.modelsDetail = null;
    mocks.modelsErrorMessage = null;
    mocks.refreshModels.mockReset();
    mocks.uncoveredIds = [];
  });

  it('connector default model display and submit', async () => {
    mocks.groups = [{ provider: 'cursor', models: [{ id: 'auto', name: 'Auto' }] }];
    const agent = {
      id: 'a',
      name: 'Builder',
      role: 'Developer',
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

    const onSubmit = vi.fn();
    render(
      <Harness
        mode="create"
        initial={baseValues({ connector: 'cursor', model: '', modelProvider: '', systemPrompt: 'Help' })}
        onSubmit={onSubmit}
        connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
      />,
    );

    const trigger = screen.getByRole('combobox', { name: 'Model' });
    expect(trigger).toHaveTextContent("Cursor's default");
    fireEvent.click(screen.getByRole('button', { name: 'Refresh models' }));
    expect(mocks.refreshModels).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].model).toBe('');
    expect(onSubmit.mock.calls[0][0].modelProvider).toBe('');
  });

  it('marks a model the user checked as not covered', () => {
    mocks.groups = [
      {
        provider: 'cursor',
        models: [
          { id: 'auto', name: 'Auto' },
          { id: 'composer-2.5', name: 'Composer 2.5' },
        ],
      },
    ];
    mocks.uncoveredIds = ['composer-2.5'];
    render(
      <Harness
        mode="edit"
        initial={baseValues({
          connector: 'cursor',
          modelProvider: 'cursor',
          model: 'composer-2.5',
        })}
        onSubmit={vi.fn()}
        connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
      />,
    );

    const trigger = screen.getByRole('combobox', { name: 'Model' });
    expect(trigger).toHaveTextContent('Composer 2.5');
    expect(trigger).not.toHaveTextContent(NOT_COVERED_BY_PLAN);
    expect(screen.getByText(NOT_COVERED_BY_PLAN)).toBeVisible();

    const list = openCombobox(trigger);
    expect(within(list).getByRole('option', { name: /Composer 2.5/ })).toHaveTextContent(
      NOT_COVERED_BY_PLAN,
    );
    expect(within(list).getByRole('option', { name: 'Auto' })).not.toHaveTextContent(
      NOT_COVERED_BY_PLAN,
    );
  });

  it('lists one provider without a group heading and sets that provider when picked', async () => {
    mocks.groups = [
      {
        provider: 'cursor',
        models: [
          { id: 'auto', name: 'Auto' },
          { id: 'composer-2.5', name: 'Composer 2.5' },
        ],
      },
    ];
    const onSubmit = vi.fn();
    render(
      <Harness
        mode="create"
        initial={baseValues({ connector: 'cursor', systemPrompt: 'Help' })}
        onSubmit={onSubmit}
        connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
      />,
    );

    const trigger = screen.getByRole('combobox', { name: 'Model' });
    const list = openCombobox(trigger);
    expect(list.textContent).not.toContain('cursor');
    selectComboboxOption(trigger, 'Auto');
    expect(trigger).toHaveTextContent('Auto');

    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].model).toBe('auto');
    expect(onSubmit.mock.calls[0][0].modelProvider).toBe('cursor');
  });

  it('links a turned-off connector to System → Connectors', () => {
    mocks.modelsError = true;
    mocks.modelsErrorMessage =
      "Can't list Cursor's models because it's turned off in Connectors.";
    render(
      <Harness
        mode="edit"
        initial={baseValues({ connector: 'cursor' })}
        onSubmit={vi.fn()}
        connectorOptions={[]}
      />,
    );

    expect(screen.getByText(/Cursor is turned off\. Turn it on in/)).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Connectors' })).toHaveAttribute(
      'href',
      '/system/connectors',
    );
    expect(
      screen.queryByText("Couldn't get Cursor's models. Using Cursor's default for now."),
    ).toBeNull();
  });

});

describe('AgentForm thinking level', () => {
  beforeEach(() => {
    mocks.plugins = [];
    mocks.groups = [];
    mocks.modelsLoading = false;
    mocks.modelsError = false;
    mocks.modelsDetail = null;
    mocks.modelsErrorMessage = null;
    mocks.uncoveredIds = [];
  });

  const claudeCode: ConnectorOption = {
    id: 'claude-code',
    displayName: 'Claude Code',
    caps: { thinkingLevel: true },
  };

  it('is hidden for a connector without the capability', () => {
    render(
      <Harness
        initial={baseValues({ connector: 'cursor', systemPrompt: 'Help' })}
        onSubmit={vi.fn()}
        connectorOptions={[{ id: 'cursor', displayName: 'Cursor', caps: { thinkingLevel: false } }]}
      />,
    );

    expect(screen.queryByRole('combobox', { name: 'Thinking level' })).toBeNull();
  });

  it('defaults to the connector and submits the picked level', async () => {
    const onSubmit = vi.fn();
    render(
      <Harness
        initial={baseValues({ connector: 'claude-code', systemPrompt: 'Help' })}
        onSubmit={onSubmit}
        connectorOptions={[claudeCode]}
      />,
    );

    const trigger = screen.getByRole('combobox', { name: 'Thinking level' });
    expect(trigger).toHaveTextContent("Claude Code's default");
    selectComboboxOption(trigger, 'High');

    fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].thinkingLevel).toBe('high');
  });

  it('shows a saved level and keeps it when the connector changes', () => {
    const agent: Agent = {
      id: 'a1',
      name: 'Builder',
      role: 'Developer',
      responsibilities: [],
      systemPrompt: 'Help',
      connector: 'claude-code',
      thinkingLevel: 'max',
      health: 'healthy',
      enabled: true,
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    };
    const values = agentToFormValues(agent);
    expect(values.thinkingLevel).toBe('max');
    expect(agentToFormValues({ ...agent, thinkingLevel: null }).thinkingLevel).toBe('');

    render(
      <Harness
        initial={values}
        onSubmit={vi.fn()}
        connectorOptions={[claudeCode, { id: 'cursor', displayName: 'Cursor' }]}
      />,
    );
    expect(screen.getByRole('combobox', { name: 'Thinking level' })).toHaveTextContent('Max');
  });

  it('clears a hidden level on submit', async () => {
    const onSubmit = vi.fn();
    render(
      <Harness
        initial={baseValues({ connector: 'cursor', systemPrompt: 'Help', thinkingLevel: 'max' })}
        onSubmit={onSubmit}
        connectorOptions={[claudeCode, { id: 'cursor', displayName: 'Cursor' }]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].thinkingLevel).toBe('');
  });
});

describe('AgentForm plugins picker', () => {
  beforeEach(() => {
    mocks.plugins = [pluginA, pluginB, pluginC];
    mocks.groups = [];
    mocks.modelsError = false;
  });

  it('lists only enabled ok plugins', () => {
    render(<Harness initial={baseValues()} onSubmit={vi.fn()} />);

    expect(screen.getByRole('checkbox', { name: /alpha-plugin/ })).toBeEnabled();
    expect(screen.queryByRole('checkbox', { name: /beta-plugin/ })).toBeNull();
    expect(screen.queryByRole('checkbox', { name: /gamma-plugin/ })).toBeNull();
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

