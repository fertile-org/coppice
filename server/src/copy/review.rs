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

/// Checkbox: In Review tickets count toward the cap.
pub const COUNT_IN_REVIEW_LABEL: &str = "Also count In Review";

/// The one comment posted when a ticket hits the review-round cap.
pub const RECONSIDER_APPROACH: &str = "Reconsider the approach.";

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
pub const READY_HELD_PREFIX: &str = "This ticket stays in Ready until you have room to review it.";

/// Prefix of [`review_entry_held`]. The ticket did not enter Wait for Human Review.
pub const ENTRY_HELD_PREFIX: &str = "This ticket stays here until you have room to review it.";

/// Board chip. `count` occupies the cap. `cap` is the board limit.
pub fn waiting_on_review(count: i64, cap: i32) -> String {
    format!("{count} of {cap} waiting for review")
}

/// Comment and drag refusal while Ready cannot start.
pub fn ready_held(count: i64) -> String {
    format!("{READY_HELD_PREFIX} {count} waiting on your review.")
}

/// Comment and drag refusal while Wait for Human Review is full.
pub fn review_entry_held(count: i64) -> String {
    format!("{ENTRY_HELD_PREFIX} {count} waiting on your review.")
}

pub fn is_ready_hold(message: &str) -> bool {
    message.starts_with(READY_HELD_PREFIX)
}

pub fn is_entry_hold(message: &str) -> bool {
    message.starts_with(ENTRY_HELD_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_strings() {
        assert_eq!(REVIEW_CAP_LABEL, "Review cap");
        assert_eq!(
            REVIEW_CAP_HINT,
            "How many tickets can wait for your review at once. Leave it empty for no cap."
        );
        assert_eq!(BOARD_SETTINGS_LABEL, "Board settings");
        assert_eq!(COUNT_IN_REVIEW_LABEL, "Also count In Review");
        assert_eq!(RECONSIDER_APPROACH, "Reconsider the approach.");
        assert_eq!(
            REVIEW_ROUND_INTRO,
            "Review only the new diff. Earlier findings are below."
        );
        assert_eq!(REVIEW_ROUND_NEW_DIFF, "New diff");
        assert_eq!(REVIEW_ROUND_EARLIER_FINDINGS, "Earlier findings");
        assert_eq!(REVIEW_ROUND_NO_DIFF, "(no new diff)");
        assert_eq!(REVIEW_ROUND_NO_FINDINGS, "(none)");
        assert_eq!(
            REVIEW_CAP_INVALID,
            "Enter a whole number of 1 or more, or leave it empty for no cap."
        );
        assert_eq!(waiting_on_review(3, 5), "3 of 5 waiting for review");
        assert_eq!(
            ready_held(3),
            "This ticket stays in Ready until you have room to review it. 3 waiting on your review."
        );
        assert_eq!(
            review_entry_held(3),
            "This ticket stays here until you have room to review it. 3 waiting on your review."
        );
        assert!(is_ready_hold(&ready_held(1)));
        assert!(is_entry_hold(&review_entry_held(1)));
        assert!(!is_ready_hold(&review_entry_held(1)));
        assert!(!is_entry_hold(&ready_held(1)));
        assert!(!is_ready_hold(&waiting_on_review(1, 1)));
        assert!(!is_entry_hold(&waiting_on_review(1, 1)));
    }
}
