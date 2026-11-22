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
  it('flags enabled probed issues and ignores healthy rows', () => {
    expect(connectorNeedsAttention(connector({ enabled: false, readiness: 'not_on_path' }))).toBe(
      false,
    );
    expect(connectorNeedsAttention(connector({ readiness: 'found' }))).toBe(false);
    expect(
      connectorNeedsAttention(
        connector({
          readiness: 'not_on_path',
          cli: { found: false, path: null, probe: { status: 'not_run', detail: null } },
        }),
      ),
    ).toBe(true);
    expect(connectorNeedsAttention(connector({ readiness: 'found_not_signed_in' }))).toBe(true);
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

  it('filters by search and visibility chips', () => {
    const { visible, counts } = visibleConnectors(all, 'code claude', 'all');
    expect(visible.map((c) => c.id)).toEqual(['claude-code']);
    expect(counts).toEqual({ all: 1, enabled: 1, disabled: 0, attention: 0 });

    expect(visibleConnectors(all, '', 'enabled').visible.map((c) => c.id)).toEqual([
      'codex',
      'claude-code',
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

  it('picks path, sign-in, probe, or last-test failures in order', () => {
    expect(
      attentionReason(
        connector({
          readiness: 'not_on_path',
          cli: { found: false, path: null, probe: { status: 'failed', detail: 'x' } },
        }),
      ),
    ).toContain('PATH');
    expect(attentionReason(connector({ readiness: 'found_not_signed_in' }))).toContain('signed in');
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
    ).toContain('Last test failed');
  });
});

describe('activeCheckId and agentCountLabel', () => {
  it('resolves the active check id and agent count copy', () => {
    const c = connector({
      lastCheck: { id: 'last', status: 'running', createdAt: PROBED_AT },
    });
    expect(activeCheckId(c, 'session')).toBe('last');
    expect(agentCountLabel(0)).toBe('No agents');
    expect(agentCountLabel(2)).toBe('2 agents');
  });
});
