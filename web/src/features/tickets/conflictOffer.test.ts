import { describe, expect, it } from 'vitest';
import { ApiError } from '../../lib/api';
import { parseGitConflict, selectedConflictAgent } from './conflictOffer';

const ada = {
  id: 'ada-id',
  name: 'Ada',
  askLabel: 'Ask Ada to resolve',
};

const bea = {
  id: 'bea-id',
  name: 'Bea',
  askLabel: 'Ask Bea to resolve',
  unavailableReason: "Bea's connector isn't ready. Check it in System → Connectors.",
};

function conflictError(overrides: Record<string, unknown> = {}) {
  return new ApiError(
    400,
    JSON.stringify({
      message: "Couldn't merge into main because these files conflict: README.md. Nothing was changed.",
      conflict: {
        operation: 'merge',
        baseBranch: 'main',
        files: ['README.md'],
        canAskAssignee: true,
        askLabel: 'Ask Ada to resolve',
        defaultAgentId: 'ada-id',
        agents: [ada, bea],
        rereviewNote: 'When the conflicts are resolved, the ticket goes back to In Review.',
        ...overrides,
      },
    }),
  );
}

describe('parseGitConflict', () => {
  it('parses the picker fields', () => {
    const conflict = parseGitConflict(conflictError());
    expect(conflict?.defaultAgentId).toBe('ada-id');
    expect(conflict?.agents).toEqual([
      { ...ada, unavailableReason: null },
      bea,
    ]);
    expect(conflict?.canAskAssignee).toBe(true);
  });

  it('treats a missing agents list as empty', () => {
    const conflict = parseGitConflict(
      conflictError({ agents: undefined, defaultAgentId: undefined }),
    );
    expect(conflict?.agents).toEqual([]);
    expect(conflict?.defaultAgentId).toBeNull();
  });
});

describe('selectedConflictAgent', () => {
  it('prefers the selected id, then the default, then the first agent', () => {
    const conflict = parseGitConflict(conflictError())!;
    expect(selectedConflictAgent(conflict, 'bea-id')?.id).toBe('bea-id');
    expect(selectedConflictAgent(conflict, null)?.id).toBe('ada-id');
    expect(selectedConflictAgent({ ...conflict, defaultAgentId: null }, null)?.id).toBe(
      'ada-id',
    );
  });
});
