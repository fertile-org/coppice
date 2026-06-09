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
  it('parses bare JSON', () => {
    const result = parseResultContractFromText(JSON.stringify(doneContract));
    expect(result?.status).toBe('done');
    expect(result?.summary).toContain('Research complete');
  });

  it('parses fenced json code block', () => {
    const text = '```json\n' + JSON.stringify(doneContract, null, 2) + '\n```';
    const result = parseResultContractFromText(text);
    expect(result?.status).toBe('done');
    if (result?.status === 'done') {
      expect(result.changedFiles).toEqual(['docs/research/competitive-analysis.md']);
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

  it('parses blocked contract', () => {
    const blocked = {
      status: 'blocked',
      blockerType: 'missing_secret',
      summary: 'Need DB_READONLY_URL',
      nextStatus: 'Blocked',
      mentionAgents: ['owner'],
    };
    const result = parseResultContractFromText(JSON.stringify(blocked));
    expect(result?.status).toBe('blocked');
    if (result?.status === 'blocked') {
      expect(result.blockerType).toBe('missing_secret');
    }
  });

  it('returns null for normal markdown', () => {
    expect(parseResultContractFromText('Just a regular update.')).toBeNull();
  });
});
