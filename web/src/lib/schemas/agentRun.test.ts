import { describe, expect, it } from 'vitest';
import { agentRunSchema } from './agentRun';

const base = {
  id: '00000000-0000-4000-8000-000000000001',
  ticketId: '00000000-0000-4000-8000-000000000003',
  agentId: '00000000-0000-4000-8000-000000000002',
  jobType: 'work_on_ticket',
  status: 'running',
  sandboxProfileId: 'permissive-default',
  worktreePath: null,
  branchName: null,
  errorMessage: null,
  sessionId: null,
  startedAt: null,
  endedAt: null,
  createdAt: '2026-10-18T00:00:00Z',
  connector: 'opencode',
};

describe('agentRunSchema consoleKind', () => {
  it('keeps the run console format', () => {
    expect(agentRunSchema.parse({ ...base, consoleKind: 'structured' }).consoleKind).toBe(
      'structured',
    );
  });

  it('treats a missing or unknown format as the connector default', () => {
    expect(agentRunSchema.parse(base).consoleKind).toBeUndefined();
    expect(agentRunSchema.parse({ ...base, consoleKind: null }).consoleKind).toBeNull();
    expect(agentRunSchema.parse({ ...base, consoleKind: 'hologram' }).consoleKind).toBeNull();
  });
});
