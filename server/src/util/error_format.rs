const MAX_JOB_ERROR_LEN: usize = 4000;

pub fn format_job_error(err: &anyhow::Error) -> String {
    let message = format!("{err:#}").trim().to_string();
    if message.len() <= MAX_JOB_ERROR_LEN {
        return message;
    }
    let mut truncated = message;
    truncated.truncate(MAX_JOB_ERROR_LEN);
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("git command failed: git worktree add: fatal: not a git repository")]
    struct InnerGitError;

    #[test]
    fn format_job_error_includes_full_chain() {
        let err = anyhow::Error::new(InnerGitError).context("ensure worktree");
        let formatted = format_job_error(&err);
        assert!(formatted.contains("ensure worktree"));
        assert!(formatted.contains("git command failed"));
        assert!(formatted.contains("not a git repository"));
    }

    #[test]
    fn format_job_error_truncates_long_messages() {
        let err = anyhow::anyhow!("x".repeat(5000));
        let formatted = format_job_error(&err);
        assert!(formatted.len() <= MAX_JOB_ERROR_LEN + 4);
        assert!(formatted.ends_with('…'));
    }
}
