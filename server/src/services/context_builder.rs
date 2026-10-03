use std::path::Path;

use crate::domain::context_profile::ContextProfile;
use crate::domain::workflow::{self, is_ready_tech_lead_refinement};
use crate::plugins::skills::SkillInfo;
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
    // HumanChat: recent-thread excerpt. Full: None for owned work, Some("") for
    // other non-work runs, or Some(request) for a bounded consultation.
    pub thread_excerpt: Option<&'a str>,
}

/// Slim `.agent/context.md`: who the agent is, what the task is, and where to
/// fetch everything else (MCP tools and skills) instead of embedding it.
pub fn build_tool_first_context(
    input: &ContextInput<'_>,
    skills: &[SkillInfo],
    required_skill: Option<&str>,
) -> String {
    let mut out = format!(
        "# Agent\n\n**Name:** {}\n**Role:** {}\n\n",
        input.agent_name, input.agent_role
    );
    for (label, items) in [
        ("Skills", input.agent_skills),
        ("Responsibilities", input.agent_responsibilities),
    ] {
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("**{label}:**\n"));
        for item in items {
            out.push_str(&format!("- {item}\n"));
        }
        out.push('\n');
    }
    out.push_str(input.agent_system_prompt);
    out.push_str("\n\n");
    write_tool_first_task(&mut out, input);
    write_tool_first_repository(&mut out, input);

    out.push_str("# Skills\n\n");
    if skills.is_empty() {
        out.push_str("(none)\n");
    }
    for skill in skills {
        out.push_str(&format!("- {} — {}\n", skill.id, skill.description));
    }
    out.push_str("\nLoad a skill's instructions with `skill_load`.\n");
    if let Some(required) = required_skill {
        out.push_str(&format!("Load `{required}` before starting.\n"));
    }

    out.push_str(
        "\n# Coppice tools\n\n\
         Use `ticket_get` / `ticket_comments` / `ticket_runs` for ticket details.\n\
         Use `knowledge_search` for approved project knowledge.\n\
         Use `board_agents` to find agent keys for handoff.\n\
         Finish by calling `result_submit` with your result; fix and resubmit if it returns errors.\n",
    );
    out
}

fn tool_first_job_label(input: &ContextInput<'_>) -> &'static str {
    match input.context_profile {
        ContextProfile::Conversation => "Agent chat reply",
        ContextProfile::KnowledgeCompaction => "Knowledge compaction",
        ContextProfile::ConnectorCheck => "Connection check",
        ContextProfile::HumanChat => "Human chat reply",
        ContextProfile::HumanAgent => "Human-requested work",
        ContextProfile::Full => match full_context_kind(input) {
            FullContextKind::Work => "Work on ticket",
            FullContextKind::Consultation(_) => "Consultation (respond to mention)",
            FullContextKind::Other => "Ticket follow-up",
        },
    }
}

fn write_tool_first_task(out: &mut String, input: &ContextInput<'_>) {
    out.push_str(&format!("# Task\n\n**Job:** {}\n\n", tool_first_job_label(input)));

    if input.context_profile == ContextProfile::Conversation {
        out.push_str(
            "## Agent Chat\n\nYou are replying in a human-owned exploratory chat session. \
             Do not change files, tickets, workflow state, or knowledge. \
             Answer the latest human message directly and concisely.\n\n",
        );
        match (&input.human_request, input.latest_comments) {
            (Some(human), _) => out.push_str(&format!(
                "### Latest human message\n\n{}\n\n",
                human.body
            )),
            (None, transcript) => out.push_str(&format!(
                "### Conversation transcript\n\n{}\n\n",
                transcript.unwrap_or("(No previous messages)")
            )),
        }
        return;
    }

    if let Some(human) = &input.human_request {
        out.push_str("## Human request (read this first)\n\n");
        if human.posted_at.is_empty() {
            out.push_str(&format!("**Mode:** {}\n\n", human.mode_label));
        } else {
            out.push_str(&format!(
                "**Posted:** {} — **Mode:** {}\n\n",
                human.posted_at, human.mode_label
            ));
        }
        out.push_str(&format!(
            "> {}\n\nThis instruction overrides the ticket description when they conflict.\n\n",
            human.body
        ));
    }

    if input.context_profile == ContextProfile::Full {
        if let FullContextKind::Consultation(request) = full_context_kind(input) {
            out.push_str(&format!(
                "## Consultation request (answer this first)\n\n\
                 <consultation_request>\n{request}\n</consultation_request>\n\n\
                 - Answer the consultation request only; the ticket is background.\n\
                 - Do not implement product behavior. Do not edit, create, delete, or rewrite files.\n\
                 - Do not commit, stage, push, or merge.\n\
                 - Do not take assignment or hand off ownership; Coppice ignores `assignTo`, \
                 `updatedDescription`, `acceptanceCriteria`, `splitTickets`, and workflow changes here.\n\
                 - `mentionAgents` / `agentRequests` may notify agents but will not start another consultation.\n\n"
            ));
        }
    }

    if input.context_profile == ContextProfile::HumanChat {
        if let Some(excerpt) = input.thread_excerpt {
            out.push_str(&format!("## Recent thread\n\n{excerpt}\n\n"));
        }
    }

    out.push_str(&format!("## Ticket\n\n**Title:** {}\n", input.ticket_title));
    out.push_str(&format!("**Status:** {}\n", input.ticket_status));
    if let Some(substatus) = input.ticket_substatus {
        out.push_str(&format!("**Substatus:** {substatus}\n"));
    }
    out.push_str(&format!(
        "**Assignee:** {}\n",
        input.assignee_agent_key.unwrap_or("(unassigned)")
    ));
    if let Some(id) = input.ticket_id {
        out.push_str(&format!("**Ticket ID:** {id}\n"));
    }
    out.push('\n');

    if let Some(resume) = input.resume_context {
        out.push_str(&format!("## Previous attempt summary\n\n{resume}\n\n"));
    }
}

fn write_tool_first_repository(out: &mut String, input: &ContextInput<'_>) {
    let Some(repo) = input.repo_name else {
        if input.context_profile == ContextProfile::Conversation {
            if let Some(path) = input.worktree_path {
                out.push_str(&format!("# Repository\n\n**Working directory:** `{path}`\n\n"));
            }
        }
        return;
    };
    out.push_str(&format!(
        "# Repository\n\n**Name:** {repo}\n**Worktree path:** {}\n",
        input.worktree_path.unwrap_or("(not set)")
    ));
    if input.project_rules.is_some() {
        out.push_str("Project rules: read `AGENTS.md` in the worktree.\n");
    }
    let full = input.context_profile == ContextProfile::Full;
    let read_only = (full && matches!(full_context_kind(input), FullContextKind::Consultation(_)))
        || is_ready_tech_lead_task(input)
        || (full && is_in_qa_qc_task(input));
    if read_only {
        out.push_str("Read-only run: do not edit, stage, or commit files.\n");
    } else if input.context_profile != ContextProfile::HumanChat {
        out.push_str("Commit before finishing; Coppice syncs and auto-commits — see skill coppice-git.\n");
    }
    out.push('\n');
}

/// Minimal overlay for in-process chat → board ticket drafting (no transcript reply).
pub fn build_draft_ticket_context(input: &ContextInput<'_>) -> String {
    let transcript = input.latest_comments.unwrap_or("(No previous messages)");
    let repository = input
        .worktree_path
        .map(|path| format!("\n# Working directory\n\n`{path}`\n"))
        .unwrap_or_default();
    format!(
        r#"# Draft board ticket

You are drafting a board-ready ticket from a human-owned chat session.
Do not reply in chat. Do not create a ticket. Draft title and description only.
Coppice tools are not available here: return the JSON object below as your final reply and do not call `result_submit`.

# Agent

**Name:** {name}
**Role:** {role}

{system_prompt}
{repository}
# Conversation transcript

{transcript}

# Drafting rules

- Produce a concise, actionable title (not a pasted chat line).
- Write a structured markdown description with Context and Next steps when helpful.
- Do not invent acceptanceCriteria as a separate field.
- Do not write, edit, create, delete, rename, stage, commit, or push files.
- Do not change tickets, workflow state, or knowledge.

# Expected output contract

Return one JSON object:

```json
{{
  "status": "done",
  "summary": "<concise board ticket title>",
  "updatedDescription": "<markdown description with context and next steps>",
  "changedFiles": [],
  "testsRun": [],
  "blockers": []
}}
```

`summary` is the ticket title. `updatedDescription` is the ticket body.
"#,
        name = input.agent_name,
        role = input.agent_role,
        system_prompt = input.agent_system_prompt,
        repository = repository,
        transcript = transcript,
    )
}

enum FullContextKind<'a> {
    Work,
    Other,
    Consultation(&'a str),
}

fn full_context_kind<'a>(input: &'a ContextInput<'a>) -> FullContextKind<'a> {
    match input.thread_excerpt {
        None => FullContextKind::Work,
        Some("") => FullContextKind::Other,
        Some(request) => FullContextKind::Consultation(request),
    }
}

fn is_ready_tech_lead_task(input: &ContextInput) -> bool {
    matches!(full_context_kind(input), FullContextKind::Work)
        && is_ready_tech_lead_refinement(
            input.context_profile,
            "work_on_ticket",
            input.ticket_status,
            input.agent_key,
            input.agent_role,
        )
}

fn is_in_qa_qc_task(input: &ContextInput) -> bool {
    workflow::is_in_qa_qc_task(input.ticket_status, input.agent_key, input.agent_role)
}

/// Fixed context for a connector check run: identity line plus the two tool calls.
pub fn connector_check_context(agent_name: &str) -> String {
    format!(
        "# Agent\n\n**Name:** {agent_name}\n\n# Task\n\n**Job:** Connection check\n\n\
         This is a Coppice connection check. Call the `ticket_get` tool, then call \
         `result_submit` with outcome `done` and summary `connection ok`.\n"
    )
}

pub fn write_context_document(worktree: &Path, markdown: &str) -> std::io::Result<()> {
    let agent_dir = worktree.join(".agent");
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(agent_dir.join("context.md"), markdown)?;
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
        (ContextProfile::Full, None, None, None, None)
    }

    fn fixture_full_input() -> ContextInput<'static> {
        // Leaked so the fixture can be `'static`; tests only.
        let (context_profile, human_request, ticket_id, assignee_agent_key, thread_excerpt) =
            full_profile_defaults();
        let description: &'static str = Box::leak(
            format!("## Context\n\n{}", "Refine the polling retry behaviour. ".repeat(60))
                [..2048]
                .to_string()
                .into_boxed_str(),
        );
        let comments: &'static str = Box::leak(
            (0..10)
                .map(|i| {
                    format!(
                        "- **Human** (comment {i}): {}\n",
                        "Please keep the retry window configurable and add tests. ".repeat(2)
                    )
                })
                .collect::<String>()
                .into_boxed_str(),
        );
        let rules: &'static str = Box::leak(
            "Follow AGENTS.md conventions. ".repeat(40)[..1024]
                .to_string()
                .into_boxed_str(),
        );
        ContextInput {
            ticket_title: "Refine polling retry",
            ticket_description: description,
            ticket_status: "backlog",
            ticket_substatus: None,
            agent_name: "PM Agent",
            agent_key: "pm",
            agent_role: "PM",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "You are the product manager.",
            repo_name: Some("coppice"),
            repo_remote_url: Some("https://github.com/example/coppice"),
            repo_default_branch: Some("main"),
            worktree_path: Some("/data/worktrees/coppice/ticket-1"),
            latest_comments: Some(comments),
            project_rules: Some(rules),
            resume_context: None,
            context_profile,
            human_request,
            ticket_id,
            assignee_agent_key,
            thread_excerpt,
        }
    }

    /// Byte length of the legacy fat `Full` context for `fixture_full_input()`,
    /// measured once before the tool-first switch removed the builder.
    pub const LEGACY_FULL_CONTEXT_BYTES: usize = 9576;

    fn test_skills() -> Vec<SkillInfo> {
        [
            ("coppice-collaboration", "Mentions, agentRequests, assignTo and field roles for handing work between agents."),
            ("coppice-git", "Git rules for ticket worktrees: commit, do not push, verification commands."),
            ("coppice-pm-refinement", "How a PM refines a backlog ticket and hands it to the Tech Lead."),
            ("coppice-qc-verification", "Verification-only QA: gather evidence, report defects, never fix."),
            ("coppice-splitting", "Split a large ticket into children or continue a long task with a progress note."),
            ("coppice-tech-lead-review", "Ready-stage technical refinement and In Review code review verdicts."),
        ]
        .into_iter()
        .map(|(id, description)| SkillInfo {
            id: id.into(),
            description: description.into(),
            path: std::path::PathBuf::from(format!("/skills/{id}")),
        })
        .collect()
    }

    #[test]
    fn tool_first_full_context_is_at_least_half_smaller() {
        let slim = build_tool_first_context(&fixture_full_input(), &test_skills(), None);
        println!("slim={} legacy={LEGACY_FULL_CONTEXT_BYTES}", slim.len());
        assert!(
            slim.len() * 2 <= LEGACY_FULL_CONTEXT_BYTES,
            "slim context is {} bytes; legacy was {LEGACY_FULL_CONTEXT_BYTES}",
            slim.len()
        );
    }

    #[test]
    fn tool_first_context_lists_agent_skills_and_responsibilities() {
        let mut input = fixture_full_input();
        let md = build_tool_first_context(&input, &test_skills(), None);
        assert!(!md.contains("**Skills:**"));
        assert!(!md.contains("**Responsibilities:**"));

        let skills = ["Rust".to_string(), "SQL".to_string()];
        let responsibilities = ["Own the API".to_string()];
        input.agent_skills = &skills;
        input.agent_responsibilities = &responsibilities;
        let md = build_tool_first_context(&input, &test_skills(), None);
        let agent = &md[..md.find("# Task\n").expect("task")];
        assert!(agent.contains("**Skills:**\n- Rust\n- SQL\n"));
        assert!(agent.contains("**Responsibilities:**\n- Own the API\n"));
        assert!(agent.contains("You are the product manager."));
    }

    #[test]
    fn tool_first_context_lists_skills_and_required_skill() {
        let md = build_tool_first_context(
            &fixture_full_input(),
            &test_skills(),
            Some("coppice-qc-verification"),
        );
        assert!(md.contains("coppice-qc-verification — "));
        assert!(md.contains("Load `coppice-qc-verification` before starting."));
        let none = build_tool_first_context(&fixture_full_input(), &test_skills(), None);
        assert!(!none.contains("before starting."));
    }

    #[test]
    fn tool_first_context_has_no_json_contract() {
        let md = build_tool_first_context(&fixture_full_input(), &test_skills(), None);
        assert!(!md.contains("```json"));
        assert!(!md.contains(".agent/ticket.json"));
        assert!(!md.contains(".agent/comments.json"));
        assert!(!md.contains(".agent/runs.json"));
    }

    #[test]
    fn tool_first_context_mentions_result_submit() {
        let md = build_tool_first_context(&fixture_full_input(), &test_skills(), None);
        assert!(md.contains(
            "Finish by calling `result_submit` with your result; fix and resubmit if it returns errors."
        ));
        for line in [
            "Use `ticket_get` / `ticket_comments` / `ticket_runs` for ticket details.",
            "Use `knowledge_search` for approved project knowledge.",
            "Use `board_agents` to find agent keys for handoff.",
        ] {
            assert!(md.contains(line), "missing {line}");
        }
    }

    #[test]
    fn tool_first_sections_appear_in_order() {
        let md = build_tool_first_context(&fixture_full_input(), &test_skills(), None);
        let positions: Vec<usize> = [
            "# Agent\n",
            "# Task\n",
            "# Repository\n",
            "# Skills\n",
            "# Coppice tools\n",
        ]
        .iter()
        .map(|heading| md.find(heading).unwrap_or_else(|| panic!("missing {heading}")))
        .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{positions:?}");
        assert!(md.contains("Commit before finishing; Coppice syncs and auto-commits — see skill coppice-git."));
        // The bulky legacy inputs are fetched via tools, not embedded.
        assert!(!md.contains("Refine the polling retry behaviour."));
        assert!(!md.contains("Please keep the retry window configurable"));
        assert!(!md.contains("Follow AGENTS.md conventions."));
    }

    #[test]
    fn tool_first_consultation_keeps_exact_request_and_read_only_rules() {
        let mut input = fixture_full_input();
        input.thread_excerpt = Some("Verify the data assumptions.\nCheck the index.");
        let md = build_tool_first_context(&input, &test_skills(), None);
        assert!(md.contains(
            "<consultation_request>\nVerify the data assumptions.\nCheck the index.\n</consultation_request>"
        ));
        assert!(md.contains("Do not edit"));
        assert!(md.contains("Do not commit"));
        assert!(md.contains("Do not take assignment"));
        assert!(md.contains("Read-only run"));
        assert!(!md.contains("Commit before finishing"));
    }

    #[test]
    fn tool_first_ready_tech_lead_and_qc_runs_are_read_only() {
        let mut input = fixture_full_input();
        input.agent_key = "tech_lead";
        input.agent_role = "Technical Lead";
        input.ticket_status = "ready";
        let md = build_tool_first_context(&input, &test_skills(), None);
        assert!(md.contains("Read-only run"));
        assert!(!md.contains("Commit before finishing"));

        input.agent_key = "qc";
        input.agent_role = "QC";
        input.ticket_status = "in_qa";
        let md = build_tool_first_context(&input, &test_skills(), None);
        assert!(md.contains("Read-only run"));
    }

    #[test]
    fn tool_first_human_agent_puts_request_before_ticket_and_keeps_snapshot() {
        let ticket_id = Uuid::new_v4();
        let mut input = fixture_full_input();
        input.context_profile = ContextProfile::HumanAgent;
        input.ticket_substatus = Some("implementing");
        input.ticket_id = Some(ticket_id);
        input.assignee_agent_key = Some("frontend_engineer");
        input.human_request = Some(HumanRequest {
            body: "Please fix the retry logic in the poller.",
            posted_at: "2026-06-14T12:00:00Z",
            mode_label: "Agent",
        });
        let md = build_tool_first_context(&input, &test_skills(), Some("coppice-git"));
        let human = md.find("## Human request (read this first)").expect("human");
        let ticket = md.find("## Ticket\n").expect("ticket");
        assert!(human < ticket);
        assert!(md.contains("Please fix the retry logic in the poller."));
        assert!(md.contains("**Mode:** Agent"));
        assert!(md.contains("**Substatus:** implementing"));
        assert!(md.contains("**Assignee:** frontend_engineer"));
        assert!(md.contains(&format!("**Ticket ID:** {ticket_id}")));
        assert!(md.contains("Load `coppice-git` before starting."));
    }

    #[test]
    fn tool_first_human_chat_keeps_thread_excerpt() {
        let mut input = fixture_full_input();
        input.context_profile = ContextProfile::HumanChat;
        input.thread_excerpt = Some("- **Human:** Can you help?");
        input.human_request = Some(HumanRequest {
            body: "What is the current status?",
            posted_at: "2026-06-14T12:00:00Z",
            mode_label: "Chat",
        });
        let md = build_tool_first_context(&input, &test_skills(), None);
        assert!(md.contains("## Recent thread"));
        assert!(md.contains("Can you help?"));
        assert!(md.contains("What is the current status?"));
        assert!(!md.contains("Commit before finishing"));
    }

    #[test]
    fn tool_first_resume_section_carries_previous_attempt() {
        let mut input = fixture_full_input();
        input.resume_context = Some("**Prior blocker:** Need API shape.");
        let md = build_tool_first_context(&input, &test_skills(), None);
        assert!(md.contains("## Previous attempt summary"));
        assert!(md.contains("Need API shape."));
    }

    #[test]
    fn tool_first_chat_uses_transcript_or_latest_human_message() {
        let mut input = fixture_full_input();
        input.context_profile = ContextProfile::Conversation;
        input.repo_name = None;
        input.latest_comments = Some("- **Human:** First question");
        let full = build_tool_first_context(&input, &test_skills(), None);
        assert!(full.contains("### Conversation transcript"));
        assert!(full.contains("First question"));
        assert!(full.contains("Do not change files, tickets, workflow state, or knowledge."));
        assert!(full.contains("/data/worktrees/coppice/ticket-1"));

        input.human_request = Some(HumanRequest {
            body: "What is the second question?",
            posted_at: "",
            mode_label: "Chat",
        });
        let slim = build_tool_first_context(&input, &test_skills(), None);
        assert!(slim.contains("### Latest human message"));
        assert!(slim.contains("What is the second question?"));
        assert!(!slim.contains("# Conversation transcript"));
        assert!(!slim.contains("First question"));
    }

    #[test]
    fn connector_check_context_is_fixed() {
        let context = connector_check_context("Checker");
        assert!(context.contains("**Name:** Checker"));
        assert!(context.contains(
            "This is a Coppice connection check. Call the `ticket_get` tool, then call `result_submit` with outcome `done` and summary `connection ok`."
        ));
        assert!(!context.contains("skill_load"));
    }
}
