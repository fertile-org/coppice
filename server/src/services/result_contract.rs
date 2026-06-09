use crate::domain::comment::CommentIntent;
use crate::domain::run::RunStatus;
use crate::domain::substatus::{Substatus, TicketStatus};
use crate::providers::AgentRunResult;

pub struct ApplyTicketUpdate {
    pub status: Option<TicketStatus>,
    pub substatus: Option<Substatus>,
    pub substatus_metadata: Option<serde_json::Value>,
}

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
            ..
        } => Ok(ApplyResult {
                run_status: RunStatus::Succeeded,
                ticket: ApplyTicketUpdate {
                    status: None,
                    substatus: None,
                    substatus_metadata: None,
                },
                comment: ApplyComment {
                    body: build_done_comment_body(summary, changed_files, tests_run, blockers),
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
                },
                comment: ApplyComment {
                    body: summary.clone(),
                    intent: CommentIntent::Blocked,
                    mentions: mention_agents.clone(),
                },
            })
        }
    }
}

fn build_done_comment_body(
    summary: &str,
    changed_files: &[String],
    tests_run: &[String],
    blockers: &[String],
) -> String {
    let mut body = summary.to_string();
    if !changed_files.is_empty() {
        body.push_str("\n\n**Changed files:**\n");
        for file in changed_files {
            body.push_str(&format!("- {file}\n"));
        }
    }
    if !tests_run.is_empty() {
        body.push_str("\n**Tests run:**\n");
        for test in tests_run {
            body.push_str(&format!("- {test}\n"));
        }
    }
    if !blockers.is_empty() {
        body.push_str("\n**Blockers:**\n");
        for blocker in blockers {
            body.push_str(&format!("- {blocker}\n"));
        }
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
