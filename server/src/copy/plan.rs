//! Copy for planning and Plan Review.
//!
//! Replace these strings word for word. The API sends them to the web app,
//! and planning runs post the plan comment from [`format_plan`].

/// Refused drag or status write into In Progress.
pub const PLAN_REQUIRED: &str = "Approve the plan before moving this ticket to In Progress.";

/// Posted when Ready has nobody to write the plan.
pub const NO_PLANNER: &str = "This ticket has no assignee to write the plan.";

/// Ask for changes while a planning run is still active.
pub const PLAN_ALREADY_RUNNING: &str = "A plan is already being written for this ticket.";

pub const APPROVE_ONLY_FROM_PLAN_REVIEW: &str = "Approve the plan from Plan Review.";

pub const NO_PLAN_YET: &str = "There is no plan to approve yet.";

pub const PLAN_STALE: &str = "The ticket changed after this plan was written. Ask for a new plan.";

pub const ASK_ONLY_FROM_PLAN_REVIEW: &str = "Ask for plan changes from Plan Review.";

pub const ASK_COMMENT_REQUIRED: &str = "Write what should change in the plan.";

/// System comment recorded when a human approves the plan.
pub const PLAN_APPROVED_NOTE: &str = "Plan approved.";

pub const IMPLEMENTATION_NOT_STARTED: &str =
    "The plan is approved. Assign an agent with a ready repository to start the work.";

/// Posted when Skip planning cannot start work.
pub const WORK_NOT_STARTED: &str = "Assign an agent with a ready repository to start the work.";

/// Posted when a plan run leaves files in its scratch worktree.
pub const PLAN_CHANGES_DISCARDED: &str =
    "The plan run changed files. Those changes were discarded.";

pub const PLAN_HEADING: &str = "## Plan";
pub const APPROACH_HEADING: &str = "### Approach";
pub const STEPS_HEADING: &str = "### Steps";
pub const RISKS_HEADING: &str = "### Risks";
pub const OUT_OF_SCOPE_HEADING: &str = "### Out of scope";
pub const NONE: &str = "None.";
pub const FOLLOW_APPROACH_STEP: &str = "Follow the approach above.";
pub const EMPTY_APPROACH: &str = "No approach was written.";

pub const PLAN_TASK: &str = "Write a plan for this ticket. Do not edit files, do not commit, and do not create or change a branch.";

pub const PLAN_FORMAT_INSTRUCTION: &str = "Put the plan in `summary` when you call `result_submit` with status `done`. Use exactly this markdown:";

pub const CHANGES_REQUESTED_HEADING: &str = "## Changes requested";

pub const PLAN_CHANGED_FILES_REJECTED: &str = "changedFiles is not allowed on a planning run.";
pub const PLAN_DESCRIPTION_REJECTED: &str = "updatedDescription is not allowed on a planning run.";
pub const PLAN_ASSIGN_REJECTED: &str = "assignTo is not allowed on a planning run.";
pub const PLAN_SPLIT_REJECTED: &str = "splitTickets is not allowed on a planning run.";
pub const PLAN_MUST_FINISH: &str =
    "A planning run finishes with status `done` and the plan in `summary`.";

/// Checkbox lines under the plan, in order, without the marker.
pub fn checklist_steps(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed
                .strip_prefix("- [ ]")
                .or_else(|| trimmed.strip_prefix("- [x]"))
                .or_else(|| trimmed.strip_prefix("- [X]"))?;
            let text = rest.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        })
        .collect()
}

fn is_canonical_plan(text: &str) -> bool {
    text.contains(PLAN_HEADING)
        && text.contains(STEPS_HEADING)
        && checklist_steps(text).iter().any(|step| !step.is_empty())
}

/// One markdown shape for every plan comment, native plan mode or not.
pub fn format_plan(summary: &str) -> String {
    let summary = summary.trim();
    if is_canonical_plan(summary) {
        return summary.to_string();
    }
    let approach = if summary.is_empty() {
        EMPTY_APPROACH
    } else {
        summary
    };
    format!(
        "{PLAN_HEADING}\n\n{APPROACH_HEADING}\n{approach}\n\n{STEPS_HEADING}\n- [ ] {FOLLOW_APPROACH_STEP}\n\n{RISKS_HEADING}\n{NONE}\n\n{OUT_OF_SCOPE_HEADING}\n{NONE}"
    )
}

pub fn plan_template() -> String {
    format!(
        "{PLAN_HEADING}\n\n{APPROACH_HEADING}\n...\n\n{STEPS_HEADING}\n- [ ] ...\n\n{RISKS_HEADING}\n{NONE}\n\n{OUT_OF_SCOPE_HEADING}\n{NONE}\n"
    )
}

/// Instructions appended to a planning run. `changes` is the human comment.
pub fn plan_instructions(changes: Option<&str>) -> String {
    let mut out = format!(
        "{PLAN_TASK}\n\n{PLAN_FORMAT_INSTRUCTION}\n\n{}\n",
        plan_template()
    );
    if let Some(changes) = changes.map(str::trim).filter(|text| !text.is_empty()) {
        out.push_str(&format!("\n{CHANGES_REQUESTED_HEADING}\n\n{changes}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_a_plain_summary_in_the_plan_format() {
        let plan = format_plan("Ship the parser first.");
        assert!(plan.starts_with(PLAN_HEADING));
        assert!(plan.contains("Ship the parser first."));
        assert_eq!(checklist_steps(&plan), vec![FOLLOW_APPROACH_STEP]);
        assert!(plan.contains(RISKS_HEADING));
        assert!(plan.contains(OUT_OF_SCOPE_HEADING));
    }

    #[test]
    fn keeps_a_canonical_plan() {
        let raw = format_plan("ignored");
        let again = format_plan(&raw);
        assert_eq!(again, raw);
    }

    #[test]
    fn checklist_reads_open_and_done_boxes() {
        let markdown = "## Plan\n\n### Steps\n- [ ] Write the gate\n- [x] Cover the drag\n- [X] Cover the API\n";
        assert_eq!(
            checklist_steps(markdown),
            vec!["Write the gate", "Cover the drag", "Cover the API"]
        );
    }

    #[test]
    fn instructions_forbid_writes_and_include_changes() {
        let text = plan_instructions(Some("Drop the migration."));
        assert!(text.contains(PLAN_TASK));
        assert!(text.contains("Do not edit files"));
        assert!(text.contains(CHANGES_REQUESTED_HEADING));
        assert!(text.contains("Drop the migration."));
        assert!(text.contains(PLAN_FORMAT_INSTRUCTION));
    }
}
