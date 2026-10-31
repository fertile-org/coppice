//! Copy for planning and Plan Review.
//!
//! Replace these strings word for word. The API sends them to the web app,
//! and planning runs store the plan from [`format_plan`] on `ticket_plans`.

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

/// Choosing a design after the plan was approved.
pub const PLAN_DESIGN_FINAL: &str =
    "The plan is approved, so this design is final. To change it, add a comment on the ticket.";

/// Choosing from a set that newer options replaced.
pub const DESIGN_SUPERSEDED: &str =
    "Newer design options replaced these. Open the latest ones to choose.";

/// Change request recorded when a different design is chosen in Plan Review.
pub fn plan_design_change_note(title: &str) -> String {
    format!("Update the plan to use design option {title}.")
}

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

/// Added when the agent has the design skill. Guidance only: Plan Review does
/// not wait for a choice, and a person can ask for plan changes after picking.
pub const PLAN_DESIGN_INSTRUCTION: &str = "If this ticket changes what people see, load `design:design-ui-options` and publish 2–3 design options before you submit the plan, marking the one you recommend, then write the plan around it and say which one it is. If `design__list` shows a selected option, write the plan around that one and don't publish new options unless a change request asks for them. Skip this when the ticket has no visible change.";

/// Added to work runs when the agent has the design skill.
pub const WORK_DESIGN_INSTRUCTION: &str = "If `ticket_get` returns `selectedDesign`, build that option. Read the HTML at its `path` for the layout when you can; otherwise follow its title and the plan.";

/// A person reviews the plan at Plan Review, so it has to fit on one screen.
/// Detail belongs to the Tech Lead and the implementer, not the plan.
pub const PLAN_LENGTH_INSTRUCTION: &str = "Keep the whole plan to about one screen for a human to review. Approach: at most 3 sentences. Steps: at most 8, each one line. Acceptance criteria: at most 6, each one line. Risks and Out of scope: one line each. Name a file, function, or test only where it decides the work; when you name a file, use its full repo-relative path. Leave implementation detail to the implementer.";

/// Above these, `result_submit` warns a planning run. The plan still saves.
pub const PLAN_MAX_STEPS: usize = 8;
pub const PLAN_MAX_CRITERIA: usize = 8;
pub const PLAN_MAX_WORDS: usize = 400;

pub fn plan_too_many_steps(count: usize) -> String {
    format!("The plan has {count} steps; keep it to {PLAN_MAX_STEPS} or fewer. Merge small steps and leave implementation detail to the implementer, or propose sub-tickets in `splitTickets`. Resubmit to replace this plan.")
}

pub fn plan_too_many_criteria(count: usize) -> String {
    format!("The plan has {count} acceptance criteria; keep it to {PLAN_MAX_CRITERIA} or fewer. Keep the ones a person checks at Human Review, or propose sub-tickets in `splitTickets`. Resubmit to replace this plan.")
}

pub fn plan_too_long(words: usize) -> String {
    format!("The plan is about {words} words; keep it under {PLAN_MAX_WORDS} so it fits on one screen. Drop signatures, line numbers, SQL, and code, and leave that detail to the implementer. Resubmit to replace this plan.")
}

pub const CHANGES_REQUESTED_HEADING: &str = "## Changes requested";

pub const PLAN_CHANGED_FILES_REJECTED: &str = "changedFiles is not allowed on a planning run.";
pub const PLAN_DESCRIPTION_REJECTED: &str = "updatedDescription is not allowed on a planning run.";
pub const PLAN_ASSIGN_REJECTED: &str = "assignTo is not allowed on a planning run.";
pub const PLAN_SPLIT_REJECTED: &str = "splitTickets is not allowed on a planning run.";
pub const PLAN_SPLIT_TOO_SMALL: &str =
    "A split needs at least 2 sub-tickets. Plan one ticket without splitTickets instead.";
pub const PLAN_MUST_FINISH: &str =
    "A planning run finishes with status `done` and the plan in `summary`.";

pub const PLAN_SPLIT_INSTRUCTION: &str = "If the work is too big for one ticket, you may also propose 2 or more sub-tickets in `splitTickets`, each with a title, a description, acceptance criteria, and short one-line `steps`. They are created only when the plan is approved, each with its own approved plan. Then write this plan as the umbrella plan for the whole ticket.";

/// System note when approving a plan creates its sub-tickets.
pub fn split_approved_note(count: usize) -> String {
    format!("Plan approved. Coppice created {count} sub-tickets in Backlog, each with its approved plan. Work happens on the sub-tickets.")
}

/// Approach line of a sub-ticket's plan.
pub fn child_plan_approach(parent_title: &str, description: &str) -> String {
    let description = description.trim();
    if description.is_empty() {
        format!("Part of \"{}\".", parent_title.trim())
    } else {
        format!("Part of \"{}\". {description}", parent_title.trim())
    }
}

/// The approved plan attached to a sub-ticket created at Plan Review.
pub fn child_plan(
    parent_title: &str,
    description: &str,
    steps: &[String],
    acceptance_criteria: Option<&str>,
) -> String {
    let mut out = format!(
        "{PLAN_HEADING}\n\n{APPROACH_HEADING}\n{}\n\n{STEPS_HEADING}\n",
        child_plan_approach(parent_title, description)
    );
    let steps: Vec<&str> = steps.iter().map(|step| step.trim()).filter(|s| !s.is_empty()).collect();
    if steps.is_empty() {
        out.push_str(&format!("- [ ] {FOLLOW_APPROACH_STEP}\n"));
    }
    for step in steps {
        out.push_str(&format!("- [ ] {step}\n"));
    }
    let criteria: Vec<String> = acceptance_criteria
        .unwrap_or("")
        .lines()
        .map(|line| {
            let line = line.trim();
            let line = line
                .strip_prefix("- [ ]")
                .or_else(|| line.strip_prefix("- [x]"))
                .or_else(|| line.strip_prefix("- "))
                .or_else(|| line.strip_prefix("* "))
                .unwrap_or(line);
            line.trim().to_string()
        })
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if !criteria.is_empty() {
        out.push_str(&format!("\n{ACCEPTANCE_CRITERIA_HEADING}\n"));
        for criterion in criteria {
            out.push_str(&format!("- [ ] {criterion}\n"));
        }
    }
    out.push_str(&format!("\n{RISKS_HEADING}\n{NONE}\n\n{OUT_OF_SCOPE_HEADING}\n{NONE}\n"));
    out
}

/// Refused work start on a split parent.
pub const UMBRELLA_NO_WORK: &str = "This ticket was split into sub-tickets, so work happens on them. Verify this ticket once they're all Done and merged.";

pub const VERIFY_NOT_READY: &str =
    "You can verify this ticket once every sub-ticket is Done and merged.";

pub const VERIFY_NOT_A_PARENT: &str = "Only a ticket split at Plan Review can be verified.";

pub const VERIFY_ALREADY_RUNNING: &str = "Verification is already running.";

pub const VERIFY_TASK: &str = "Verify this parent ticket. Its work was split into the sub-tickets below, and each one is Done and merged. Check the parent's acceptance criteria against the merged code on the default branch. Do not edit files, commit, push, or create or change a branch.";

pub const VERIFY_FORMAT_INSTRUCTION: &str = "Call `result_submit` with status `done`, a short `summary` of what you checked, and `criteriaResults`: one entry per acceptance criterion with `met`, `not_met`, or `not_checked`, and one line of evidence naming the sub-ticket, commit, test, or file.";

pub const VERIFICATION_COMMENT_TITLE: &str = "**Verification**";

pub const VERIFY_EXCLUDED_HEADING: &str = "Excluded (archived):";

/// Instructions for a verification run on a split parent.
pub fn verify_instructions(criteria: &[String], merged: &[(String, String)], excluded: &[String]) -> String {
    let mut out = format!("{VERIFY_TASK}\n\n{ACCEPTANCE_CRITERIA_HEADING}\n");
    if criteria.is_empty() {
        out.push_str("The parent has no acceptance criteria. Check its description instead.\n");
    }
    for criterion in criteria {
        out.push_str(&format!("- [ ] {criterion}\n"));
    }
    out.push_str("\n### Sub-tickets (merged)\n");
    for (title, sha) in merged {
        let short = sha.get(..7).unwrap_or(sha);
        out.push_str(&format!("- {title} (`{short}`)\n"));
    }
    if !excluded.is_empty() {
        out.push_str(&format!("\n### {VERIFY_EXCLUDED_HEADING}\n"));
        for title in excluded {
            out.push_str(&format!("- {title}\n"));
        }
    }
    out.push_str(&format!("\n{VERIFY_FORMAT_INSTRUCTION}\n"));
    out
}

/// The comment a verification run posts on the parent.
pub fn verification_comment(
    summary: &str,
    results: &[crate::providers::CriterionResult],
    excluded: &[String],
) -> String {
    use crate::providers::CriterionStatus;
    let mut out = format!("{VERIFICATION_COMMENT_TITLE}\n\n{}\n", summary.trim());
    if !results.is_empty() {
        out.push('\n');
        for result in results {
            let mark = match result.status {
                CriterionStatus::Met => "Met",
                CriterionStatus::NotMet => "Not met",
                CriterionStatus::NotChecked => "Not checked",
            };
            out.push_str(&format!("- **{mark}:** {}", result.criterion.trim()));
            if let Some(evidence) = result.evidence.as_deref() {
                out.push_str(&format!(" ({})", evidence.trim()));
            }
            out.push('\n');
        }
    }
    if !excluded.is_empty() {
        out.push_str(&format!("\n{VERIFY_EXCLUDED_HEADING} {}\n", excluded.join(", ")));
    }
    out
}

/// Most questions a planning run may ask.
pub const MAX_PLAN_QUESTIONS: usize = 3;

pub const PLAN_QUESTIONS_INSTRUCTION: &str = "If something in the ticket is unclear and the answer would change the plan, you may ask instead of planning: call `result_submit` with status `done`, an empty `summary`, and 1 to 3 short `planQuestions`, each with optional `choices`. You get one round of questions. If nothing is unclear, write the plan.";

pub const PLAN_QUESTIONS_TOO_MANY: &str =
    "Ask at most 3 questions. If more is unclear, write the plan and list it under Risks.";

pub const PLAN_QUESTIONS_ALREADY_ASKED: &str = "You already asked your questions for this ticket. Write the plan now and list anything still unclear under Risks.";

pub const ANSWERS_HEADING: &str = "## Your questions and the answers";

pub const ANSWERS_INSTRUCTION: &str =
    "Write the plan now. Don't ask more questions; list anything still unclear under Risks.";

/// Ticket comment that holds the questions.
pub const QUESTIONS_COMMENT_TITLE: &str = "**Questions before the plan**";

/// Ticket comment that holds the human's answers.
pub const ANSWERS_COMMENT_TITLE: &str = "**Answers**";

/// Shown when an answer is not given.
pub const NO_ANSWER: &str = "No answer.";

pub const SEND_ANSWERS_REQUIRED: &str = "Answer at least one question.";

pub const NO_QUESTIONS_WAITING: &str = "There are no questions waiting for answers.";

pub const APPROVE_WAITING_FOR_ANSWERS: &str =
    "Answer the questions first. There's no plan to approve yet.";

/// Markdown for the questions comment.
pub fn questions_comment(questions: &[crate::providers::PlanQuestion]) -> String {
    let mut out = format!("{QUESTIONS_COMMENT_TITLE}\n");
    for (index, question) in questions.iter().enumerate() {
        out.push_str(&format!("\n{}. {}", index + 1, question.question.trim()));
        for choice in &question.choices {
            out.push_str(&format!("\n   - {}", choice.trim()));
        }
    }
    out
}

/// Markdown for the answers comment and the next planning prompt.
pub fn answered_questions(
    questions: &[crate::providers::PlanQuestion],
    answers: &[String],
) -> String {
    let mut out = String::new();
    for (index, question) in questions.iter().enumerate() {
        let answer = answers
            .get(index)
            .map(|answer| answer.trim())
            .filter(|answer| !answer.is_empty())
            .unwrap_or(NO_ANSWER);
        out.push_str(&format!(
            "{}. {}\n   {}\n",
            index + 1,
            question.question.trim(),
            answer.lines().collect::<Vec<_>>().join("\n   ")
        ));
    }
    out
}

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

const TEMPLATE_APPROACH: &str = "One to three sentences: what changes and why.";
const TEMPLATE_STEP: &str = "One line: an outcome, not code.";
const TEMPLATE_CRITERION: &str =
    "One line a person can check (verified by a test, command, or manual check)";

pub fn plan_template() -> String {
    format!(
        "{PLAN_HEADING}\n\n{APPROACH_HEADING}\n{TEMPLATE_APPROACH}\n\n{STEPS_HEADING}\n- [ ] {TEMPLATE_STEP}\n\n{ACCEPTANCE_CRITERIA_HEADING}\n- [ ] {TEMPLATE_CRITERION}\n\n{RISKS_HEADING}\n{NONE}\n\n{OUT_OF_SCOPE_HEADING}\n{NONE}\n"
    )
}

pub const PREVIOUS_PLAN_HEADING: &str = "## Previous plan";

/// Description section an approved plan is written into. The text above it is
/// the original description; the Tech Lead may revise only this section.
pub const APPROVED_PLAN_SECTION: &str = "## Approved plan";

/// The plan as it goes under [`APPROVED_PLAN_SECTION`]: the text after
/// `## Plan`, so a revision's `### Changes requested` preamble is left out.
pub fn approved_plan_section_body(plan_comment: &str) -> String {
    let body = plan_comment.trim();
    match body.find(PLAN_HEADING) {
        Some(start) => body[start + PLAN_HEADING.len()..].trim().to_string(),
        None => body.to_string(),
    }
}

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
        "{PLAN_TASK}\n\n{PLAN_FORMAT_INSTRUCTION}\n\n{}\n{PLAN_CRITERIA_INSTRUCTION}\n{PLAN_DIAGRAM_INSTRUCTION}\n{PLAN_LENGTH_INSTRUCTION}\n",
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

/// Advisory length checks on a submitted plan. Diagrams (fenced code blocks)
/// and a revision's `### Changes requested` preamble don't count.
pub fn plan_length_warnings(summary: &str) -> Vec<String> {
    let plan = approved_plan_section_body(summary);
    let mut warnings = Vec::new();
    let steps = checklist_steps(&plan).len();
    if steps > PLAN_MAX_STEPS {
        warnings.push(plan_too_many_steps(steps));
    }
    let criteria = plan_criteria(&plan).len();
    if criteria > PLAN_MAX_CRITERIA {
        warnings.push(plan_too_many_criteria(criteria));
    }
    let words = prose_word_count(&plan);
    if words > PLAN_MAX_WORDS {
        warnings.push(plan_too_long(words));
    }
    warnings
}

/// Words outside fenced code blocks, headings excluded.
fn prose_word_count(markdown: &str) -> usize {
    let mut in_fence = false;
    let mut words = 0;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.starts_with('#') {
            continue;
        }
        words += line
            .split_whitespace()
            .filter(|word| !matches!(*word, "-" | "*" | "[" | "]" | "[x]" | "[X]"))
            .count();
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_plan_section_drops_the_heading_and_a_revision_preamble() {
        let plan = "### Changes requested\n1. Done.\n\n## Plan\n\n### Approach\nShip it.";
        assert_eq!(approved_plan_section_body(plan), "### Approach\nShip it.");
        assert_eq!(approved_plan_section_body("  Just text. "), "Just text.");
    }

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
        assert_eq!(checklist_steps(&plan_template()), vec![TEMPLATE_STEP]);
        assert_eq!(plan_criteria(&plan_template()), vec![TEMPLATE_CRITERION]);
    }

    #[test]
    fn questions_and_answers_read_in_order() {
        use crate::providers::PlanQuestion;
        let questions = vec![
            PlanQuestion {
                question: "Which database?".into(),
                choices: vec!["Postgres".into(), "SQLite".into()],
            },
            PlanQuestion {
                question: "Keep the endpoint?".into(),
                choices: Vec::new(),
            },
        ];
        let asked = questions_comment(&questions);
        assert!(asked.starts_with(QUESTIONS_COMMENT_TITLE));
        assert!(asked.contains("1. Which database?\n   - Postgres\n   - SQLite"));
        assert!(asked.contains("2. Keep the endpoint?"));
        let answered = answered_questions(&questions, &["Postgres".into()]);
        assert_eq!(
            answered,
            "1. Which database?\n   Postgres\n2. Keep the endpoint?\n   No answer.\n"
        );
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
    fn instructions_ask_for_a_plan_one_screen_long() {
        let text = plan_instructions(None);
        assert!(text.contains(PLAN_LENGTH_INSTRUCTION));
        assert!(text.contains("at most 8"));
        let revision = PlanRevision {
            previous_plan: "## Plan\n\n### Steps\n- [ ] Old\n",
            requests: vec!["Shorter please."],
        };
        assert!(plan_instructions(Some(&revision)).contains(PLAN_LENGTH_INSTRUCTION));
        assert!(text.contains("full repo-relative path"));
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
    fn plan_with(steps: usize, criteria: usize, approach: &str) -> String {
        let mut plan =
            format!("{PLAN_HEADING}\n\n{APPROACH_HEADING}\n{approach}\n\n{STEPS_HEADING}\n");
        for i in 0..steps {
            plan.push_str(&format!("- [ ] Step {i}\n"));
        }
        plan.push_str(&format!("\n{ACCEPTANCE_CRITERIA_HEADING}\n"));
        for i in 0..criteria {
            plan.push_str(&format!("- [ ] Criterion {i} (verified by test)\n"));
        }
        plan.push_str(&format!(
            "\n{RISKS_HEADING}\n{NONE}\n\n{OUT_OF_SCOPE_HEADING}\n{NONE}\n"
        ));
        plan
    }

    #[test]
    fn a_short_plan_has_no_length_warning() {
        assert!(plan_length_warnings(&plan_with(5, 4, "Add a check.")).is_empty());
    }

    #[test]
    fn a_long_plan_warns_on_steps_criteria_and_words() {
        let plan = plan_with(9, 9, &"word ".repeat(PLAN_MAX_WORDS));
        assert_eq!(
            plan_length_warnings(&plan),
            vec![
                plan_too_many_steps(9),
                plan_too_many_criteria(9),
                plan_too_long(prose_word_count(&plan)),
            ]
        );
    }

    #[test]
    fn diagrams_do_not_count_toward_the_length() {
        let diagram = format!(
            "Short.\n\n```mermaid\n{}```",
            "A --> B\n".repeat(PLAN_MAX_WORDS)
        );
        assert!(plan_length_warnings(&plan_with(3, 3, &diagram)).is_empty());
    }

    #[test]
    fn a_revision_preamble_does_not_count_toward_the_length() {
        let preamble = format!(
            "{REVISED_CHANGES_HEADING}\n{}\n\n",
            "Done as asked. ".repeat(PLAN_MAX_WORDS)
        );
        let revised = format!("{preamble}{}", plan_with(3, 3, "Add a check."));
        assert!(plan_length_warnings(&revised).is_empty());
    }
}
