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
    "An agent is still working on this ticket. Cancel, then try again when it stops.";

/// `{err}` is the git error.
pub const ERROR_GIT: &str = "Couldn't restart: {err}. Nothing was changed.";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_files_uses_the_none_sentence() {
        assert_eq!(comment_for(0), COMMENT_NONE);
        assert_eq!(comment_for(0), "This ticket has no changes yet.");
        assert!(!comment_for(0).contains("files"));
    }

    #[test]
    fn file_comment_uses_singular_and_plural() {
        assert_eq!(comment_for(1), "This ticket already changed 1 file.");
        assert_eq!(comment_for(4), "This ticket already changed 4 files.");
        assert_eq!(
            confirm_discard(1),
            "Delete the changes to 1 file and start over? You can't undo this."
        );
        assert_eq!(
            confirm_discard(4),
            "Delete the changes to 4 files and start over? You can't undo this."
        );
    }

    #[test]
    fn git_error_includes_the_detail() {
        assert_eq!(
            git_error("fatal: not a git repository"),
            "Couldn't restart: fatal: not a git repository. Nothing was changed."
        );
    }
}
