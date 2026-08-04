use std::path::Path;

use crate::domain::context_profile::ContextProfile;
use crate::sandbox::permissive::SANDBOX_NOTE;
use uuid::Uuid;

pub struct HumanRequest<'a> {
    pub body: &'a str,
    pub posted_at: &'a str,
    pub mode_label: &'a str, // "Agent" | "Chat"
}

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
    pub latest_comments: Option<&'a str>,
    pub project_rules: Option<&'a str>,
    pub resume_context: Option<&'a str>,
    pub context_profile: ContextProfile,
    pub human_request: Option<HumanRequest<'a>>,
    pub ticket_id: Option<Uuid>,
    pub assignee_agent_key: Option<&'a str>,
    pub thread_excerpt: Option<&'a str>,
}

pub fn build_context_md(input: &ContextInput) -> String {
    match input.context_profile {
        ContextProfile::Full => build_full_context(input),
        ContextProfile::HumanAgent => build_human_agent_context(input),
        ContextProfile::HumanChat => build_human_chat_context(input),
    }
}

fn build_full_context(input: &ContextInput) -> String {
    let substatus_line = match input.ticket_substatus {
        Some(substatus) => format!("**Substatus:** {substatus}\n\n"),
        None => String::new(),
    };

    let skills = format_bullet_list(input.agent_skills);
    let responsibilities = format_bullet_list(input.agent_responsibilities);
    let repository_section = format_repository_section(input);
    let latest_comments_section = format_latest_comments_section(input);
    let project_rules_section = format_project_rules_section(input);
    let resume_section = format_resume_section(input);
    let verification_guidance = format_verification_guidance();
    let output_contract = format_full_output_contract(input);

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

{repository_section}{latest_comments_section}{resume_section}{project_rules_section}{verification_guidance}# Sandbox

{sandbox_note}

{output_contract}
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
        latest_comments_section = latest_comments_section,
        resume_section = resume_section,
        project_rules_section = project_rules_section,
        verification_guidance = verification_guidance,
        sandbox_note = SANDBOX_NOTE,
        output_contract = output_contract,
    )
}

pub fn format_full_output_contract(input: &ContextInput<'_>) -> String {
    let contract_guidance = format_contract_guidance(input);
    format!(
        r#"# Expected output contract

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
    )
}

fn build_human_agent_context(input: &ContextInput) -> String {
    let human_block = format_human_request_block(input.human_request.as_ref(), true);
    let snapshot = format_ticket_snapshot_human_agent(input);
    let skills = format_bullet_list(input.agent_skills);
    let responsibilities = format_bullet_list(input.agent_responsibilities);
    let repository_section = format_repository_section(input);
    let verification_guidance = format_verification_guidance();
    let git_rules = format_git_rules();
    let on_demand = format_on_demand_section();

    format!(
        r#"{human_block}{snapshot}# Agent role

**Name:** {agent_name}
**Role:** {agent_role}

**Skills:**
{skills}

**Responsibilities:**
{responsibilities}

**System prompt:**

{system_prompt}

{repository_section}{verification_guidance}# Sandbox

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
  "mentionAgents": ["<agent keys to notify>"],
  "blockers": [],
  "splitTickets": []
}}
```

{git_rules}
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

{on_demand}"#,
        human_block = human_block,
        snapshot = snapshot,
        agent_name = input.agent_name,
        agent_role = input.agent_role,
        skills = skills,
        responsibilities = responsibilities,
        system_prompt = input.agent_system_prompt,
        repository_section = repository_section,
        verification_guidance = verification_guidance,
        sandbox_note = SANDBOX_NOTE,
        git_rules = git_rules,
        on_demand = on_demand,
    )
}

fn build_human_chat_context(input: &ContextInput) -> String {
    let human_block = format_human_request_block(input.human_request.as_ref(), false);
    let thread_section = format_thread_excerpt_section(input.thread_excerpt);
    let snapshot = format_ticket_snapshot_minimal(input);
    let skills = format_bullet_list(input.agent_skills);
    let responsibilities = format_bullet_list(input.agent_responsibilities);
    let chat_contract = format_human_chat_contract_guidance();
    let on_demand = format_on_demand_section();

    format!(
        r#"{human_block}{thread_section}{snapshot}# Agent role

**Name:** {agent_name}
**Role:** {agent_role}

**Skills:**
{skills}

**Responsibilities:**
{responsibilities}

**System prompt:**

{system_prompt}

# Expected output contract

Return a single JSON object as your final result.

## `done` — reply to the human

```json
{{
  "status": "done",
  "summary": "<concise markdown reply to the human>"
}}
```

{chat_contract}
## `blocked` — cannot proceed

```json
{{
  "status": "blocked",
  "blockerType": "<missing_capability | missing_secret | permission | needs_human | ...>",
  "summary": "<why you are blocked>",
  "mentionAgents": ["<agent keys to notify>"]
}}
```

{on_demand}"#,
        human_block = human_block,
        thread_section = thread_section,
        snapshot = snapshot,
        agent_name = input.agent_name,
        agent_role = input.agent_role,
        skills = skills,
        responsibilities = responsibilities,
        system_prompt = input.agent_system_prompt,
        chat_contract = chat_contract,
        on_demand = on_demand,
    )
}

fn format_human_request_block(human: Option<&HumanRequest<'_>>, for_agent: bool) -> String {
    let Some(human) = human else {
        return String::new();
    };

    let mut block = format!(
        r#"# Human request (read this first)

**From:** Human
**Posted:** {posted_at}
**Mode:** {mode_label}

> {body}

This instruction overrides ticket description and thread summaries when they conflict.
"#,
        posted_at = human.posted_at,
        mode_label = human.mode_label,
        body = human.body,
    );

    if for_agent {
        block.push_str(
            "\nExecute in the ticket worktree unless the request is purely informational (then reply in your result summary only).\n\n",
        );
    } else {
        block.push('\n');
    }

    block
}

fn format_on_demand_section() -> String {
    r#"## On-demand ticket data

If you need full description, history, or past runs, read:
- `.agent/ticket.json`
- `.agent/comments.json`
- `.agent/runs.json`

Do not load these unless necessary for the human request.
"#
    .to_string()
}

fn format_ticket_snapshot_human_agent(input: &ContextInput) -> String {
    let substatus_line = match input.ticket_substatus {
        Some(substatus) => format!("**Substatus:** {substatus}\n\n"),
        None => String::new(),
    };
    let assignee = input.assignee_agent_key.unwrap_or("(unassigned)");
    let ticket_id = input
        .ticket_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "(unknown)".to_string());

    format!(
        r#"# Ticket snapshot

**Title:** {title}

**Status:** {status}

{substatus}**Assignee:** {assignee}

**Ticket ID:** {ticket_id}

"#,
        title = input.ticket_title,
        status = input.ticket_status,
        substatus = substatus_line,
        assignee = assignee,
        ticket_id = ticket_id,
    )
}

fn format_ticket_snapshot_minimal(input: &ContextInput) -> String {
    format!(
        r#"# Ticket snapshot

**Title:** {title}

**Status:** {status}

"#,
        title = input.ticket_title,
        status = input.ticket_status,
    )
}

fn format_thread_excerpt_section(excerpt: Option<&str>) -> String {
    match excerpt {
        Some(excerpt) => format!("## Recent thread\n\n{excerpt}\n\n"),
        None => String::new(),
    }
}

fn format_git_rules() -> String {
    r#"## Coppice platform rules — git (required)

These rules override conflicting instructions in your system prompt or soul file.

- This ticket uses a **shared worktree and branch** (see Repository section). All agents working on this ticket use the same checkout.
- Before returning `status: "done"` or `status: "continued"`, commit all changes locally with a clear message.
- Do not push unless explicitly allowed.
- Do not run `git merge` or `git pull` manually — Coppice syncs the worktree to the branch tip before each run.
- Coppice auto-commits any remaining uncommitted changes when your run finishes and records the branch in the ticket comment.

"#
    .to_string()
}

fn format_human_chat_contract_guidance() -> String {
    r#"## Coppice platform rules — human chat reply (required)

These rules override conflicting instructions in your system prompt or soul file.

- Reply to the human's question or request with a **concise markdown summary** in the `summary` field of your JSON result.
- Do **not** use `assignTo` — human chat runs do not change ticket workflow or assignment.
- Do **not** assume you should edit code or use the worktree unless the human explicitly asks for implementation help; prefer answering in the summary.
- Omit `updatedDescription`, `acceptanceCriteria`, and `changedFiles` unless the human asked you to update the ticket.
- Use `status: "done"` for a complete reply; use `status: "blocked"` only if you genuinely cannot answer.

"#
    .to_string()
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

fn is_in_qa_qc_task(input: &ContextInput) -> bool {
    if !input.ticket_status.eq_ignore_ascii_case("in_qa") {
        return false;
    }
    if input.agent_key.eq_ignore_ascii_case("qc") {
        return true;
    }
    input.agent_role.to_ascii_lowercase().contains("quality")
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
On changes required, use `status: "blocked"`, list concrete fixes in `summary`, and `mentionAgents` for the implementer (e.g. `["backend_engineer"]`).
```

- Put test commands in the `testsRun` JSON array only — do not append a "Tests run" section inside `summary`.
- On approval, return `status: "done"` and **omit `assignTo` and `mentionAgents`** — workflow gates advance the ticket to In QA.
- When changes are required, set `mentionAgents` to the implementer agent key — Coppice assigns them and auto-starts a fix run.
- Use blank lines between `##` sections so comments render cleanly.
"#
        .to_string();
    }

    if is_in_qa_qc_task(input) {
        return r#"## Coppice platform rules — QA verification (required)

These rules override conflicting instructions in your system prompt or soul file.

**Your role is verification-only.** You may inspect code, run tests, and gather evidence. You must **not** edit, patch, or fix source files, configuration, or product behavior — fixing is the implementing engineer's job. Leave `changedFiles` empty; any changes you make will not be committed or treated as the implementation.

**On pass (no defects):** return `status: "done"` with a short summary. Coppice moves the ticket to Wait for Final Review. Omit `assignTo` and `mentionAgents`.

**On defects:** report a defect comment — do **not** fix it yourself. Return `status: "done"` with:
- `blockers`: one entry per defect, each with reproduction steps, the failed check or test, and expected vs actual behavior.
- `mentionAgents`: `["backend_engineer"]` (the implementing engineer agent key on this project). Coppice assigns that engineer, appends the `@agent` mention to the comment, and auto-starts their fix run when `auto_start_runs` is enabled.
- Do **not** use `assignTo` or attempt to set status yourself — the workflow gate drives the handoff from `blockers` + `mentionAgents` and returns the ticket to In Progress.

Put test commands in `testsRun` only — not inside `summary`.
"#
        .to_string();
    }

    format!(
        "{}{}",
        r#"**Field roles (do not duplicate content across fields):**
- `updatedDescription` — full ticket body (scope, context, constraints). Stored on the ticket.
- `acceptanceCriteria` — checklist only. Stored under `## Acceptance criteria` on the ticket.
- `summary` — short activity note for the comment thread (1–3 sentences). Do not paste the full spec, analysis tables, or acceptance checklist here when `updatedDescription` is set.

## Coppice platform rules — implementer completion (required)

- On `status: "done"`, **omit `assignTo`** — workflow gates move the ticket to In Review automatically.
- Only PM agents use `assignTo` (when refining backlog tickets). Use agent keys that exist on the project (e.g. `backend_engineer`, `research`).

"#,
        format_git_rules(),
    ) + r#"## Coppice platform rules — long tasks (required)

- Prefer `status: "continued"` with `progressNote` when substantial work remains and the session is getting long.
- Use `status: "done"` only when acceptance criteria are met.
- Use `status: "blocked"` when genuinely stuck.
"#
}

fn format_resume_section(input: &ContextInput) -> String {
    match input.resume_context {
        Some(ctx) => format!("# Previous attempt summary\n\n{ctx}\n\n"),
        None => String::new(),
    }
}

fn format_latest_comments_section(input: &ContextInput) -> String {
    match input.latest_comments {
        Some(comments) => format!("# Latest comments\n\n{comments}\n\n"),
        None => String::new(),
    }
}

fn format_project_rules_section(input: &ContextInput) -> String {
    match input.project_rules {
        Some(rules) => format!("# Project rules\n\n{rules}\n\n"),
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
    write_context_document(worktree, &build_context_md(input))
}

pub fn write_context_document(worktree: &Path, markdown: &str) -> std::io::Result<()> {
    let agent_dir = worktree.join(".agent");
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(agent_dir.join("context.md"), markdown)?;
    Ok(())
}

pub fn write_agent_context_files(
    worktree: &Path,
    ticket_json: &serde_json::Value,
    comments_json: &serde_json::Value,
    runs_json: &serde_json::Value,
) -> std::io::Result<()> {
    let agent_dir = worktree.join(".agent");
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(
        agent_dir.join("ticket.json"),
        serde_json::to_string_pretty(ticket_json)?,
    )?;
    std::fs::write(
        agent_dir.join("comments.json"),
        serde_json::to_string_pretty(comments_json)?,
    )?;
    std::fs::write(
        agent_dir.join("runs.json"),
        serde_json::to_string_pretty(runs_json)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn full_profile_defaults() -> (
        ContextProfile,
        Option<HumanRequest<'static>>,
        Option<Uuid>,
        Option<&'static str>,
        Option<&'static str>,
    ) {
        (
            ContextProfile::Full,
            None,
            None,
            None,
            None,
        )
    }

    #[test]
    fn context_includes_required_sections() {
        let (context_profile, human_request, ticket_id, assignee_agent_key, thread_excerpt) =
            full_profile_defaults();
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
            latest_comments: None,
            project_rules: None,
            resume_context: None,
            context_profile,
            human_request,
            ticket_id,
            assignee_agent_key,
            thread_excerpt,
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
        let (context_profile, human_request, ticket_id, assignee_agent_key, thread_excerpt) =
            full_profile_defaults();
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
            latest_comments: None,
            project_rules: None,
            resume_context: None,
            context_profile,
            human_request,
            ticket_id,
            assignee_agent_key,
            thread_excerpt,
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
        let (context_profile, human_request, ticket_id, assignee_agent_key, thread_excerpt) =
            full_profile_defaults();
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
            latest_comments: None,
            project_rules: None,
            resume_context: Some(
                "**Prior blocker:** Need API shape. / **PM answer:** Use option A.",
            ),
            context_profile,
            human_request,
            ticket_id,
            assignee_agent_key,
            thread_excerpt,
        });
        assert!(md.contains("# Previous attempt summary"));
        assert!(md.contains("Need API shape."));
        assert!(md.contains("Use option A."));
    }

    #[test]
    fn tech_lead_in_review_context_includes_review_rules() {
        let (context_profile, human_request, ticket_id, assignee_agent_key, thread_excerpt) =
            full_profile_defaults();
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
            latest_comments: None,
            project_rules: None,
            resume_context: None,
            context_profile,
            human_request,
            ticket_id,
            assignee_agent_key,
            thread_excerpt,
        });
        assert!(md.contains("Coppice platform rules — code review (required)"));
        assert!(md.contains("## Verdict"));
        assert!(md.contains("moves this ticket to In QA"));
        assert!(!md.contains("implementer completion"));
    }

    #[test]
    fn qc_in_qa_context_is_verification_only_with_mention_handoff() {
        let (context_profile, human_request, ticket_id, assignee_agent_key, thread_excerpt) =
            full_profile_defaults();
        let md = build_context_md(&ContextInput {
            ticket_title: "Verify retry behavior",
            ticket_description: "QC the retry fix",
            ticket_status: "in_qa",
            ticket_substatus: None,
            agent_name: "QC Agent",
            agent_key: "qc",
            agent_role: "QC",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "You are QC.",
            repo_name: None,
            repo_remote_url: None,
            repo_default_branch: None,
            worktree_path: None,
            latest_comments: None,
            project_rules: None,
            resume_context: None,
            context_profile,
            human_request,
            ticket_id,
            assignee_agent_key,
            thread_excerpt,
        });
        assert!(md.contains("Coppice platform rules — QA verification (required)"));
        // Verification-only: must not edit or fix source.
        assert!(md.contains("verification-only"));
        assert!(md.contains("must **not** edit, patch, or fix"));
        // Defect contract: mentionAgents to the engineer, no assignTo/status manipulation.
        assert!(md.contains("mentionAgents"));
        assert!(md.contains("`[\"backend_engineer\"]`"));
        assert!(md.contains("Do **not** use `assignTo`"));
        assert!(md.contains("blockers"));
        assert!(md.contains("reproduction steps"));
        // Pass path preserved.
        assert!(md.contains("Wait for Final Review"));
        // Implementer rules must not leak into the QC contract.
        assert!(!md.contains("implementer completion"));
    }

    #[test]
    fn full_profile_unchanged() {
        let (context_profile, human_request, ticket_id, assignee_agent_key, thread_excerpt) =
            full_profile_defaults();
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
            latest_comments: None,
            project_rules: None,
            resume_context: None,
            context_profile,
            human_request,
            ticket_id,
            assignee_agent_key,
            thread_excerpt,
        });
        assert!(md.contains("# Current task"));
        assert!(md.contains("**Description:**"));
        assert!(md.contains("Add retry"));
        assert!(!md.contains("# Human request (read this first)"));
        assert!(!md.contains("On-demand ticket data"));
    }

    #[test]
    fn human_agent_puts_human_request_first() {
        let ticket_id = Uuid::new_v4();
        let human_request = HumanRequest {
            body: "Please fix the retry logic in the poller.",
            posted_at: "2026-06-14T12:00:00Z",
            mode_label: "Agent",
        };
        let md = build_context_md(&ContextInput {
            ticket_title: "Fix polling",
            ticket_description: "Full description that should not appear",
            ticket_status: "in_progress",
            ticket_substatus: Some("implementing"),
            agent_name: "FE Agent",
            agent_key: "frontend_engineer",
            agent_role: "Frontend Engineer",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "You are FE.",
            repo_name: Some("coppice"),
            repo_remote_url: None,
            repo_default_branch: Some("main"),
            worktree_path: Some("/data/worktrees/coppice/ticket-1"),
            latest_comments: None,
            project_rules: None,
            resume_context: Some("Full thread that should not appear"),
            context_profile: ContextProfile::HumanAgent,
            human_request: Some(human_request),
            ticket_id: Some(ticket_id),
            assignee_agent_key: Some("frontend_engineer"),
            thread_excerpt: None,
        });

        let human_pos = md.find("# Human request (read this first)").expect("human block");
        let snapshot_pos = md.find("# Ticket snapshot").expect("snapshot");
        let agent_pos = md.find("# Agent role").expect("agent role");
        assert!(human_pos < snapshot_pos);
        assert!(snapshot_pos < agent_pos);
        assert!(md.contains("Please fix the retry logic in the poller."));
        assert!(md.contains("**Mode:** Agent"));
        assert!(md.contains("Execute in the ticket worktree unless the request is purely informational"));
        assert!(md.contains(&format!("**Ticket ID:** {ticket_id}")));
        assert!(md.contains("**Assignee:** frontend_engineer"));
        assert!(md.contains("**Substatus:** implementing"));
    }

    #[test]
    fn human_agent_omits_description_and_full_thread() {
        let md = build_context_md(&ContextInput {
            ticket_title: "Fix polling",
            ticket_description: "Full description that should not appear",
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
            latest_comments: None,
            project_rules: None,
            resume_context: Some("Full thread that should not appear"),
            context_profile: ContextProfile::HumanAgent,
            human_request: Some(HumanRequest {
                body: "Quick fix please",
                posted_at: "2026-06-14T12:00:00Z",
                mode_label: "Agent",
            }),
            ticket_id: None,
            assignee_agent_key: None,
            thread_excerpt: None,
        });

        assert!(!md.contains("Full description that should not appear"));
        assert!(!md.contains("Full thread that should not appear"));
        assert!(!md.contains("# Current task"));
        assert!(!md.contains("## Ticket thread"));
        assert!(!md.contains("**Field roles"));
        assert!(!md.contains("implementer completion"));
        assert!(!md.contains("PM refinement"));
        assert!(md.contains("Coppice platform rules — git (required)"));
        assert!(md.contains("Coppice platform rules — verification (required)"));
        assert!(md.contains("On-demand ticket data"));
        assert!(!md.contains("\"assignTo\""));
    }

    #[test]
    fn human_chat_includes_short_excerpt_only() {
        let md = build_context_md(&ContextInput {
            ticket_title: "Fix polling",
            ticket_description: "Full description that should not appear",
            ticket_status: "in_progress",
            ticket_substatus: Some("implementing"),
            agent_name: "FE Agent",
            agent_key: "frontend_engineer",
            agent_role: "Frontend Engineer",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "You are FE.",
            repo_name: Some("coppice"),
            repo_remote_url: None,
            repo_default_branch: None,
            worktree_path: None,
            latest_comments: None,
            project_rules: None,
            resume_context: Some("Full resume thread"),
            context_profile: ContextProfile::HumanChat,
            human_request: Some(HumanRequest {
                body: "What is the current status?",
                posted_at: "2026-06-14T12:00:00Z",
                mode_label: "Chat",
            }),
            ticket_id: None,
            assignee_agent_key: None,
            thread_excerpt: Some("- **Human:** Can you help?\n- **Agent:** Sure, working on it."),
        });

        assert!(md.contains("## Recent thread"));
        assert!(md.contains("Can you help?"));
        assert!(!md.contains("Full description that should not appear"));
        assert!(!md.contains("Full resume thread"));
        assert!(!md.contains("# Repository"));
        assert!(!md.contains("# Sandbox"));
        assert!(!md.contains("**Substatus:**"));
        assert!(md.contains("human chat reply (required)"));
        assert!(md.contains("concise markdown summary"));
        assert!(md.contains("On-demand ticket data"));
        assert!(!md.contains("\"assignTo\""));
    }
}
