//! Copy shown after Coppice restarts during a run.
//!
//! Replace these strings word for word. The API sends them to the web app.

/// Ticket comment when nothing changed.
pub const COMMENT_NONE: &str = "This ticket has no changes yet.";

/// Ticket comment when one file changed.
pub const COMMENT_FILE: &str = "This ticket already changed 1 file.";

/// Ticket comment when more than one file changed. `{count}` is that number.
pub const COMMENT_FILES: &str = "This ticket already changed {count} files.";

pub const ACTION_RESUME: &str = "Keep the changes";
pub const ACTION_DISCARD: &str = "Start over";
pub const ACTION_CANCEL: &str = "Cancel";

/// Confirm dialog when one file changed.
pub const CONFIRM_DISCARD_FILE: &str =
    "Delete the changes to 1 file and start over? You can't undo this.";

/// Confirm dialog when more than one file changed. `{count}` is that number.
pub const CONFIRM_DISCARD: &str =
    "Delete the changes to {count} files and start over? You can't undo this.";

pub const ERROR_NONE: &str = "There's no earlier work on this ticket to keep or delete.";
pub const ERROR_USE_DISCARD: &str =
    "The earlier work can't be continued. Choose Start over to begin again.";
pub const ERROR_USE_CANCEL: &str =
    "There's nothing to throw away. Choose Cancel to clear this ticket's earlier run.";

/// `{err}` is the git error.
pub const ERROR_GIT: &str =
    "Couldn't finish restarting: {err}. Check the ticket's files before you try again.";

pub fn comment_for(changed_files: u32) -> String {
    if changed_files == 0 {
        COMMENT_NONE.to_string()
    } else if changed_files == 1 {
        COMMENT_FILE.to_string()
    } else {
        COMMENT_FILES.replace("{count}", &changed_files.to_string())
    }
}

pub fn confirm_discard(changed_files: u32) -> String {
    if changed_files == 1 {
        CONFIRM_DISCARD_FILE.to_string()
    } else {
        CONFIRM_DISCARD.replace("{count}", &changed_files.to_string())
    }
}

pub fn git_error(err: &str) -> String {
    ERROR_GIT.replace("{err}", err)
}
