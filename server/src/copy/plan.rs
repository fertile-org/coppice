//! Copy for planning and Plan Review.
//!
//! Replace these strings word for word. The API sends them to the web app,
//! and planning runs post the plan comment from [`format_plan`].

/// Refused drag or status write into In Progress.
pub const PLAN_REQUIRED: &str =
    "Approve the plan, or turn on Skip planning, before moving this ticket to In Progress.";

/// Refused move into Ready without an approved plan or Skip planning.
pub const READY_REQUIRES_PLAN: &str =
    "Approve the plan before moving this ticket to Ready, or turn on Skip planning.";

/// Posted when no enabled PM can write the plan.
pub const NO_PM: &str =
    "No PM agent is available to write the plan. Add one in Agents, or turn on Skip planning.";

/// Posted when Ready has no Tech Lead to start. The ticket is not blocked.
pub const NO_TECH_LEAD: &str =
    "No Technical Lead agent is available, so this ticket stays in Ready without an assignee. Assign an agent to start work.";

/// Starting a plan from a column that is not Backlog or Ready.
pub const PLAN_START_WRONG_COLUMN: &str =
    "You can start a plan only from Backlog, or from Ready if the ticket has no plan yet.";

/// Posted when Ready has nobody to write the plan.
pub const NO_PLANNER: &str = "Assign an agent to this ticket to write the plan.";

/// Ask for changes while a planning run is still active.
pub const PLAN_ALREADY_RUNNING: &str =
    "The plan is still being written. Ask for changes once it's done.";

pub const APPROVE_ONLY_FROM_PLAN_REVIEW: &str =
    "You can approve a plan only while the ticket is in Plan Review.";

pub const NO_PLAN_YET: &str = "There's no plan to approve yet.";

pub const PLAN_STALE: &str =
    "The ticket changed after this plan was written. Ask for changes to get a new plan.";

pub const ASK_ONLY_FROM_PLAN_REVIEW: &str =
    "You can ask for plan changes only while the ticket is in Plan Review.";

pub const ASK_COMMENT_REQUIRED: &str = "Write what should change in the plan.";

/// System comment recorded when a human approves the plan.
pub const PLAN_APPROVED_NOTE: &str = "Plan approved.";

pub const IMPLEMENTATION_NOT_STARTED: &str =
    "The plan is approved, but work couldn't start. Check that the ticket has an assignee and its repository is ready.";

/// Posted when Skip planning cannot start work.
pub const WORK_NOT_STARTED: &str =
    "Work couldn't start. Check that the ticket has an assignee and its repository is ready.";

/// Posted when a plan run leaves files in its scratch worktree.
pub const PLAN_CHANGES_DISCARDED: &str =
    "The plan run changed some files. Coppice discarded them, and your ticket's code is unchanged.";

pub const PLAN_HEADING: &str = "## Plan";
pub const APPROACH_HEADING: &str = "### Approach";
pub const STEPS_HEADING: &str = "### Steps";
pub const ACCEPTANCE_CRITERIA_HEADING: &str = "### Acceptance criteria";
pub const RISKS_HEADING: &str = "### Risks";
pub const OUT_OF_SCOPE_HEADING: &str = "### Out of scope";
pub const NONE: &str = "None.";
pub const FOLLOW_APPROACH_STEP: &str = "Follow the approach above.";
pub const EMPTY_APPROACH: &str = "No approach was written.";

pub const PLAN_TASK: &str =
    "Write a plan for this ticket. Do not edit files, commit, push, or create or change a branch.";

pub const PLAN_FORMAT_INSTRUCTION: &str = "Put the plan in `summary` when you call `result_submit` with status `done`. Use exactly this markdown:";

/// Criteria are the contract Human Review checks the finished change against.
pub const PLAN_CRITERIA_INSTRUCTION: &str = "Under Acceptance criteria, write each line as behavior a person can check, then how it's verified: a test, a command, or a manual check. If the ticket description has `## Acceptance criteria`, start from those.";

/// Diagrams support the written plan. They are not a substitute for it.
pub const PLAN_DIAGRAM_INSTRUCTION: &str = "When the ticket needs a flow, architecture, or data model, add one or two small diagrams as fenced mermaid code blocks. Skip diagrams otherwise. The written plan always stays.";

pub const CHANGES_REQUESTED_HEADING: &str = "## Changes requested";

pub const PLAN_CHANGED_FILES_REJECTED: &str = "changedFiles is not allowed on a planning run.";
pub const PLAN_DESCRIPTION_REJECTED: &str = "updatedDescription is not allowed on a planning run.";
pub const PLAN_ASSIGN_REJECTED: &str = "assignTo is not allowed on a planning run.";
pub const PLAN_SPLIT_REJECTED: &str = "splitTickets is not allowed on a planning run.";
pub const PLAN_MUST_FINISH: &str =
    "A planning run finishes with status `done` and the plan in `summary`.";

fn checkbox_text(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("- [ ]")
        .or_else(|| trimmed.strip_prefix("- [x]"))
        .or_else(|| trimmed.strip_prefix("- [X]"))?;
    let text = rest.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The lines under `heading`, up to the next markdown heading.
fn section<'a>(markdown: &'a str, heading: &str) -> Option<Vec<&'a str>> {
    let mut lines = markdown.lines();
    lines.by_ref().find(|line| line.trim() == heading)?;
    Some(
        lines
            .take_while(|line| !line.trim_start().starts_with('#'))
            .collect(),
    )
}

/// Checkbox lines under Steps, in order, without the marker. A plan with no
/// Steps heading uses every checkbox line.
pub fn checklist_steps(markdown: &str) -> Vec<String> {
    match section(markdown, STEPS_HEADING) {
        Some(lines) => lines.into_iter().filter_map(checkbox_text).collect(),
        None => markdown.lines().filter_map(checkbox_text).collect(),
    }
}

/// Checkbox lines under Acceptance criteria. Empty for plans without them.
pub fn plan_criteria(markdown: &str) -> Vec<String> {
    section(markdown, ACCEPTANCE_CRITERIA_HEADING)
        .unwrap_or_default()
        .into_iter()
        .filter_map(checkbox_text)
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
        "{PLAN_HEADING}\n\n{APPROACH_HEADING}\n...\n\n{STEPS_HEADING}\n- [ ] ...\n\n{ACCEPTANCE_CRITERIA_HEADING}\n- [ ] ... (verified by ...)\n\n{RISKS_HEADING}\n{NONE}\n\n{OUT_OF_SCOPE_HEADING}\n{NONE}\n"
    )
}

pub const PREVIOUS_PLAN_HEADING: &str = "## Previous plan";

/// Heading a revised plan opens with, one line per change request.
pub const REVISED_CHANGES_HEADING: &str = "### Changes requested";

pub const REVISION_INSTRUCTION: &str = "Revise the previous plan. Start the revised plan with `### Changes requested`: one line per request above, in the same order, saying how you addressed it or why you didn't. Then write the full plan in the format above.";

/// A revision run: the plan it replaces and every change request on this
/// ticket's plans, oldest first. The last one is the newest.
pub struct PlanRevision<'a> {
    pub previous_plan: &'a str,
    pub requests: Vec<&'a str>,
}

/// Instructions appended to a planning run.
pub fn plan_instructions(revision: Option<&PlanRevision<'_>>) -> String {
    let mut out = format!(
        "{PLAN_TASK}\n\n{PLAN_FORMAT_INSTRUCTION}\n\n{}\n{PLAN_CRITERIA_INSTRUCTION}\n{PLAN_DIAGRAM_INSTRUCTION}\n",
        plan_template()
    );
    let Some(revision) = revision else {
        return out;
    };
    let requests: Vec<&str> = revision
        .requests
        .iter()
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
        .collect();
    if requests.is_empty() {
        return out;
    }
    let previous = revision.previous_plan.trim();
    if !previous.is_empty() {
        out.push_str(&format!("\n{PREVIOUS_PLAN_HEADING}\n\n{previous}\n"));
    }
    out.push_str(&format!("\n{CHANGES_REQUESTED_HEADING}\n\n"));
    for (index, request) in requests.iter().enumerate() {
        // Keep multi-line requests inside their numbered item.
        let body = request.lines().collect::<Vec<_>>().join("\n   ");
        out.push_str(&format!("{}. {body}\n", index + 1));
    }
    out.push_str(&format!("\n{REVISION_INSTRUCTION}\n"));
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
    fn criteria_are_their_own_section_and_not_steps() {
        let markdown = "## Plan\n\n### Approach\nDo it.\n\n### Steps\n- [ ] Write the gate\n- [ ] Cover the API\n\n### Acceptance criteria\n- [ ] Dragging to In Progress without a plan is refused (verified by integration test)\n- [ ] The API returns 409 (verified by curl)\n\n### Risks\nNone.\n";
        assert_eq!(checklist_steps(markdown), vec!["Write the gate", "Cover the API"]);
        assert_eq!(
            plan_criteria(markdown),
            vec![
                "Dragging to In Progress without a plan is refused (verified by integration test)",
                "The API returns 409 (verified by curl)"
            ]
        );
        assert_eq!(format_plan(markdown), markdown.trim());
    }

    #[test]
    fn a_plan_without_criteria_still_reads_its_steps() {
        let markdown = "## Plan\n\n### Steps\n- [ ] Only step\n\n### Risks\nNone.\n";
        assert_eq!(checklist_steps(markdown), vec!["Only step"]);
        assert!(plan_criteria(markdown).is_empty());
        assert!(plan_criteria(&format_plan("plain text")).is_empty());
    }

    #[test]
    fn the_template_asks_for_checkable_criteria() {
        let text = plan_instructions(None);
        assert!(text.contains(ACCEPTANCE_CRITERIA_HEADING));
        assert!(text.contains(PLAN_CRITERIA_INSTRUCTION));
        assert_eq!(checklist_steps(&plan_template()), vec!["..."]);
    }

    #[test]
    fn instructions_add_diagrams_only_when_a_plan_needs_one() {
        let text = plan_instructions(None);
        assert!(text.contains(
            "When the ticket needs a flow, architecture, or data model, add one or two small diagrams"
        ));
        assert!(text.contains("Skip diagrams otherwise."));
        assert!(text.contains("The written plan always stays."));
        assert!(text.contains(PLAN_HEADING));
        assert!(text.contains(STEPS_HEADING));
    }

    #[test]
    fn instructions_forbid_writes_and_include_changes() {
        let revision = PlanRevision {
            previous_plan: "## Plan\n\n### Steps\n- [ ] Add a migration\n",
            requests: vec!["Drop the migration."],
        };
        let text = plan_instructions(Some(&revision));
        assert!(text.contains(PLAN_TASK));
        assert!(text.contains("Do not edit files, commit, push, or create or change a branch."));
        assert!(text.contains(CHANGES_REQUESTED_HEADING));
        assert!(text.contains("1. Drop the migration."));
        assert!(text.contains(PLAN_FORMAT_INSTRUCTION));
    }

    #[test]
    fn a_first_plan_has_no_revision_section() {
        let text = plan_instructions(None);
        assert!(!text.contains(PREVIOUS_PLAN_HEADING));
        assert!(!text.contains(CHANGES_REQUESTED_HEADING));
        assert!(!text.contains(REVISION_INSTRUCTION));
    }

    #[test]
    fn a_third_round_lists_every_request_in_order_with_the_previous_plan() {
        let revision = PlanRevision {
            previous_plan: "## Plan\n\n### Steps\n- [ ] Second version\n",
            requests: vec!["Use Postgres.", "Split the API change\ninto two steps."],
        };
        let text = plan_instructions(Some(&revision));
        let previous = text.find(PREVIOUS_PLAN_HEADING).unwrap();
        let first = text.find("1. Use Postgres.").unwrap();
        let second = text.find("2. Split the API change\n   into two steps.").unwrap();
        assert!(previous < first && first < second, "{text}");
        assert!(text.contains("- [ ] Second version"));
        assert!(text.contains(REVISION_INSTRUCTION));
    }

    #[test]
    fn a_revised_plan_keeps_its_changes_section_out_of_the_steps() {
        let revised = "### Changes requested\n- Use Postgres: switched the store to Postgres.\n- [ ] Split the API change: now two steps.\n\n## Plan\n\n### Approach\nPostgres.\n\n### Steps\n- [ ] Add the table\n- [ ] Change the API\n";
        assert_eq!(format_plan(revised), revised.trim());
        assert_eq!(checklist_steps(revised), vec!["Add the table", "Change the API"]);
    }
}
