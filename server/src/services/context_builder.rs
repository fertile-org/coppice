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
}

pub fn build_context_md(input: &ContextInput) -> String {
    let substatus_line = match input.ticket_substatus {
        Some(substatus) => format!("**Substatus:** {substatus}\n\n"),
        None => String::new(),
    };

    let skills = format_bullet_list(input.agent_skills);
    let responsibilities = format_bullet_list(input.agent_responsibilities);

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

# Sandbox

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
  "nextStatus": "<board column, e.g. In Review>",
  "mentionAgents": ["<agent keys to notify>"],
  "blockers": []
}}
```

## `blocked` — cannot proceed

```json
{{
  "status": "blocked",
  "blockerType": "<missing_capability | missing_secret | permission | needs_human | ...>",
  "summary": "<why you are blocked>",
  "nextStatus": "<board column, e.g. Blocked>",
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
        sandbox_note = SANDBOX_NOTE,
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
        });
        assert!(md.contains("# Current task"));
        assert!(md.contains("# Agent role"));
        assert!(md.contains("# Sandbox"));
        assert!(md.contains("# Expected output contract"));
        assert!(md.contains("Fix polling"));
    }
}
