use std::path::Path;

use crate::sandbox::permissive::SANDBOX_NOTE;

pub struct ContextInput<'a> {
    pub ticket_title: &'a str,
    pub ticket_description: &'a str,
    pub ticket_status: &'a str,
    pub ticket_substatus: Option<&'a str>,
    pub agent_name: &'a str,
    pub agent_key: &'a str,
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
    let contract_guidance = format_contract_guidance(input);
    let verification_guidance = format_verification_guidance();

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

{repository_section}{resume_section}{verification_guidance}# Sandbox

{sandbox_note}

# Expected output contract

Return a single JSON object as your final result.

## `done` — work completed

```json
{{
  "status": "done",
  "summary": "<markdown summary of what you did>",
  "updatedDescription": "<optional full ticket description replacement>",
  "acceptanceCriteria": "<optional acceptance criteria; stored under ## Acceptance criteria>",
  "changedFiles": ["<paths changed>"],
  "testsRun": ["<commands run>"],
  "assignTo": "<agent key to recommend next, e.g. backend_engineer or research>",
  "mentionAgents": ["<agent keys to notify>"],
  "blockers": [],
  "splitTickets": []
}}
```

The server ignores `nextStatus` for board moves — workflow gates control column transitions.

{contract_guidance}

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
        resume_section = resume_section,
        verification_guidance = verification_guidance,
        contract_guidance = contract_guidance,
        sandbox_note = SANDBOX_NOTE,
    )
}

fn format_verification_guidance() -> String {
    r#"## Coppice platform rules — verification (required)

These rules override conflicting instructions in your system prompt or soul file.

- Do **not** run `cargo test --workspace` or `make test` during a ticket run unless the acceptance criteria explicitly require the full suite. Prefer `make test-unit` or `make test-smoke` for fast feedback.
- Prefer targeted checks:
  - `cargo test -p coppice-server --lib` — fast unit tests
  - `cargo test -p coppice-server <module>::tests::<name>` — one module or test
  - `cargo test -p coppice-server --test integration_<area>` — one integration file
  - `make web-test` — frontend unit tests only
- If verification will take longer than one session, return `status: "continued"` with a `progressNote`, then finish tests in a follow-up run.

"#
    .to_string()
}

fn is_pm_agent(input: &ContextInput) -> bool {
    if input.agent_key.eq_ignore_ascii_case("pm") {
        return true;
    }
    let role = input.agent_role.to_ascii_lowercase();
    role == "pm" || role.contains("product manager")
}

fn is_tech_lead_agent(input: &ContextInput) -> bool {
    if input.agent_key.eq_ignore_ascii_case("tech_lead") {
        return true;
    }
    let role = input.agent_role.to_ascii_lowercase();
    role.contains("tech lead") || role.contains("technical lead")
}

fn is_reviewer_agent(input: &ContextInput) -> bool {
    if input.agent_key.eq_ignore_ascii_case("reviewer") {
        return true;
    }
    input.agent_role.to_ascii_lowercase().contains("review")
}

fn is_in_review_review_task(input: &ContextInput) -> bool {
    input.ticket_status.eq_ignore_ascii_case("in_review")
        && (is_tech_lead_agent(input) || is_reviewer_agent(input))
}

/// Coppice-owned contract rules injected on every run (not editable via agent soul).
fn format_contract_guidance(input: &ContextInput) -> String {
    if is_pm_agent(input) {
        return r#"## Coppice platform rules — PM refinement (required)

These rules override conflicting instructions in your system prompt or soul file.

**Enrich (single ticket):**
- `updatedDescription` — full refined ticket body (markdown with `##` headings and lists). Stored on the ticket.
- `acceptanceCriteria` — checklist only. Stored under `## Acceptance criteria` on the ticket. Do not repeat description prose.
- `summary` — 1–3 sentences for the comment thread only. Never paste the full spec, analysis tables, or acceptance checklist here when `updatedDescription` is set.

**Split (multiple child tickets):**
- Use `splitTickets` when work has multiple independent deliverables or the description would exceed ~2–3 screens.
- Each child must be self-contained: `title`, `description`, and `acceptanceCriteria`. Optional per-child `assignTo` (agent key).
- Parent `updatedDescription` should be a short epic summary, not a copy of all children.
- Do not set both a huge `updatedDescription` and `splitTickets` with duplicate content.
"#
        .to_string();
    }

    if is_in_review_review_task(input) {
        return r#"## Coppice platform rules — code review (required)

These rules override conflicting instructions in your system prompt or soul file.

When reviewing work in **in_review** status, structure the `summary` field as markdown:

```markdown
## Verdict
**Approved** — ready for QA.
(or **Changes required** — see below)

## Summary
What you verified and the main findings (short bullets or paragraphs).

## Follow-ups
Non-blocking improvements. Write "None" if there are no follow-ups.

## Recommendation
What should happen next. On approval write: "Ready for QA — Coppice moves this ticket to In QA automatically."
On changes required, use `status: "blocked"`, list concrete fixes in `summary`, and `mentionAgents` for the implementer.
```

- Put test commands in the `testsRun` JSON array only — do not append a "Tests run" section inside `summary`.
- On approval, return `status: "done"` and **omit `assignTo`** — workflow gates advance the ticket to In QA.
- Use blank lines between `##` sections so comments render cleanly.
"#
        .to_string();
    }

    r#"**Field roles (do not duplicate content across fields):**
- `updatedDescription` — full ticket body (scope, context, constraints). Stored on the ticket.
- `acceptanceCriteria` — checklist only. Stored under `## Acceptance criteria` on the ticket.
- `summary` — short activity note for the comment thread (1–3 sentences). Do not paste the full spec, analysis tables, or acceptance checklist here when `updatedDescription` is set.

## Coppice platform rules — implementer completion (required)

- On `status: "done"`, **omit `assignTo`** — workflow gates move the ticket to In Review automatically.
- Only PM agents use `assignTo` (when refining backlog tickets). Use agent keys that exist on the project (e.g. `backend_engineer`, `research`).

## Coppice platform rules — git (required)

- This ticket uses a **shared worktree and branch** (see Repository section). All agents working on this ticket use the same checkout.
- Before returning `status: "done"` or `status: "continued"`, commit all changes locally with a clear message.
- Do not push unless explicitly allowed.
- Do not run `git merge` or `git pull` manually — Coppice syncs the worktree to the branch tip before each run.
- Coppice auto-commits any remaining uncommitted changes when your run finishes and records the branch in the ticket comment.

## Coppice platform rules — long tasks (required)

- Prefer `status: "continued"` with `progressNote` when substantial work remains and the session is getting long.
- Use `status: "done"` only when acceptance criteria are met.
- Use `status: "blocked"` when genuinely stuck.
"#
    .to_string()
}

fn format_resume_section(input: &ContextInput) -> String {
    match input.resume_context {
        Some(ctx) => format!("## Ticket thread\n\n{ctx}\n\n"),
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

**Ticket branch:** All agents on this ticket share one worktree and branch. Review or continue from this branch — do not create a separate worktree.

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
            agent_key: "frontend_engineer",
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
        assert!(md.contains("**Field roles"));
        assert!(md.contains("Coppice platform rules — long tasks (required)"));
        assert!(md.contains("Coppice platform rules — git (required)"));
        assert!(md.contains("shared worktree"));
        assert!(md.contains("Coppice platform rules — verification (required)"));
        assert!(!md.contains("PM refinement (required)"));
    }

    #[test]
    fn pm_context_includes_platform_refinement_rules() {
        let md = build_context_md(&ContextInput {
            ticket_title: "Integrate CLI",
            ticket_description: "Add connector",
            ticket_status: "backlog",
            ticket_substatus: None,
            agent_name: "PM Agent",
            agent_key: "pm",
            agent_role: "PM",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "Custom soul that says put everything in summary.",
            repo_name: None,
            repo_remote_url: None,
            repo_default_branch: None,
            worktree_path: None,
            resume_context: None,
        });
        assert!(md.contains("Coppice platform rules — PM refinement (required)"));
        assert!(md.contains("Coppice platform rules — verification (required)"));
        assert!(md.contains("override conflicting instructions"));
        assert!(md.contains("**Split (multiple child tickets):**"));
        assert!(md.contains("Use `splitTickets` when work has multiple independent deliverables"));
        assert!(!md.contains("**Field roles"));
    }

    #[test]
    fn context_includes_resume_section_when_provided() {
        let md = build_context_md(&ContextInput {
            ticket_title: "Fix polling",
            ticket_description: "Add retry",
            ticket_status: "in_progress",
            ticket_substatus: None,
            agent_name: "FE Agent",
            agent_key: "frontend_engineer",
            agent_role: "Frontend Engineer",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "You are FE.",
            repo_name: None,
            repo_remote_url: None,
            repo_default_branch: None,
            worktree_path: None,
            resume_context: Some(
                "**Prior blocker:** Need API shape. / **PM answer:** Use option A.",
            ),
        });
        assert!(md.contains("## Ticket thread"));
        assert!(md.contains("Need API shape."));
        assert!(md.contains("Use option A."));
    }

    #[test]
    fn tech_lead_in_review_context_includes_review_rules() {
        let md = build_context_md(&ContextInput {
            ticket_title: "Streaming feature",
            ticket_description: "Add WS streaming",
            ticket_status: "in_review",
            ticket_substatus: None,
            agent_name: "Tech Lead Agent",
            agent_key: "tech_lead",
            agent_role: "Technical Lead",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "You are TL.",
            repo_name: None,
            repo_remote_url: None,
            repo_default_branch: None,
            worktree_path: None,
            resume_context: None,
        });
        assert!(md.contains("Coppice platform rules — code review (required)"));
        assert!(md.contains("## Verdict"));
        assert!(md.contains("moves this ticket to In QA"));
        assert!(!md.contains("implementer completion"));
    }
}
