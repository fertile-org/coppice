//! Copy for the review-capacity cap.
//!
//! Replace these strings word for word. The API sends them to the web app.

/// Number field on the board.
pub const REVIEW_CAP_LABEL: &str = "Review cap";

/// Shown under the review cap field.
pub const REVIEW_CAP_HINT: &str =
    "How many tickets can wait for your review at once. Leave it empty for no cap.";

/// Disclosure that holds the review cap field.
pub const BOARD_SETTINGS_LABEL: &str = "Board settings";

/// Name field on the board settings drawer.
pub const BOARD_NAME_LABEL: &str = "Name";

/// Rejected board name: blank, whitespace, or a slug that would be empty.
pub const BOARD_NAME_INVALID: &str = "Enter a board name that includes letters or numbers.";

/// Slug collision with another board (`foo-bar` matches `foobar`).
pub const BOARD_NAME_CONFLICT: &str = "This name is too similar to another board's name.";

/// Checkbox: In Review tickets count toward the cap.
pub const COUNT_IN_REVIEW_LABEL: &str = "Also count In Review";

/// Lead sentence when a ticket hits the review-round cap. `rounds` is that cap.
pub fn review_didnt_pass(rounds: i32) -> String {
    format!("Review didn't pass after {rounds} rounds. This may need a different approach.")
}

/// Label before the last round's open findings. Omitted when there are none.
pub const OPEN_FINDINGS_LABEL: &str = "Open findings:";

/// Round-cap comment. Findings are one markdown bullet each.
pub fn round_cap_comment(rounds: i32, findings: &[String]) -> String {
    let lead = review_didnt_pass(rounds);
    let findings: Vec<String> = findings
        .iter()
        .map(|finding| finding.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|finding| !finding.is_empty())
        .collect();
    if findings.is_empty() {
        return lead;
    }
    let mut body = format!("{lead} {OPEN_FINDINGS_LABEL}");
    for finding in findings {
        body.push('\n');
        body.push_str("- ");
        body.push_str(&finding);
    }
    body
}

/// First line of a later review round in the reviewer prompt.
pub const REVIEW_ROUND_INTRO: &str = "Review only the new diff. Earlier findings are below.";

/// Heading above the diff since the last review round.
pub const REVIEW_ROUND_NEW_DIFF: &str = "New diff";

/// Heading above findings from earlier rounds.
pub const REVIEW_ROUND_EARLIER_FINDINGS: &str = "Earlier findings";

/// Shown when the new range has no diff.
pub const REVIEW_ROUND_NO_DIFF: &str = "(no new diff)";

/// Shown when no earlier findings were stored.
pub const REVIEW_ROUND_NO_FINDINGS: &str = "(none)";

/// Rejected review cap. Empty means no cap. Zero is not a cap.
pub const REVIEW_CAP_INVALID: &str =
    "Enter a whole number of 1 or more, or leave it empty for no cap.";

/// Prefix of [`ready_held`]. A Ready ticket did not start.
pub const READY_HELD_PREFIX: &str = "This ticket waits in Ready until you have room to review it.";

/// Prefix of [`review_entry_held`]. The ticket did not enter Wait for Human Review.
pub const ENTRY_HELD_PREFIX: &str = "This ticket waits here until you have room to review it.";

fn tickets_waiting_on_you(count: i64) -> String {
    if count == 1 {
        "1 ticket is waiting on you.".to_string()
    } else {
        format!("{count} tickets are waiting on you.")
    }
}

/// Board chip. `count` occupies the cap. `cap` is the board limit.
pub fn waiting_on_review(count: i64, cap: i32) -> String {
    format!("{count} of {cap} waiting for review")
}

/// Comment and drag refusal while Ready cannot start.
pub fn ready_held(count: i64) -> String {
    format!("{READY_HELD_PREFIX} {}", tickets_waiting_on_you(count))
}

/// Comment and drag refusal while Wait for Human Review is full.
pub fn review_entry_held(count: i64) -> String {
    format!("{ENTRY_HELD_PREFIX} {}", tickets_waiting_on_you(count))
}

pub fn is_ready_hold(message: &str) -> bool {
    message.starts_with(READY_HELD_PREFIX)
}

pub fn is_entry_hold(message: &str) -> bool {
    message.starts_with(ENTRY_HELD_PREFIX)
}

/// Skip human review was on, but no reviewer or Tech Lead passed the ticket.
pub const SKIP_REVIEW_NO_AGENT_REVIEW: &str =
    "Skip human review didn't apply: no agent code review approved this ticket.";

/// Skip human review was on, but an agent asked for changes.
pub const SKIP_REVIEW_CHANGES_REQUESTED: &str =
    "Skip human review didn't apply: an agent asked for changes on this ticket.";

/// Skip human review was on, but the ticket was split.
pub const SKIP_REVIEW_SPLIT: &str = "Skip human review didn't apply: this ticket was split.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_helpers_distinguish_ready_and_entry_copy() {
        assert!(is_ready_hold(&ready_held(1)));
        assert!(is_entry_hold(&review_entry_held(1)));
        assert!(!is_ready_hold(&review_entry_held(1)));
        assert!(!is_entry_hold(&ready_held(1)));
        assert!(!is_ready_hold(&waiting_on_review(1, 1)));
        assert!(!is_entry_hold(&waiting_on_review(1, 1)));
        assert!(!round_cap_comment(3, &["  ".into()]).contains("Open findings"));
    }
}
