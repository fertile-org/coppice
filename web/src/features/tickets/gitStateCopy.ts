export type TicketGitState =
  | 'in_review'
  | 'not_merged'
  | 'conflict'
  | 'main_dirty'
  | 'changed_after_review'
  | 'not_pushed'
  | 'worktree_kept'
  | 'merged';

export type GitStateTone = 'neutral' | 'warning' | 'danger' | 'success';

export const gitStateCopy: Record<
  TicketGitState,
  { label: string; primary: string | null; tone: GitStateTone }
> = {
  in_review: { label: 'In review', primary: 'Rebase…', tone: 'neutral' },
  not_merged: { label: 'Not merged', primary: 'Merge', tone: 'warning' },
  conflict: { label: 'Conflict', primary: 'Ask agent', tone: 'danger' },
  main_dirty: { label: 'Main changed', primary: 'Retry merge', tone: 'warning' },
  changed_after_review: {
    label: 'Changed after review',
    primary: null,
    tone: 'warning',
  },
  not_pushed: { label: 'Not pushed', primary: 'Push main', tone: 'warning' },
  worktree_kept: {
    label: 'Worktree kept',
    primary: 'Remove worktree',
    tone: 'neutral',
  },
  merged: { label: 'Merged', primary: null, tone: 'success' },
};
