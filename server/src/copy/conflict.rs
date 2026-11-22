//! Copy for a merge or rebase that stopped on a conflict.
//!
//! Replace these strings word for word. The API sends them to the web app.

/// "rebase onto" in [`conflict_message`].
pub const REBASE_ACTION: &str = "rebase onto";
/// "merge into" in [`conflict_message`].
pub const MERGE_ACTION: &str = "merge into";

pub const NO_ELIGIBLE_AGENT: &str = "No agent on this board can resolve the conflicts.";

pub const AGENT_NOT_ELIGIBLE: &str = "That agent cannot resolve this conflict.";

pub const REREVIEW_NOTE: &str =
    "When the conflicts are resolved, the ticket goes back to In Review. You'll need to accept it again before it merges.";

pub const INVALID_FILE_LIST: &str = "The conflict file list is invalid.";

/// Shown when a person tries to move a Done ticket to another column.
pub const DONE_COLUMN_HOLDS: &str =
    "Done tickets can't be moved. Create a new ticket for follow-up work.";

/// Comment posted when a person asks an agent to resolve a Done ticket's
/// merge conflict.
/// Also copied into that agent's run context.
pub fn merge_conflict_reopen_comment(branch: &str, files: &[String], agent_name: &str) -> String {
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
        "Couldn't merge into {branch} because these files conflict:\n{listed}\n\n\
         The ticket is back in In Progress with {agent_name}. You'll need to accept it again in Human Review before it merges."
    )
}

pub fn connector_not_ready_reason(agent_name: &str) -> String {
    format!("{agent_name}'s connector isn't ready. Check it in System → Connectors.")
}

pub fn ask_to_resolve_label(agent_name: &str) -> String {
    format!("Ask {agent_name} to resolve")
}

pub fn other_agent_active_reason(agent_name: &str) -> String {
    format!("{agent_name} already has an active run on this ticket.")
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

/// Instruction for the resolving agent's run. Includes [`REREVIEW_NOTE`].
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
