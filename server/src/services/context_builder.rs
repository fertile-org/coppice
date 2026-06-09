use std::path::Path;

use crate::sandbox::permissive::SANDBOX_NOTE;

pub struct ContextInput<'a> {
    pub ticket_title: &'a str,
    pub ticket_description: &'a str,
    pub ticket_status: &'a str,
    pub ticket_substatus: Option<&'a str>,
    pub agent_name: &'a str,
    pub agent_role: &'a str,
    pub agent_skills: &'a [String],
    pub agent_responsibilities: &'a [String],
    pub agent_system_prompt: &'a str,
    pub repo_name: Option<&'a str>,
    pub repo_remote_url: Option<&'a str>,
    pub repo_default_branch: Option<&'a str>,
    pub worktree_path: Option<&'a str>,
    pub resume_context: Option<&'a str>,
}

pub fn build_context_md(input: &ContextInput) -> String {
    let substatus_line = match input.ticket_substatus {
        Some(substatus) => format!("**Substatus:** {substatus}\n\n"),
        None => String::new(),
    };

    let skills = format_bullet_list(input.agent_skills);
    let responsibilities = format_bullet_list(input.agent_responsibilities);
    let repository_section = format_repository_section(input);
    let resume_section = format_resume_section(input);

    format!(
        r#"# Current task

**Title:** {title}

**Description:**

{description}

**Status:** {status}

{substatus}# Agent role

**Name:** {agent_name}
**Role:** {agent_role}

**Skills:**
{skills}

**Responsibilities:**
{responsibilities}

**System prompt:**

{system_prompt}

{repository_section}{resume_section}# Sandbox

{sandbox_note}

# Expected output contract

Return a single JSON object as your final result.

## `done` — work completed

```json
{{
  "status": "done",
  "summary": "<markdown summary of what you did>",
  "changedFiles": ["<paths changed>"],
  "testsRun": ["<commands run>"],
  "assignTo": "<agent key to recommend next, e.g. backend_engineer or research>",
  "mentionAgents": ["<agent keys to notify>"],
  "blockers": []
}}
```

The server ignores `nextStatus` for board moves — workflow gates control column transitions.

## `blocked` — cannot proceed

```json
{{
  "status": "blocked",
  "blockerType": "<missing_capability | missing_secret | permission | needs_human | ...>",
  "summary": "<why you are blocked>",
  "mentionAgents": ["<agent keys to notify>"]
}}
```

When blocked by missing capability or secret, also include `requiredCapabilities` and/or `requiredSecrets` arrays as applicable.
"#,
        title = input.ticket_title,
        description = input.ticket_description,
        status = input.ticket_status,
        substatus = substatus_line,
        agent_name = input.agent_name,
        agent_role = input.agent_role,
        skills = skills,
        responsibilities = responsibilities,
        system_prompt = input.agent_system_prompt,
        repository_section = repository_section,
        sandbox_note = SANDBOX_NOTE,
    )
}

fn format_resume_section(input: &ContextInput) -> String {
    match input.resume_context {
        Some(ctx) => format!("## Resume\n\n{ctx}\n\n"),
        None => String::new(),
    }
}

fn format_repository_section(input: &ContextInput) -> String {
    let name = input.repo_name.unwrap_or("(not set)");
    let default_branch = input.repo_default_branch.unwrap_or("(not set)");
    let worktree = input.worktree_path.unwrap_or("(not set)");

    let remote_line = match input.repo_remote_url {
        Some(url) => format!("**Remote URL:** {url}\n\n"),
        None => String::new(),
    };

    format!(
        r#"# Repository

**Name:** {name}

{remote_line}**Default branch:** {default_branch}

**Worktree path:** {worktree}

"#,
        remote_line = remote_line,
    )
}

fn format_bullet_list(items: &[String]) -> String {
    if items.is_empty() {
        return "- (none)\n".to_string();
    }

    items
        .iter()
        .map(|item| format!("- {item}\n"))
        .collect::<String>()
}

pub fn write_context_file(worktree: &Path, input: &ContextInput) -> std::io::Result<()> {
    let agent_dir = worktree.join(".agent");
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(agent_dir.join("context.md"), build_context_md(input))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_includes_required_sections() {
        let md = build_context_md(&ContextInput {
            ticket_title: "Fix polling",
            ticket_description: "Add retry",
            ticket_status: "in_progress",
            ticket_substatus: None,
            agent_name: "FE Agent",
            agent_role: "Frontend Engineer",
            agent_skills: &["react".into()],
            agent_responsibilities: &["implement UI".into()],
            agent_system_prompt: "You are FE.",
            repo_name: Some("coppice"),
            repo_remote_url: Some("https://github.com/example/coppice"),
            repo_default_branch: Some("main"),
            worktree_path: Some("/data/worktrees/coppice/ticket-1"),
            resume_context: None,
        });
        assert!(md.contains("# Current task"));
        assert!(md.contains("# Agent role"));
        assert!(md.contains("# Repository"));
        assert!(md.contains("**Name:** coppice"));
        assert!(md.contains("**Remote URL:** https://github.com/example/coppice"));
        assert!(md.contains("# Sandbox"));
        assert!(md.contains("# Expected output contract"));
        assert!(md.contains("Fix polling"));
    }

    #[test]
    fn context_includes_resume_section_when_provided() {
        let md = build_context_md(&ContextInput {
            ticket_title: "Fix polling",
            ticket_description: "Add retry",
            ticket_status: "in_progress",
            ticket_substatus: None,
            agent_name: "FE Agent",
            agent_role: "Frontend Engineer",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "You are FE.",
            repo_name: None,
            repo_remote_url: None,
            repo_default_branch: None,
            worktree_path: None,
            resume_context: Some("**Prior blocker:**\n\nNeed API shape.\n\n**PM answer:**\n\nUse option A."),
        });
        assert!(md.contains("## Resume"));
        assert!(md.contains("Need API shape."));
        assert!(md.contains("Use option A."));
    }
}
