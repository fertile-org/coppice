import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { KnowledgeSettings } from '../../lib/schemas/settings';
import { openCombobox, selectComboboxOption } from '../../test/combobox';
import { KnowledgeCompactionCard } from './KnowledgeCompactionCard';
import type { Agent } from './useAgents';

const mocks = vi.hoisted(() => ({
  role: 'admin',
  settings: null as KnowledgeSettings | null,
  queuedCount: 12,
  update: vi.fn(),
}));

vi.mock('../auth/useSession', () => ({
  useSession: () => ({ user: { id: 'u1', email: 'a@b.c', role: mocks.role } }),
}));

vi.mock('../settings/useKnowledgeSettings', () => ({
  useKnowledgeSettings: () => ({
    data: mocks.settings,
    isLoading: false,
    isError: false,
  }),
  useUpdateKnowledgeSettings: () => ({
    mutateAsync: mocks.update,
    isPending: false,
  }),
}));

vi.mock('../knowledge/useCompaction', () => ({
  useCompactionStatus: () => ({
    data: {
      queuedCount: mocks.queuedCount,
      intervalSecs: 1800,
      batchMaxTickets: 10,
    },
  }),
}));

function agent(overrides: Partial<Agent>): Agent {
  return {
    id: '00000000-0000-4000-8000-000000000001',
    name: 'Reviewer',
    role: 'Reviewer',
    skills: [],
    responsibilities: [],
    systemPrompt: '',
    connector: 'claude-code',
    health: 'healthy',
    enabled: true,
    createdAt: '2026-09-01T00:00:00Z',
    updatedAt: '2026-09-01T00:00:00Z',
    ...overrides,
  };
}

const reviewer = agent({});
const coder = agent({
  id: '00000000-0000-4000-8000-000000000002',
  name: 'Coder',
  connector: 'codex',
});

function settings(compactionAgent: Agent | null): KnowledgeSettings {
  return {
    compactionAgentId: compactionAgent?.id ?? null,
    compactionAgent: compactionAgent
      ? {
          id: compactionAgent.id,
          name: compactionAgent.name,
          enabled: compactionAgent.enabled,
          connector: compactionAgent.connector,
        }
      : null,
    readOnlyConnectors: ['mock', 'claude-code', 'cursor'],
  };
}

function renderCard(agents: Agent[], { hash = '', onCreateAgent = vi.fn() } = {}) {
  render(
    <MemoryRouter initialEntries={[`/agents${hash}`]}>
      <KnowledgeCompactionCard
        agents={agents}
        canCreateAgent
        onCreateAgent={onCreateAgent}
      />
    </MemoryRouter>,
  );
  return { onCreateAgent };
}

beforeEach(() => {
  mocks.role = 'admin';
  mocks.settings = settings(null);
  mocks.queuedCount = 12;
  mocks.update.mockReset();
  mocks.update.mockResolvedValue(settings(reviewer));
});

describe('KnowledgeCompactionCard', () => {
  it('asks for an agent first when none exist', () => {
    const { onCreateAgent } = renderCard([]);
    expect(screen.getByLabelText('Compaction agent')).toBeDisabled();
    expect(screen.getByText('Create an agent first.')).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'New agent' }));
    expect(onCreateAgent).toHaveBeenCalled();
  });

  it('defaults to off and saves the chosen agent', async () => {
    renderCard([reviewer, coder]);
    const select = screen.getByLabelText('Compaction agent');
    expect(select).toHaveTextContent('None — compaction off');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    selectComboboxOption(select, /Reviewer \(claude-code\)/);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(mocks.update).toHaveBeenCalledWith(reviewer.id));
  });

  it('marks connectors without read-only mode as unusable', () => {
    renderCard([reviewer, coder]);
    const list = openCombobox(screen.getByLabelText('Compaction agent'));
    const option = within(list).getByRole('option', { name: /Coder \(codex\)/ });
    expect(option).toHaveAttribute('aria-disabled', 'true');
    expect(option).toHaveTextContent('Cannot run compaction (no read-only mode)');
  });

  it('shows a paused hint for a disabled compaction agent', () => {
    const disabled = agent({ enabled: false });
    mocks.settings = settings(disabled);
    renderCard([disabled]);
    expect(screen.getByLabelText('Compaction agent')).toHaveTextContent(
      'Reviewer (claude-code) (disabled)',
    );
    expect(screen.getByText('This agent is disabled; compaction is paused.')).toBeVisible();
  });

  it('shows the value read-only to members', () => {
    mocks.role = 'member';
    mocks.settings = settings(reviewer);
    renderCard([reviewer]);
    expect(screen.getByText('Reviewer (claude-code)')).toBeVisible();
    expect(screen.queryByRole('combobox')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Save' })).toBeNull();
  });

  it('describes cadence and the waiting backlog', () => {
    renderCard([reviewer]);
    expect(
      screen.getByText(
        /Runs every 30 min in batches of up to 10 tickets, read-only, no repo\. 12 Done tickets are waiting\./,
      ),
    ).toBeVisible();
  });

  it('focuses the select when opened from the Knowledge page CTA', () => {
    renderCard([reviewer], { hash: '#knowledge-compaction' });
    expect(screen.getByLabelText('Compaction agent')).toHaveFocus();
  });
});
