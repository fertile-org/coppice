import { describe, expect, it } from 'vitest';
import type { ConnectorStatus } from '../../lib/schemas/connectorDiagnostics';
import {
  activeCheckId,
  agentCountLabel,
  attentionReason,
  connectorNeedsAttention,
  rowSecondaryText,
  visibleConnectors,
} from './connectorList';

const PROBED_AT = '2026-10-03T10:00:00Z';

function connector(overrides: Partial<ConnectorStatus> = {}): ConnectorStatus {
  const merged: ConnectorStatus = {
    id: 'x',
    displayName: 'X',
    enabled: true,
    cli: { found: true, path: '/usr/local/bin/x', probe: { status: 'ok', detail: 'x 1.0' } },
    auth: { status: 'detected', envSet: [], pathsFound: [] },
    authHint: '',
    docsUrl: '',
    lastRun: null,
    lastCheck: null,
    probedAt: PROBED_AT,
    readiness: 'found',
    ...overrides,
  };
  if (overrides.readiness === undefined && merged.probedAt) {
    merged.readiness = merged.cli.found ? 'found' : 'not_on_path';
  }
  return merged;
}

describe('connectorNeedsAttention', () => {
  it('never counts disabled, unprobed, or readiness found', () => {
    expect(
      connectorNeedsAttention(
        connector({
          enabled: false,
          readiness: 'not_on_path',
          cli: { found: false, path: null, probe: { status: 'not_run', detail: null } },
        }),
      ),
    ).toBe(false);
    expect(connectorNeedsAttention(connector({ probedAt: null, readiness: null }))).toBe(false);
    expect(connectorNeedsAttention(connector({ readiness: 'found' }))).toBe(false);
  });

  it('counts enabled probed not_on_path, found_not_signed_in, probe fail, last test fail', () => {
    expect(
      connectorNeedsAttention(
        connector({
          readiness: 'not_on_path',
          cli: { found: false, path: null, probe: { status: 'not_run', detail: null } },
        }),
      ),
    ).toBe(true);
    expect(connectorNeedsAttention(connector({ readiness: 'found_not_signed_in' }))).toBe(true);
    expect(
      connectorNeedsAttention(
        connector({
          readiness: 'ready',
          cli: { found: true, path: '/bin/x', probe: { status: 'failed', detail: 'boom' } },
        }),
      ),
    ).toBe(true);
    expect(
      connectorNeedsAttention(
        connector({
          readiness: 'ready',
          lastCheck: {
            id: 'c1',
            status: 'failed',
            failure: 'result_submit was not called',
            createdAt: PROBED_AT,
          },
        }),
      ),
    ).toBe(true);
  });
});

describe('visibleConnectors', () => {
  const ready = connector({ id: 'claude-code', displayName: 'Claude Code', readiness: 'ready' });
  const attention = connector({
    id: 'codex',
    displayName: 'Codex',
    readiness: 'found_not_signed_in',
  });
  const disabled = connector({
    id: 'kilo-code',
    displayName: 'Kilo Code',
    enabled: false,
    readiness: 'found',
  });
  const all = [disabled, ready, attention];

  it('search matches name and id with multi-term; counts follow search', () => {
    const { visible, counts } = visibleConnectors(all, 'code claude', 'all');
    expect(visible.map((c) => c.id)).toEqual(['claude-code']);
    expect(counts).toEqual({ all: 1, enabled: 1, disabled: 0, attention: 0 });
  });

  it('chips narrow the list; counts reflect the current search', () => {
    expect(visibleConnectors(all, '', 'all').counts).toEqual({
      all: 3,
      enabled: 2,
      disabled: 1,
      attention: 1,
    });
    expect(visibleConnectors(all, '', 'enabled').visible.map((c) => c.id)).toEqual([
      'codex',
      'claude-code',
    ]);
    expect(visibleConnectors(all, '', 'disabled').visible.map((c) => c.id)).toEqual([
      'kilo-code',
    ]);
    expect(visibleConnectors(all, '', 'attention').visible.map((c) => c.id)).toEqual(['codex']);
  });

  it('sorts needs attention, then enabled, then name', () => {
    const enabledB = connector({ id: 'b', displayName: 'Bravo', readiness: 'ready' });
    const enabledA = connector({ id: 'a', displayName: 'Alpha', readiness: 'ready' });
    const disabledZ = connector({
      id: 'z',
      displayName: 'Zulu',
      enabled: false,
      readiness: 'ready',
    });
    const attn = connector({
      id: 'attn',
      displayName: 'Needs Help',
      readiness: 'not_on_path',
      cli: { found: false, path: null, probe: { status: 'not_run', detail: null } },
    });
    const { visible } = visibleConnectors([enabledB, disabledZ, attn, enabledA], '', 'all');
    expect(visible.map((c) => c.id)).toEqual(['attn', 'a', 'b', 'z']);
  });
});

describe('rowSecondaryText', () => {
  it('prefers toggle error, live check, attention, checking, then readiness · version', () => {
    const base = connector({
      readiness: 'found_not_signed_in',
      lastCheck: {
        id: 'c1',
        status: 'failed',
        failure: 'ticket_get was not called',
        createdAt: PROBED_AT,
      },
    });
    expect(rowSecondaryText(base, 'Could not update.', undefined, false, false)).toEqual({
      text: 'Could not update.',
      alert: true,
      tone: 'danger',
    });
    expect(
      rowSecondaryText(
        base,
        null,
        { id: 'c2', connector: 'x', agentId: 'a', status: 'running', createdAt: PROBED_AT, toolCalls: [] },
        false,
        true,
      ),
    ).toEqual({ text: 'Running…', alert: false, tone: 'info' });
    expect(rowSecondaryText(base, null, undefined, false, false)).toEqual({
      text: 'Found, not signed in',
      alert: false,
      tone: 'warning',
    });
    expect(
      rowSecondaryText(connector({ probedAt: null, readiness: null }), null, undefined, false, false),
    ).toEqual({ text: 'Checking…', alert: false, tone: 'muted' });
    expect(
      rowSecondaryText(
        connector({ readiness: 'ready', cli: { found: true, path: '/bin/x', probe: { status: 'ok', detail: 'x 2.0' } } }),
        null,
        undefined,
        false,
        false,
      ),
    ).toEqual({ text: 'Ready · x 2.0', alert: false, tone: 'muted' });
  });

  it('attention reason order: path, sign-in, probe, last test', () => {
    expect(
      attentionReason(
        connector({
          readiness: 'not_on_path',
          cli: { found: false, path: null, probe: { status: 'failed', detail: 'x' } },
          lastCheck: { id: 'c', status: 'failed', failure: 'nope', createdAt: PROBED_AT },
        }),
      ),
    ).toBe('Not on your PATH');
    expect(attentionReason(connector({ readiness: 'found_not_signed_in' }))).toBe(
      'Found, not signed in',
    );
    expect(
      attentionReason(
        connector({
          readiness: 'ready',
          cli: { found: true, path: '/bin/x', probe: { status: 'timed_out', detail: null } },
        }),
      ),
    ).toBe('Probe timed out');
    expect(
      attentionReason(
        connector({
          readiness: 'ready',
          lastCheck: {
            id: 'c',
            status: 'failed',
            failure: 'result_submit was not called',
            createdAt: PROBED_AT,
          },
        }),
      ),
    ).toBe('Last test failed: result_submit was not called');
  });
});

describe('activeCheckId', () => {
  it('prefers an active lastCheck over the session id', () => {
    const c = connector({
      lastCheck: { id: 'last', status: 'running', createdAt: PROBED_AT },
    });
    expect(activeCheckId(c, 'session')).toBe('last');
    expect(activeCheckId(connector({ lastCheck: null }), 'session')).toBe('session');
    expect(
      activeCheckId(
        connector({ lastCheck: { id: 'done', status: 'passed', createdAt: PROBED_AT } }),
        'session',
      ),
    ).toBe('session');
  });
});

describe('agentCountLabel', () => {
  it('formats null, zero, one, and many', () => {
    expect(agentCountLabel(null)).toBe('…');
    expect(agentCountLabel(0)).toBe('No agents');
    expect(agentCountLabel(1)).toBe('1 agent');
    expect(agentCountLabel(2)).toBe('2 agents');
  });
});
