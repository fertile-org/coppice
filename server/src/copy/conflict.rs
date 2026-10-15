//! Copy for a merge or rebase that stopped on a conflict.
//!
//! Replace these strings word for word. The API sends them to the web app.

/// "rebase onto" in [`conflict_message`].
pub const REBASE_ACTION: &str = "rebase onto";
/// "merge into" in [`conflict_message`].
pub const MERGE_ACTION: &str = "merge into";

pub const NO_ASSIGNEE: &str = "Assign an agent to this ticket to resolve the conflicts.";

pub const REREVIEW_NOTE: &str =
    "When the conflicts are resolved, the ticket goes back to In Review. You'll need to accept it again before it merges.";

pub const INVALID_FILE_LIST: &str = "The conflict file list is invalid.";

/// [placeholder] Shown when a person tries to move a Done ticket to another column.
pub const DONE_COLUMN_HOLDS: &str =
    "[placeholder] Done tickets stay in Done. Dragging one to another column does not move it.";

/// [placeholder] Shown when a merge conflict was cleared but the ticket could not leave Done.
pub const MERGE_CONFLICT_REOPEN_FAILED: &str =
    "[placeholder] The merge conflict was cleared, but the ticket could not return to In Progress.";

/// [placeholder] Comment posted when a Done ticket's merge hits a conflict.
/// Also copied into the assignee's run context.
pub fn merge_conflict_reopen_comment(branch: &str, files: &[String]) -> String {
    let listed = if files.is_empty() {
        "- (none listed)".to_string()
    } else {
        files
            .iter()
            .map(|file| format!("- {file}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "[placeholder] Couldn't merge into {branch} because these files conflict:\n{listed}\n\n\
         The ticket is back in In Progress with the same assignee. It has to go through Human Review and be accepted again before it can be Done."
    )
}

pub fn connector_not_ready_reason(assignee: &str) -> String {
    format!("{assignee}'s connector isn't ready. Check it in Tools → Connectors.")
}

pub fn ask_to_resolve_label(assignee: &str) -> String {
    format!("Ask {assignee} to resolve")
}

/// Conflict message, including the conflicting files.
pub fn conflict_message(action: &str, branch: &str, files: &[String]) -> String {
    let listed = if files.is_empty() {
        "(none listed)".to_string()
    } else {
        files.join(", ")
    };
    format!(
        "Couldn't {action} {branch} because these files conflict: {listed}. Nothing was changed."
    )
}

/// Instruction for the assignee's run. Includes [`REREVIEW_NOTE`].
pub fn resolve_prompt(branch: &str, files: &[String]) -> String {
    let listed = if files.is_empty() {
        "(none listed)".to_string()
    } else {
        files
            .iter()
            .map(|file| format!("- {file}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Resolve the conflict with `{branch}`.\n\n\
         Rebase or merge onto `{branch}`.\n\
         These files conflict:\n\
         {listed}\n\n\
         Keep this ticket's intent and get the tests passing.\n\n\
         {REREVIEW_NOTE}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafted_strings() {
        assert_eq!(
            conflict_message(REBASE_ACTION, "main", &["README.md".into()]),
            "Couldn't rebase onto main because these files conflict: README.md. Nothing was changed."
        );
        assert_eq!(
            conflict_message(MERGE_ACTION, "main", &["README.md".into()]),
            "Couldn't merge into main because these files conflict: README.md. Nothing was changed."
        );
        assert_eq!(
            conflict_message(
                MERGE_ACTION,
                "main",
                &["README.md".into(), "src/lib.rs".into()]
            ),
            "Couldn't merge into main because these files conflict: README.md, src/lib.rs. Nothing was changed."
        );
        assert_eq!(ask_to_resolve_label("Ada"), "Ask Ada to resolve");
        assert_eq!(
            NO_ASSIGNEE,
            "Assign an agent to this ticket to resolve the conflicts."
        );
        assert_eq!(
            connector_not_ready_reason("Ada"),
            "Ada's connector isn't ready. Check it in Tools → Connectors."
        );
        assert_eq!(
            REREVIEW_NOTE,
            "When the conflicts are resolved, the ticket goes back to In Review. You'll need to accept it again before it merges."
        );
    }

    #[test]
    fn resolve_prompt_names_the_branch_files_and_rereview() {
        let prompt = resolve_prompt("main", &["README.md".into()]);
        assert!(prompt.contains("Rebase or merge onto `main`."));
        assert!(prompt.contains("- README.md"));
        assert!(prompt.contains("Keep this ticket's intent and get the tests passing."));
        assert!(prompt.contains(REREVIEW_NOTE));
    }

    #[test]
    fn merge_conflict_reopen_copy_is_placeholder() {
        assert!(DONE_COLUMN_HOLDS.starts_with("[placeholder]"));
        assert!(MERGE_CONFLICT_REOPEN_FAILED.starts_with("[placeholder]"));
        let comment =
            merge_conflict_reopen_comment("main", &["README.md".into(), "src/lib.rs".into()]);
        assert!(comment.starts_with("[placeholder]"));
        assert!(comment.contains("- README.md"));
        assert!(comment.contains("- src/lib.rs"));
        assert!(comment.contains("In Progress"));
        assert!(comment.contains("Human Review"));
    }
}
