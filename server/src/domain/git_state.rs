//! Server-owned git state of a ticket, derived from the acceptance row.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketGitState {
    InReview,
    NotMerged,
    Conflict,
    MainDirty,
    ChangedAfterReview,
    NotPushed,
    WorktreeKept,
    Merged,
}

impl TicketGitState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InReview => "in_review",
            Self::NotMerged => "not_merged",
            Self::Conflict => "conflict",
            Self::MainDirty => "main_dirty",
            Self::ChangedAfterReview => "changed_after_review",
            Self::NotPushed => "not_pushed",
            Self::WorktreeKept => "worktree_kept",
            Self::Merged => "merged",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        Some(match s {
            "in_review" => Self::InReview,
            "not_merged" => Self::NotMerged,
            "conflict" => Self::Conflict,
            "main_dirty" => Self::MainDirty,
            "changed_after_review" => Self::ChangedAfterReview,
            "not_pushed" => Self::NotPushed,
            "worktree_kept" => Self::WorktreeKept,
            "merged" => Self::Merged,
            _ => return None,
        })
    }

    pub fn needs_action(self) -> bool {
        !matches!(self, Self::InReview | Self::Merged)
    }
}

/// A failed step of the accept chain, stored in `git_outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOutcome {
    Conflict,
    MainDirty,
    NotPushed,
    WorktreeKept,
}

impl GitOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conflict => "conflict",
            Self::MainDirty => "main_dirty",
            Self::NotPushed => "not_pushed",
            Self::WorktreeKept => "worktree_kept",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        Some(match s {
            "conflict" => Self::Conflict,
            "main_dirty" => Self::MainDirty,
            "not_pushed" => Self::NotPushed,
            "worktree_kept" => Self::WorktreeKept,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitStateInput {
    pub accepted: bool,
    pub stale: bool,
    pub merged: bool,
    pub outcome: Option<GitOutcome>,
}

pub fn derive_git_state(input: &GitStateInput) -> TicketGitState {
    if !input.accepted {
        return TicketGitState::InReview;
    }
    if input.stale {
        return TicketGitState::ChangedAfterReview;
    }
    if input.merged {
        return match input.outcome {
            Some(GitOutcome::NotPushed) => TicketGitState::NotPushed,
            Some(GitOutcome::WorktreeKept) => TicketGitState::WorktreeKept,
            _ => TicketGitState::Merged,
        };
    }
    match input.outcome {
        Some(GitOutcome::Conflict) => TicketGitState::Conflict,
        Some(GitOutcome::MainDirty) => TicketGitState::MainDirty,
        _ => TicketGitState::NotMerged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        accepted: bool,
        stale: bool,
        merged: bool,
        outcome: Option<GitOutcome>,
    ) -> GitStateInput {
        GitStateInput {
            accepted,
            stale,
            merged,
            outcome,
        }
    }

    #[test]
    fn not_accepted_is_in_review() {
        assert_eq!(
            derive_git_state(&input(false, false, false, None)),
            TicketGitState::InReview
        );
    }

    #[test]
    fn accepted_unmerged_is_not_merged() {
        assert_eq!(
            derive_git_state(&input(true, false, false, None)),
            TicketGitState::NotMerged
        );
    }

    #[test]
    fn stale_wins_over_outcome() {
        let i = input(true, true, false, Some(GitOutcome::Conflict));
        assert_eq!(derive_git_state(&i), TicketGitState::ChangedAfterReview);
    }

    #[test]
    fn unmerged_outcomes() {
        assert_eq!(
            derive_git_state(&input(true, false, false, Some(GitOutcome::Conflict))),
            TicketGitState::Conflict
        );
        assert_eq!(
            derive_git_state(&input(true, false, false, Some(GitOutcome::MainDirty))),
            TicketGitState::MainDirty
        );
    }

    #[test]
    fn merged_states() {
        assert_eq!(
            derive_git_state(&input(true, false, true, Some(GitOutcome::NotPushed))),
            TicketGitState::NotPushed
        );
        assert_eq!(
            derive_git_state(&input(true, false, true, Some(GitOutcome::WorktreeKept))),
            TicketGitState::WorktreeKept
        );
        assert_eq!(
            derive_git_state(&input(true, false, true, None)),
            TicketGitState::Merged
        );
    }

    #[test]
    fn needs_action_false_only_for_in_review_and_merged() {
        use TicketGitState::*;
        for s in [InReview, Merged] {
            assert!(!s.needs_action());
        }
        for s in [
            NotMerged,
            Conflict,
            MainDirty,
            ChangedAfterReview,
            NotPushed,
            WorktreeKept,
        ] {
            assert!(s.needs_action());
        }
    }

    #[test]
    fn strings_round_trip() {
        use TicketGitState::*;
        for s in [
            InReview,
            NotMerged,
            Conflict,
            MainDirty,
            ChangedAfterReview,
            NotPushed,
            WorktreeKept,
            Merged,
        ] {
            assert_eq!(TicketGitState::parse_str(s.as_str()), Some(s));
        }
        for o in [
            GitOutcome::Conflict,
            GitOutcome::MainDirty,
            GitOutcome::NotPushed,
            GitOutcome::WorktreeKept,
        ] {
            assert_eq!(GitOutcome::parse_str(o.as_str()), Some(o));
        }
    }
}
