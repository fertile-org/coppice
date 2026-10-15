//! Copy for the review-capacity cap and the diff budget.
//!
//! Replace these strings word for word. The API sends them to the web app.

/// Number field on the board.
pub const REVIEW_CAP_LABEL: &str = "Review cap";

/// Checkbox: In Review tickets count toward the cap.
pub const COUNT_IN_REVIEW_LABEL: &str = "Also count In Review";

/// Number field for the line budget agreed with the plan.
pub const DIFF_BUDGET_LABEL: &str = "Diff budget";

/// Shown beside the diff budget field.
pub const DIFF_BUDGET_HINT: &str = "Maximum changed lines agreed with the plan.";

/// Rejected review cap.
pub const REVIEW_CAP_INVALID: &str =
    "Review cap must be zero or a positive whole number, or empty for no cap.";

/// Rejected diff budget.
pub const DIFF_BUDGET_INVALID: &str =
    "Diff budget must be zero or a positive whole number of lines, or empty for no budget.";

/// Prefix of [`ready_held`]. A Ready ticket did not start.
pub const READY_HELD_PREFIX: &str = "This ticket stays in Ready.";

/// Prefix of [`review_entry_held`]. The ticket did not enter Wait for Human Review.
pub const ENTRY_HELD_PREFIX: &str = "This ticket stays here until a review opens.";

/// Board line. `count` is how many tickets occupy review capacity.
pub fn waiting_on_review(count: i64) -> String {
    format!("{count} waiting on your review")
}

/// Comment and drag refusal while Ready cannot start.
pub fn ready_held(count: i64) -> String {
    format!("{READY_HELD_PREFIX} {count} waiting on your review.")
}

/// Comment and drag refusal while Wait for Human Review is full.
pub fn review_entry_held(count: i64) -> String {
    format!("{ENTRY_HELD_PREFIX} {count} waiting on your review.")
}

/// Comment when a run's diff is larger than the budget agreed with the plan.
pub fn diff_over_budget(actual: u64, budget: u64) -> String {
    format!(
        "This change is {actual} lines, past the {budget}-line budget agreed with the plan. The rest is left for a follow-up."
    )
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
    fn drafted_strings() {
        assert_eq!(REVIEW_CAP_LABEL, "Review cap");
        assert_eq!(COUNT_IN_REVIEW_LABEL, "Also count In Review");
        assert_eq!(DIFF_BUDGET_LABEL, "Diff budget");
        assert_eq!(
            DIFF_BUDGET_HINT,
            "Maximum changed lines agreed with the plan."
        );
        assert_eq!(
            REVIEW_CAP_INVALID,
            "Review cap must be zero or a positive whole number, or empty for no cap."
        );
        assert_eq!(
            DIFF_BUDGET_INVALID,
            "Diff budget must be zero or a positive whole number of lines, or empty for no budget."
        );
        assert_eq!(waiting_on_review(3), "3 waiting on your review");
        assert_eq!(
            ready_held(3),
            "This ticket stays in Ready. 3 waiting on your review."
        );
        assert_eq!(
            review_entry_held(3),
            "This ticket stays here until a review opens. 3 waiting on your review."
        );
        assert_eq!(
            diff_over_budget(40, 12),
            "This change is 40 lines, past the 12-line budget agreed with the plan. The rest is left for a follow-up."
        );
        assert!(is_ready_hold(&ready_held(1)));
        assert!(is_entry_hold(&review_entry_held(1)));
        assert!(!is_ready_hold(&waiting_on_review(1)));
        assert!(!is_entry_hold(&waiting_on_review(1)));
    }
}
