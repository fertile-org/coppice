//! `coppice-git` skill body: commit attribution appended when settings allow.

pub const AGENT_COAUTHOR_TRAILER: &str =
    "Co-authored-by: Coppice Agent <agent@getcoppice.app>";

const BUILTIN_GIT_SKILL_IDS: [&str; 2] = ["coppice-git", "coppice:coppice-git"];

pub fn is_coppice_git_skill(id: &str) -> bool {
    BUILTIN_GIT_SKILL_IDS.contains(&id)
}

/// Appends commit attribution instructions when `agent_commit_attribution` is on.
pub fn augment_coppice_git_body(body: &str, agent_commit_attribution: bool) -> String {
    if !agent_commit_attribution {
        return body.to_string();
    }
    format!(
        "{body}\n\n## Commit attribution (required)\n\n\
         When you create a commit, add this trailer at the end of the commit message \
         (after a blank line):\n\n\
         ```\n{AGENT_COAUTHOR_TRAILER}\n```\n\n\
         Prefer a HEREDOC (or your environment's equivalent) so the trailer is included reliably.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_appended_when_enabled() {
        let out = augment_coppice_git_body("base", true);
        assert!(out.starts_with("base"));
        assert!(out.contains(AGENT_COAUTHOR_TRAILER));
        assert!(out.contains("HEREDOC"));
    }

    #[test]
    fn attribution_omitted_when_disabled() {
        assert_eq!(augment_coppice_git_body("base", false), "base");
    }
}
