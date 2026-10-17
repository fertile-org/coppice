import { describe, expect, it } from 'vitest';
import { planCardState } from './planCardState';

const base = {
  skipPlanning: false,
  planApproved: false,
  latestPlanRunStatus: null as string | null,
};

describe('planCardState', () => {
  it('shows writing while a planning run is queued or running', () => {
    expect(
      planCardState({ ...base, status: 'backlog', latestPlanRunStatus: 'queued' }),
    ).toBe('writing');
    expect(
      planCardState({
        ...base,
        status: 'plan_review',
        latestPlanRunStatus: 'running',
      }),
    ).toBe('writing');
  });

  it('shows waiting on Plan Review when nothing is writing', () => {
    expect(planCardState({ ...base, status: 'plan_review' })).toBe('waiting');
    expect(
      planCardState({
        ...base,
        status: 'plan_review',
        latestPlanRunStatus: 'succeeded',
      }),
    ).toBe('waiting');
  });

  it('shows needs-run on Backlog and on Ready without an approved plan', () => {
    expect(planCardState({ ...base, status: 'backlog' })).toBe('needs-run');
    expect(
      planCardState({
        ...base,
        status: 'backlog',
        latestPlanRunStatus: 'failed',
      }),
    ).toBe('needs-run');
    expect(
      planCardState({
        ...base,
        status: 'ready',
        latestPlanRunStatus: 'succeeded',
      }),
    ).toBe('needs-run');
  });

  it('hides the labels when Skip planning is on or the plan is already approved', () => {
    expect(
      planCardState({ ...base, status: 'backlog', skipPlanning: true }),
    ).toBe('none');
    expect(
      planCardState({ ...base, status: 'ready', planApproved: true }),
    ).toBe('none');
    expect(planCardState({ ...base, status: 'in_progress' })).toBe('none');
  });

  it('says the ticket is waiting for answers while questions are open', () => {
    const planQuestions = {
      questions: [{ question: 'Which database?' }],
      answers: [],
      waiting: true,
    };
    expect(planCardState({ ...base, status: 'plan_review', planQuestions })).toBe('answers');
    expect(
      planCardState({
        ...base,
        status: 'plan_review',
        planQuestions: { ...planQuestions, waiting: false },
      }),
    ).toBe('waiting');
  });
});
