//! Copy for a merge or rebase that stopped on a conflict.
//!
//! Replace these strings word for word. The API sends them to the web app.

/// "rebasing onto" in [`conflict_message`].
pub const REBASE_ACTION: &str = "rebasing onto";
/// "merging into" in [`conflict_message`].
pub const MERGE_ACTION: &str = "merging into";

pub const NO_ASSIGNEE: &str = "This ticket has no assignee.";

pub const REREVIEW_NOTE: &str =
    "Once this is resolved, the ticket comes back to Human Review. Accept it again before it can merge.";

pub const INVALID_FILE_LIST: &str = "The conflict file list is invalid.";

pub fn connector_not_ready_reason(assignee: &str) -> String {
    format!("{assignee}'s connector isn't ready.")
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
        "Conflict while {action} {branch}. These files conflict: {listed}. The branch is unchanged."
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
            "Conflict while rebasing onto main. These files conflict: README.md. The branch is unchanged."
        );
        assert_eq!(
            conflict_message(
                MERGE_ACTION,
                "main",
                &["README.md".into(), "src/lib.rs".into()]
            ),
            "Conflict while merging into main. These files conflict: README.md, src/lib.rs. The branch is unchanged."
        );
        assert_eq!(ask_to_resolve_label("Ada"), "Ask Ada to resolve");
        assert_eq!(NO_ASSIGNEE, "This ticket has no assignee.");
        assert_eq!(
            connector_not_ready_reason("Ada"),
            "Ada's connector isn't ready."
        );
        assert_eq!(
            REREVIEW_NOTE,
            "Once this is resolved, the ticket comes back to Human Review. Accept it again before it can merge."
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
}
