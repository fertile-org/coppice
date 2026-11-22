import { describe, expect, it } from 'vitest';
import { parseResultContractFromText } from './parse-result-contract';

const doneContract = {
  status: 'done',
  summary: '## Research complete\n\nFound competitors.',
  changedFiles: ['docs/research/competitive-analysis.md'],
  testsRun: [],
  nextStatus: 'In Review',
  mentionAgents: [],
  blockers: [],
};

describe('parseResultContractFromText', () => {
  it('parses bare and fenced JSON', () => {
    for (const text of [
      JSON.stringify(doneContract),
      '```json\n' + JSON.stringify(doneContract, null, 2) + '\n```',
    ]) {
      const result = parseResultContractFromText(text);
      expect(result?.status).toBe('done');
      expect(result?.summary).toContain('Research complete');
    }
  });

  it('skips template placeholder contracts', () => {
    const template = {
      status: 'done',
      summary: '<markdown summary>',
      changedFiles: ['<paths>'],
      testsRun: [],
      nextStatus: 'In Review',
      mentionAgents: [],
      blockers: [],
    };
    const real = { ...doneContract };
    const text =
      'Templates:\n```json\n' +
      JSON.stringify(template) +
      '\n```\n\n```json\n' +
      JSON.stringify(real) +
      '\n```';
    const result = parseResultContractFromText(text);
    expect(result?.summary).toContain('Research complete');
  });

  it('parses blocked, optional fields, and the last duplicate JSON blob', () => {
    const blocked = {
      status: 'blocked',
      blockerType: 'missing_secret',
      summary: 'Need DB_READONLY_URL',
      nextStatus: 'Blocked',
      mentionAgents: ['owner'],
    };
    const blockedResult = parseResultContractFromText(JSON.stringify(blocked));
    expect(blockedResult?.status).toBe('blocked');
    if (blockedResult?.status === 'blocked') {
      expect(blockedResult.blockerType).toBe('missing_secret');
    }

    const contract = {
      status: 'done',
      summary: 'Refined ticket.',
      acceptanceCriteria: '- Must pass CI\n- Must include tests',
      assignTo: 'backend_engineer',
      changedFiles: [],
      testsRun: [],
      mentionAgents: [],
      blockers: [],
    };
    const doneResult = parseResultContractFromText(JSON.stringify(contract));
    if (doneResult?.status === 'done') {
      expect(doneResult.acceptanceCriteria).toContain('Must pass CI');
      expect(doneResult.assignTo).toBe('backend_engineer');
    }

    const json = JSON.stringify(doneContract);
    const last = parseResultContractFromText(json + json + json);
    expect(last?.summary).toContain('Research complete');
  });
});
