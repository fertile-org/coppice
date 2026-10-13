//! Placeholders for restart copy. Content replaces these tokens.

/// No changed files.
pub const COMMENT_NONE: &str = "{{restart.comment.none}}";

/// Changed files. `{count}` is the real number.
pub const COMMENT_FILES: &str = "{{restart.comment.files:{count}}}";

pub const ACTION_RESUME: &str = "{{restart.resume}}";
pub const ACTION_DISCARD: &str = "{{restart.discard}}";
pub const ACTION_CANCEL: &str = "{{restart.cancel}}";

/// `{count}` is the same number as the comment.
pub const CONFIRM_DISCARD: &str = "{{restart.discard_confirm:{count}}}";

pub const ERROR_NONE: &str = "{{restart.error.none}}";
pub const ERROR_USE_DISCARD: &str = "{{restart.error.use_discard}}";
pub const ERROR_USE_CANCEL: &str = "{{restart.error.use_cancel}}";
pub const ERROR_GIT: &str = "{{restart.error.git}}";

pub fn comment_for(changed_files: u32) -> String {
    if changed_files == 0 {
        COMMENT_NONE.to_string()
    } else {
        COMMENT_FILES.replace("{count}", &changed_files.to_string())
    }
}

pub fn confirm_discard(changed_files: u32) -> String {
    CONFIRM_DISCARD.replace("{count}", &changed_files.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_files_drops_the_second_sentence() {
        let comment = comment_for(0);
        assert_eq!(comment, COMMENT_NONE);
        assert!(!comment.contains("files"));
    }

    #[test]
    fn file_comment_keeps_the_real_count() {
        assert_eq!(comment_for(4), "{{restart.comment.files:4}}");
        assert_eq!(confirm_discard(4), "{{restart.discard_confirm:4}}");
    }
}
