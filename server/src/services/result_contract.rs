use crate::domain::comment::CommentIntent;
use crate::domain::run::RunStatus;
use crate::domain::substatus::{Substatus, TicketStatus};
use crate::providers::AgentRunResult;

pub struct ApplyTicketUpdate {
    pub status: Option<TicketStatus>,
    pub substatus: Option<Substatus>,
    pub substatus_metadata: Option<serde_json::Value>,
    pub updated_description: Option<String>,
    pub acceptance_criteria: Option<String>,
}

pub const ACCEPTANCE_CRITERIA_HEADER: &str = "## Acceptance criteria";

pub struct ApplyComment {
    pub body: String,
    pub intent: CommentIntent,
    pub mentions: Vec<String>,
}

pub struct ApplyResult {
    pub run_status: RunStatus,
    pub ticket: ApplyTicketUpdate,
    pub comment: ApplyComment,
}

pub fn ticket_status_from_next_status(label: &str) -> Option<TicketStatus> {
    match label.trim() {
        "Backlog" | "backlog" => Some(TicketStatus::Backlog),
        "Ready" | "ready" => Some(TicketStatus::Ready),
        "In Progress" | "in_progress" => Some(TicketStatus::InProgress),
        "In Review" | "in_review" => Some(TicketStatus::InReview),
        "In QA" | "in_qa" => Some(TicketStatus::InQa),
        "Wait for Final Review" | "wait_for_final_review" => Some(TicketStatus::WaitForFinalReview),
        "Done" | "done" => Some(TicketStatus::Done),
        "Blocked" | "blocked" => Some(TicketStatus::Blocked),
        _ => None,
    }
}

pub fn apply_agent_result(result: &AgentRunResult) -> Result<ApplyResult, String> {
    match result {
        AgentRunResult::Done {
            summary,
            changed_files,
            tests_run,
            mention_agents,
            blockers,
            updated_description,
            acceptance_criteria,
            assign_to,
            ..
        } => Ok(ApplyResult {
                run_status: RunStatus::Succeeded,
                ticket: ApplyTicketUpdate {
                    status: None,
                    substatus: None,
                    substatus_metadata: None,
                    updated_description: non_empty_opt(updated_description.as_deref()),
                    acceptance_criteria: non_empty_opt(acceptance_criteria.as_deref()),
                },
                comment: ApplyComment {
                    body: build_done_comment_body(
                        summary,
                        changed_files,
                        tests_run,
                        blockers,
                        acceptance_criteria.as_deref(),
                        updated_description.as_deref(),
                        assign_to.as_deref(),
                    ),
                    intent: CommentIntent::ImplementationDone,
                    mentions: mention_agents.clone(),
                },
            }),
        AgentRunResult::Blocked {
            blocker_type,
            summary,
            mention_agents,
            required_capabilities,
            required_secrets,
            updated_description,
            acceptance_criteria,
            ..
        } => {
            let (substatus, substatus_metadata) = blocked_substatus(
                blocker_type,
                summary,
                required_capabilities,
                required_secrets,
            );
            Ok(ApplyResult {
                run_status: RunStatus::Blocked,
                ticket: ApplyTicketUpdate {
                    status: None,
                    substatus,
                    substatus_metadata,
                    updated_description: non_empty_opt(updated_description.as_deref()),
                    acceptance_criteria: non_empty_opt(acceptance_criteria.as_deref()),
                },
                comment: ApplyComment {
                    body: build_done_comment_body(
                        summary,
                        &[],
                        &[],
                        &[],
                        acceptance_criteria.as_deref(),
                        updated_description.as_deref(),
                        None,
                    ),
                    intent: CommentIntent::Blocked,
                    mentions: mention_agents.clone(),
                },
            })
        }
        AgentRunResult::Continued {
            summary,
            progress_note,
            changed_files,
            tests_run,
            blockers,
        } => Ok(ApplyResult {
            run_status: RunStatus::Succeeded,
            ticket: ApplyTicketUpdate {
                status: None,
                substatus: None,
                substatus_metadata: None,
                updated_description: None,
                acceptance_criteria: None,
            },
            comment: ApplyComment {
                body: build_continued_comment_body(
                    summary,
                    progress_note.as_deref(),
                    changed_files,
                    tests_run,
                    blockers,
                ),
                intent: CommentIntent::ProgressUpdate,
                mentions: vec![],
            },
        }),
    }
}

fn non_empty_opt(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Merge optional description and acceptance-criteria updates into ticket body text.
pub fn merge_ticket_description(
    current: &str,
    updated_description: Option<&str>,
    acceptance_criteria: Option<&str>,
) -> Option<String> {
    let base = updated_description
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| current.to_string());

    let merged = match acceptance_criteria.map(str::trim).filter(|s| !s.is_empty()) {
        Some(criteria) => upsert_markdown_section(
            &base,
            ACCEPTANCE_CRITERIA_HEADER,
            acceptance_criteria_content(criteria),
        ),
        None => base,
    };

    if merged.trim() == current.trim() {
        None
    } else {
        Some(normalize_agent_markdown(&merged))
    }
}

/// Light cleanup for agent-authored markdown before storing on tickets.
pub fn normalize_agent_markdown(text: &str) -> String {
    let mut out = text.replace("\r\n", "\n");
    out = out.replace(".**", ".\n\n**");
    if let Some(idx) = out.find("| # |") {
        let before = &out[..idx];
        if !before.ends_with('\n') {
            out = format!("{}\n\n{}", before.trim_end(), &out[idx..]);
        }
    }
    out.trim().to_string()
}

fn acceptance_criteria_content(criteria: &str) -> &str {
    criteria
        .trim()
        .strip_prefix(ACCEPTANCE_CRITERIA_HEADER)
        .map(|rest| rest.trim_start_matches('\n'))
        .unwrap_or_else(|| criteria.trim())
}

fn format_acceptance_criteria_section(criteria: &str) -> String {
    format!(
        "{ACCEPTANCE_CRITERIA_HEADER}\n\n{}",
        acceptance_criteria_content(criteria)
    )
}

fn upsert_markdown_section(body: &str, header: &str, content: &str) -> String {
    let section = format!("{header}\n\n{content}");
    if let Some(start) = body.find(header) {
        let after_header = start + header.len();
        let rest = &body[after_header..];
        let end = rest
            .find("\n## ")
            .map(|i| after_header + i)
            .unwrap_or(body.len());
        let mut out = String::new();
        out.push_str(body[..start].trim_end());
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&section);
        let tail = body[end..].trim_start();
        if !tail.is_empty() {
            out.push_str("\n\n");
            out.push_str(tail);
        }
        out
    } else if body.trim().is_empty() {
        section
    } else {
        format!("{}\n\n{section}", body.trim_end())
    }
}

fn build_done_comment_body(
    summary: &str,
    changed_files: &[String],
    tests_run: &[String],
    blockers: &[String],
    acceptance_criteria: Option<&str>,
    updated_description: Option<&str>,
    assign_to: Option<&str>,
) -> String {
    let description_updated = updated_description
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some();

    let mut body = if description_updated {
        concise_comment_for_description_update(summary, assign_to)
    } else {
        summary.trim().to_string()
    };

    if !description_updated {
        if let Some(criteria) = acceptance_criteria.map(str::trim).filter(|s| !s.is_empty()) {
            body.push_str("\n\n");
            body.push_str(&format_acceptance_criteria_section(criteria));
        }
    }
    if !changed_files.is_empty() {
        body.push_str("\n\n**Changed files:**\n");
        for file in changed_files {
            body.push_str(&format!("- {file}\n"));
        }
    }
    if !tests_run.is_empty() {
        body.push_str("\n\n**Tests run:**\n");
        for test in tests_run {
            body.push_str(&format!("- {test}\n"));
        }
    }
    if !blockers.is_empty() {
        body.push_str("\n\n**Blockers:**\n");
        for blocker in blockers {
            body.push_str(&format!("- {blocker}\n"));
        }
    }
    body
}

fn build_continued_comment_body(
    summary: &str,
    progress_note: Option<&str>,
    changed_files: &[String],
    tests_run: &[String],
    blockers: &[String],
) -> String {
    let mut body = build_done_comment_body(summary, &[], &[], &[], None, None, None);
    if let Some(note) = progress_note.map(str::trim).filter(|s| !s.is_empty()) {
        body.push_str("\n\n**Progress note:**\n");
        body.push_str(note);
    }
    if !changed_files.is_empty() {
        body.push_str("\n\n**Changed files:**\n");
        for file in changed_files {
            body.push_str(&format!("- {file}\n"));
        }
    }
    if !tests_run.is_empty() {
        body.push_str("\n\n**Tests run:**\n");
        for test in tests_run {
            body.push_str(&format!("- {test}\n"));
        }
    }
    if !blockers.is_empty() {
        body.push_str("\n\n**Blockers:**\n");
        for blocker in blockers {
            body.push_str(&format!("- {blocker}\n"));
        }
    }
    body
}

fn concise_comment_for_description_update(summary: &str, assign_to: Option<&str>) -> String {
    let trimmed = summary.trim();
    let looks_like_full_spec = trimmed.len() > 280
        || trimmed.contains("### ")
        || trimmed.contains("| # |")
        || trimmed.contains("\n## ");

    if !looks_like_full_spec {
        return trimmed.to_string();
    }

    let mut body = "Updated the ticket description and acceptance criteria.".to_string();
    if let Some(assignee) = assign_to.map(str::trim).filter(|s| !s.is_empty()) {
        body.push_str(&format!(" Recommends **{assignee}** for the next run."));
    }
    body
}

fn blocked_substatus(
    blocker_type: &str,
    summary: &str,
    required_capabilities: &[String],
    required_secrets: &[String],
) -> (Option<Substatus>, Option<serde_json::Value>) {
    match blocker_type {
        "missing_capability" => (
            Some(Substatus::BlockedByMissingCapability),
            required_capabilities.first().map(|capability| {
                serde_json::json!({ "capability": capability })
            }),
        ),
        "missing_secret" => (
            Some(Substatus::BlockedByMissingSecret),
            required_secrets
                .first()
                .map(|secret_key| serde_json::json!({ "secretKey": secret_key })),
        ),
        "permission" => (Some(Substatus::BlockedByPermission), None),
        "error" => (
            Some(Substatus::BlockedByError),
            Some(serde_json::json!({ "reason": summary })),
        ),
        _ => (
            Some(Substatus::BlockedByError),
            Some(serde_json::json!({ "reason": summary })),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::comment::intent_to_str;
    use crate::providers::fixtures_root;

    fn load_fixture(name: &str) -> AgentRunResult {
        let path = fixtures_root().join(name);
        let raw = std::fs::read_to_string(path).expect("read fixture");
        serde_json::from_str(&raw).expect("deserialize fixture")
    }

    #[test]
    fn done_fixture_maps_to_succeeded_in_review() {
        let result = load_fixture("done.json");
        let applied = apply_agent_result(&result).expect("apply done");
        assert_eq!(applied.run_status, RunStatus::Succeeded);
        assert_eq!(applied.ticket.status, None);
        assert_eq!(
            intent_to_str(applied.comment.intent),
            "implementation_done"
        );
    }

    #[test]
    fn apply_does_not_set_ticket_status_from_next_status() {
        let result = load_fixture("done.json");
        let applied = apply_agent_result(&result).expect("apply done");
        assert_eq!(applied.ticket.status, None);
    }

    #[test]
    fn blocked_fixture_maps_to_blocked_status() {
        let result = load_fixture("blocked.json");
        let applied = apply_agent_result(&result).expect("apply blocked");
        assert_eq!(applied.run_status, RunStatus::Blocked);
        assert_eq!(applied.ticket.status, None);
        assert_eq!(intent_to_str(applied.comment.intent), "blocked");
    }

    #[test]
    fn merge_replaces_description_and_appends_acceptance_criteria() {
        let merged = merge_ticket_description(
            "Old description",
            Some("New description body"),
            Some("- Criterion one\n- Criterion two"),
        )
        .expect("merged");
        assert!(merged.contains("New description body"));
        assert!(merged.contains("## Acceptance criteria"));
        assert!(merged.contains("- Criterion one"));
    }

    #[test]
    fn merge_appends_acceptance_criteria_to_existing_description() {
        let merged = merge_ticket_description(
            "Existing scope",
            None,
            Some("- Must pass CI"),
        )
        .expect("merged");
        assert!(merged.starts_with("Existing scope"));
        assert!(merged.contains("## Acceptance criteria"));
        assert!(merged.contains("- Must pass CI"));
    }

    #[test]
    fn merge_strips_duplicate_acceptance_criteria_header() {
        let merged = merge_ticket_description(
            "Scope",
            None,
            Some("## Acceptance criteria\n- [ ] Ship feature"),
        )
        .expect("merged");
        assert_eq!(merged.matches("## Acceptance criteria").count(), 1);
        assert!(merged.contains("- [ ] Ship feature"));
    }

    #[test]
    fn merge_returns_none_when_unchanged() {
        assert!(merge_ticket_description("Same", None, None).is_none());
    }

    #[test]
    fn done_comment_separates_tests_run_with_blank_line() {
        let result = AgentRunResult::Done {
            summary: "Approving.".into(),
            changed_files: vec![],
            tests_run: vec!["cargo test -p coppice-server --lib".into()],
            next_status: None,
            assign_to: None,
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: vec![],
            blockers: vec![],
            split_tickets: vec![],
        };
        let applied = apply_agent_result(&result).expect("apply");
        assert!(applied.comment.body.contains("Approving.\n\n**Tests run:**"));
    }

    #[test]
    fn done_comment_includes_acceptance_criteria_without_description_update() {
        let result = AgentRunResult::Done {
            summary: "Refined scope.".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: None,
            updated_description: None,
            acceptance_criteria: Some("- Must pass CI\n- Must include tests".into()),
            mention_agents: vec![],
            blockers: vec![],
            split_tickets: vec![],
        };
        let applied = apply_agent_result(&result).expect("apply");
        assert!(applied.comment.body.contains("Refined scope."));
        assert!(applied.comment.body.contains("## Acceptance criteria"));
        assert!(applied.comment.body.contains("- Must pass CI"));
    }

    #[test]
    fn done_comment_omits_duplicate_spec_when_description_updated() {
        let result = AgentRunResult::Done {
            summary: "## PM Refinement\n\n### Current state\nLong analysis with | # | Task | table".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: Some("backend_engineer".into()),
            updated_description: Some("## Scope\n\nBuild connector.".into()),
            acceptance_criteria: Some("- [ ] Ship feature".into()),
            mention_agents: vec![],
            blockers: vec![],
            split_tickets: vec![],
        };
        let applied = apply_agent_result(&result).expect("apply");
        assert!(applied.comment.body.contains("Updated the ticket description"));
        assert!(applied.comment.body.contains("backend_engineer"));
        assert!(!applied.comment.body.contains("| # |"));
        assert!(!applied.comment.body.contains("## Acceptance criteria"));
    }

    #[test]
    fn normalize_agent_markdown_breaks_glued_sections_and_tables() {
        let raw = "Intro line.**Scope** details here | # | Task | Effort |\n|---|";
        let normalized = normalize_agent_markdown(raw);
        assert!(normalized.contains(".\n\n**Scope**"));
        assert!(normalized.contains("\n\n| # |"));
    }

    #[test]
    fn apply_extracts_updated_description_fields() {
        let result = AgentRunResult::Done {
            summary: "Refined".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: None,
            updated_description: Some("Updated body".into()),
            acceptance_criteria: Some("- AC1".into()),
            mention_agents: vec![],
            blockers: vec![],
            split_tickets: vec![],
        };
        let applied = apply_agent_result(&result).expect("apply");
        assert_eq!(
            applied.ticket.updated_description.as_deref(),
            Some("Updated body")
        );
        assert_eq!(
            applied.ticket.acceptance_criteria.as_deref(),
            Some("- AC1")
        );
    }

    #[test]
    fn continued_fixture_maps_to_succeeded_progress() {
        let result = load_fixture("backend_engineer/continued.json");
        let applied = apply_agent_result(&result).expect("apply continued");
        assert_eq!(applied.run_status, RunStatus::Succeeded);
        assert_eq!(applied.ticket.status, None);
        assert_eq!(applied.ticket.substatus, None);
        assert_eq!(intent_to_str(applied.comment.intent), "progress_update");
        assert!(applied.comment.body.contains("Implemented TmuxStream create/kill"));
        assert!(applied.comment.body.contains("**Progress note:**"));
        assert!(applied
            .comment
            .body
            .contains("server/src/sessions/tmux_stream.rs"));
    }

    #[test]
    fn blocked_missing_capability_sets_substatus() {
        let result = load_fixture("blocked-missing-capability.json");
        let applied = apply_agent_result(&result).expect("apply blocked capability");
        assert_eq!(
            applied.ticket.substatus,
            Some(Substatus::BlockedByMissingCapability)
        );
        assert_eq!(
            applied.ticket.substatus_metadata,
            Some(serde_json::json!({ "capability": "postgres" }))
        );
    }
}
