use crate::domain::context_profile::ContextProfile;
use crate::domain::substatus::{Substatus, TicketStatus};
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::{
    is_ready_tech_lead_refinement as matches_ready_tech_lead_refinement, JobRequest,
    PendingRecommendation, RunOutcome, TransitionAction, TransitionContext,
};
use crate::providers::AgentRunResult;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const MAX_CLARIFICATION_ROUNDS: i32 = 3;
pub const MAX_MENTIONS_PER_RUN: u32 = 2;

pub struct WorkflowService;

impl WorkflowService {
    pub fn is_legal_transition(from: TicketStatus, to: TicketStatus) -> bool {
        use TicketStatus::*;
        matches!(
            (from, to),
            (Backlog, Ready)
                | (Backlog, InProgress)
                | (Backlog, InReview)
                | (Backlog, Blocked)
                | (Ready, InProgress)
                | (Ready, InReview)
                | (Ready, Blocked)
                | (InProgress, InReview)
                | (InProgress, Blocked)
                | (InReview, InQa)
                | (InReview, WaitForFinalReview)
                | (InReview, Blocked)
                | (InQa, WaitForFinalReview)
                | (InQa, Blocked)
                | (Blocked, Ready)
                | (Blocked, InProgress)
                | (Blocked, Backlog)
                | (WaitForFinalReview, Done)
        )
    }

    pub fn resolve_transition(ctx: TransitionContext) -> Result<TransitionAction, String> {
        if ctx.job_type == "respond_to_mention"
            && (ctx.context_profile == crate::domain::context_profile::ContextProfile::Full
                || ctx.run_outcome == RunOutcome::Succeeded)
        {
            return Ok(TransitionAction::default());
        }

        let mut action = TransitionAction::default();

        if ctx.run_outcome == RunOutcome::Blocked {
            if let Some(handoff) = resolve_verification_handoff(&ctx) {
                apply_assign_to(&mut action, &ctx, &handoff.agent_key);
                action.new_status = Some(TicketStatus::InProgress);
                action.enqueue_jobs.push(handoff.job);
                return Ok(action);
            }

            let mention_agents = mention_agents_from_contract(&ctx.contract)
                .into_iter()
                .take(MAX_MENTIONS_PER_RUN as usize)
                .collect::<Vec<_>>();
            if !mention_agents.is_empty() {
                let first_key = mention_agents[0].clone();
                let metadata = if let Some(id) = ctx.project_agent_ids.get(&first_key) {
                    serde_json::json!({ "agentId": id, "agentKey": first_key })
                } else {
                    serde_json::json!({ "agentKey": first_key })
                };
                action.substatus = Some(Some(Substatus::WaitingForAgent));
                action.substatus_metadata = Some(Some(metadata));
                for key in mention_agents {
                    if let Some(&agent_id) = ctx.project_agent_ids.get(&key) {
                        action.enqueue_jobs.push(JobRequest {
                            job_type: "respond_to_mention".into(),
                            agent_id,
                            resume_agent_id: ctx.assignee_agent_id,
                        });
                    }
                }
            }
            return Ok(action);
        }

        if ctx.run_outcome != RunOutcome::Succeeded || ctx.job_type != "work_on_ticket" {
            return Ok(action);
        }

        if matches!(ctx.contract, AgentRunResult::Continued { .. }) {
            return Ok(action);
        }

        if is_ready_tech_lead_refinement(&ctx) {
            return Ok(resolve_ready_tech_lead_handoff(&ctx));
        }

        if let Some(handoff) = resolve_verification_handoff(&ctx) {
            apply_assign_to(&mut action, &ctx, &handoff.agent_key);
            action.new_status = Some(TicketStatus::InProgress);
            action.enqueue_jobs.push(handoff.job);
            return Ok(action);
        }

        // Scope B smoke: after PM clarification, implementer completion skips QA → final review.
        if ctx.current_status == TicketStatus::InProgress
            && is_implementer(&ctx.agent_role)
            && ctx.clarification_round > 0
        {
            action.new_status = Some(TicketStatus::WaitForFinalReview);
            action.new_assignee_id = Some(None);
            return Ok(action);
        }

        if let Some(assign_key) = assign_to_from_contract(&ctx.contract) {
            let key_known = ctx.project_agent_keys.iter().any(|k| k == &assign_key);
            if ctx.auto_assign_enabled && !key_known {
                // PM backlog refinement must assign a real project agent; implementer
                // completion should still advance via the succeeded gate when assignTo
                // is wrong or names an agent not on the project (e.g. frontend_engineer).
                if unknown_assign_to_blocks_ticket(ctx.current_status, &ctx.agent_role) {
                    action.new_status = Some(TicketStatus::Blocked);
                    action.system_comments.push(unknown_assign_to_notice(
                        &assign_key,
                        &ctx.project_agent_keys,
                        true,
                    ));
                    return Ok(action);
                }
                action.system_comments.push(unknown_assign_to_notice(
                    &assign_key,
                    &ctx.project_agent_keys,
                    false,
                ));
            } else if key_known || !ctx.auto_assign_enabled {
                apply_assign_to(&mut action, &ctx, &assign_key);
            }
        }

        if let Some(next) = resolve_succeeded_gate(ctx.current_status, &ctx.agent_role) {
            if !Self::is_legal_transition(ctx.current_status, next) {
                return Err(format!(
                    "illegal transition {:?} -> {:?}",
                    ctx.current_status, next
                ));
            }
            action.new_status = Some(next);
            if next == TicketStatus::WaitForFinalReview {
                action.new_assignee_id = Some(None);
            }
        }

        Ok(action)
    }

    pub fn resolve_run_start_transition(
        current: TicketStatus,
        agent_key: &str,
        agent_role: &str,
        job_type: &str,
        context_profile: ContextProfile,
    ) -> Option<TicketStatus> {
        if context_profile == ContextProfile::HumanAgent || job_type != "work_on_ticket" {
            return None;
        }
        if matches_ready_tech_lead_refinement(
            context_profile,
            job_type,
            status_to_str(current),
            agent_key,
            agent_role,
        ) {
            return None;
        }
        match (current, agent_role) {
            (TicketStatus::Backlog, role) if is_implementer(role) => Some(TicketStatus::InProgress),
            (TicketStatus::Ready, role) if is_implementer(role) => Some(TicketStatus::InProgress),
            _ => None,
        }
    }

    pub fn final_approve(current: TicketStatus) -> Result<TicketStatus, &'static str> {
        if current == TicketStatus::WaitForFinalReview {
            Ok(TicketStatus::Done)
        } else {
            Err("final approve requires wait_for_final_review")
        }
    }
}

pub fn is_implementer(role: &str) -> bool {
    let r = role.to_lowercase();
    r.contains("engineer") || r.contains("research")
}

fn is_pm(role: &str) -> bool {
    let r = role.to_lowercase();
    r == "pm" || r.contains("product manager")
}

fn is_reviewer(role: &str) -> bool {
    role.to_lowercase().contains("review")
}

fn is_tech_lead(role: &str) -> bool {
    let r = role.to_lowercase();
    r.contains("tech lead") || r.contains("technical lead")
}

fn is_qc(role: &str) -> bool {
    let r = role.to_lowercase();
    r == "qc" || r.contains("quality")
}

fn is_verification_role(role: &str) -> bool {
    is_reviewer(role) || is_tech_lead(role) || is_qc(role)
}

struct VerificationHandoff {
    agent_key: String,
    job: JobRequest,
}

fn resolve_verification_handoff(ctx: &TransitionContext) -> Option<VerificationHandoff> {
    if ctx.job_type != "work_on_ticket"
        || !is_verification_role(&ctx.agent_role)
        || is_ready_tech_lead_refinement(ctx)
    {
        return None;
    }

    let send_back = match ctx.run_outcome {
        RunOutcome::Blocked => true,
        RunOutcome::Succeeded => match &ctx.contract {
            AgentRunResult::Done { blockers, .. } => !blockers.is_empty(),
            _ => false,
        },
    };
    if !send_back {
        return None;
    }

    let mention_agents = mention_agents_from_contract(&ctx.contract)
        .into_iter()
        .take(MAX_MENTIONS_PER_RUN as usize)
        .collect::<Vec<_>>();
    let agent_key = mention_agents
        .into_iter()
        .find(|key| ctx.project_agent_ids.contains_key(key))?;
    let agent_id = *ctx.project_agent_ids.get(&agent_key)?;

    Some(VerificationHandoff {
        agent_key,
        job: JobRequest {
            job_type: "work_on_ticket".into(),
            agent_id,
            resume_agent_id: None,
        },
    })
}

fn is_ready_tech_lead_refinement(ctx: &TransitionContext) -> bool {
    matches_ready_tech_lead_refinement(
        ctx.context_profile,
        &ctx.job_type,
        status_to_str(ctx.current_status),
        &ctx.agent_key,
        &ctx.agent_role,
    )
}

fn resolve_ready_tech_lead_handoff(ctx: &TransitionContext) -> TransitionAction {
    let mut action = TransitionAction::default();
    let Some(assign_key) = assign_to_from_contract(&ctx.contract) else {
        action.system_comments.push(ready_handoff_notice(
            None,
            None,
            &ctx.project_implementer_keys,
        ));
        return action;
    };
    let assign_key = assign_key.trim();
    if assign_key.is_empty() {
        action.system_comments.push(ready_handoff_notice(
            None,
            None,
            &ctx.project_implementer_keys,
        ));
        return action;
    }

    if !ctx.project_agent_keys.iter().any(|key| key == assign_key) {
        action.system_comments.push(ready_handoff_notice(
            Some(assign_key),
            Some("unknown or disabled"),
            &ctx.project_implementer_keys,
        ));
        return action;
    }

    if !ctx
        .project_implementer_keys
        .iter()
        .any(|key| key == assign_key)
    {
        action.system_comments.push(ready_handoff_notice(
            Some(assign_key),
            Some("not an implementer"),
            &ctx.project_implementer_keys,
        ));
        return action;
    }

    if ctx
        .project_agent_ids
        .get(assign_key)
        .is_some_and(|target_id| Some(*target_id) == ctx.assignee_agent_id)
    {
        action.system_comments.push(ready_handoff_notice(
            Some(assign_key),
            Some("the current Tech Lead cannot hand off to itself"),
            &ctx.project_implementer_keys,
        ));
        return action;
    }

    apply_assign_to(&mut action, ctx, assign_key);
    action
}

fn ready_handoff_notice(
    assign_key: Option<&str>,
    reason: Option<&str>,
    implementer_keys: &[String],
) -> String {
    let available = if implementer_keys.is_empty() {
        "(none — add or enable an implementer first)".to_string()
    } else {
        implementer_keys
            .iter()
            .map(|key| format!("`{key}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };

    match (assign_key, reason) {
        (None, _) => format!(
            "Technical refinement handoff is incomplete: the Tech Lead did not return `assignTo`. The ticket remains in Ready and no implementation run was started. Re-run technical refinement and select an enabled implementer. Available implementer keys: {available}."
        ),
        (Some(key), Some(reason)) => format!(
            "Technical refinement handoff is incomplete: `assignTo` target `{key}` is {reason}. The ticket remains in Ready and no implementation run was started. Re-run technical refinement and select an enabled implementer. Available implementer keys: {available}."
        ),
        (Some(_), None) => unreachable!("invalid Ready handoff notice reason"),
    }
}

/// Unknown `assignTo` blocks only when PM refinement must pick a valid next assignee.
fn unknown_assign_to_blocks_ticket(current: TicketStatus, role: &str) -> bool {
    current == TicketStatus::Backlog && is_pm(role)
}

fn unknown_assign_to_notice(assign_key: &str, known_keys: &[String], blocked: bool) -> String {
    let available = if known_keys.is_empty() {
        "(none)".to_string()
    } else {
        known_keys
            .iter()
            .map(|key| format!("`{key}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };

    if blocked {
        format!(
            "Workflow blocked this ticket: the agent recommended assignee `{assign_key}`, which is not available on this project. Add or enable the agent, or re-run with a valid agent key. Available keys: {available}."
        )
    } else {
        format!(
            "Workflow note: the agent returned assignTo `{assign_key}`, which is not on this project — ignored; status was advanced by workflow gates. Available keys: {available}."
        )
    }
}

fn resolve_succeeded_gate(current: TicketStatus, role: &str) -> Option<TicketStatus> {
    use TicketStatus::*;
    match (current, role) {
        (Backlog, role) if is_pm(role) => Some(Ready),
        (Backlog, role) if is_implementer(role) => Some(InReview),
        (Ready, role) if is_implementer(role) => Some(InReview),
        (InProgress, role) if is_implementer(role) => Some(InReview),
        (InReview, role) if is_implementer(role) => Some(WaitForFinalReview),
        (InReview, role) if is_reviewer(role) || is_tech_lead(role) => Some(InQa),
        (InQa, role) if is_qc(role) => Some(WaitForFinalReview),
        _ => None,
    }
}

fn assign_to_from_contract(contract: &AgentRunResult) -> Option<String> {
    match contract {
        AgentRunResult::Done { assign_to, .. } | AgentRunResult::Blocked { assign_to, .. } => {
            assign_to.clone()
        }
        AgentRunResult::Continued { .. } => None,
    }
}

fn mention_agents_from_contract(contract: &AgentRunResult) -> Vec<String> {
    match contract {
        AgentRunResult::Done { mention_agents, .. }
        | AgentRunResult::Blocked { mention_agents, .. } => mention_agents.clone(),
        AgentRunResult::Continued { .. } => vec![],
    }
}

fn summary_from_contract(contract: &AgentRunResult) -> Option<String> {
    match contract {
        AgentRunResult::Done { summary, .. }
        | AgentRunResult::Blocked { summary, .. }
        | AgentRunResult::Continued { summary, .. } => {
            if summary.is_empty() {
                None
            } else {
                Some(summary.clone())
            }
        }
    }
}

fn apply_assign_to(action: &mut TransitionAction, ctx: &TransitionContext, key: &str) {
    if ctx.auto_assign_enabled {
        if let Some(&id) = ctx.project_agent_ids.get(key) {
            action.new_assignee_id = Some(Some(id));
            action.pending_recommendation = Some(None);
        }
    } else {
        let Some(recommended_by) = ctx.assignee_agent_id else {
            return;
        };
        let recommended_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::new());
        action.pending_recommendation = Some(Some(PendingRecommendation {
            recommended_agent_key: key.to_string(),
            recommended_by_agent_id: recommended_by,
            recommended_at,
            summary: summary_from_contract(&ctx.contract),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::context_profile::ContextProfile;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn pm_agent_id() -> Uuid {
        Uuid::from_u128(0x100)
    }

    fn engineer_agent_id() -> Uuid {
        Uuid::from_u128(0x200)
    }

    fn tech_lead_agent_id() -> Uuid {
        Uuid::from_u128(0x300)
    }

    fn minimal_ctx() -> TransitionContext {
        TransitionContext {
            ticket_id: Uuid::from_u128(1),
            current_status: TicketStatus::Backlog,
            assignee_agent_id: Some(pm_agent_id()),
            agent_role: "PM".into(),
            agent_key: "pm".into(),
            job_type: "work_on_ticket".into(),
            run_outcome: RunOutcome::Succeeded,
            contract: AgentRunResult::Done {
                summary: String::new(),
                changed_files: vec![],
                tests_run: vec![],
                next_status: None,
                assign_to: None,
                updated_description: None,
                acceptance_criteria: None,
                mention_agents: vec![],
                agent_requests: vec![],
                blockers: vec![],
                split_tickets: vec![],
            },
            project_agent_keys: vec!["pm".into()],
            project_agent_ids: HashMap::from([("pm".into(), pm_agent_id())]),
            project_implementer_keys: vec![],
            auto_assign_enabled: true,
            clarification_round: 0,
            context_profile: ContextProfile::Full,
        }
    }

    fn done_with_assign_to(key: &str) -> AgentRunResult {
        AgentRunResult::Done {
            summary: "Enriched ticket".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: Some(key.into()),
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: vec![],
            agent_requests: vec![],
            blockers: vec![],
            split_tickets: vec![],
        }
    }

    fn blocked_with_mentions(keys: &[&str]) -> AgentRunResult {
        AgentRunResult::Blocked {
            blocker_type: "error".into(),
            summary: "Need clarification".into(),
            next_status: None,
            assign_to: None,
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: keys.iter().map(|k| (*k).into()).collect(),
            required_capabilities: vec![],
            required_secrets: vec![],
        }
    }

    fn agent_map(keys: &[(&str, Uuid)]) -> HashMap<String, Uuid> {
        keys.iter().map(|(k, id)| (k.to_string(), *id)).collect()
    }

    #[test]
    fn rejects_backlog_to_done() {
        assert!(!WorkflowService::is_legal_transition(
            TicketStatus::Backlog,
            TicketStatus::Done,
        ));
    }

    #[test]
    fn case1_pm_backlog_to_ready_with_pending_recommendation() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Backlog,
            agent_role: "PM".into(),
            agent_key: "pm".into(),
            job_type: "work_on_ticket".into(),
            run_outcome: RunOutcome::Succeeded,
            auto_assign_enabled: false,
            contract: done_with_assign_to("engineer"),
            project_agent_keys: vec!["pm".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("pm", pm_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::Ready));
        assert!(action.pending_recommendation.unwrap().is_some());
        assert!(action.new_assignee_id.is_none());
    }

    #[test]
    fn auto_assign_true_applies_assign_to_when_agent_exists() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Backlog,
            agent_role: "PM".into(),
            auto_assign_enabled: true,
            contract: done_with_assign_to("backend_engineer"),
            project_agent_keys: vec!["pm".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("pm", pm_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::Ready));
        assert_eq!(action.new_assignee_id, Some(Some(engineer_agent_id())));
        assert!(matches!(action.pending_recommendation, Some(None)));
    }

    #[test]
    fn ready_tech_lead_auto_assigns_enabled_implementer_without_advancing_status() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Ready,
            agent_role: "Technical Lead".into(),
            agent_key: "tech_lead".into(),
            assignee_agent_id: Some(tech_lead_agent_id()),
            auto_assign_enabled: true,
            contract: done_with_assign_to("backend_engineer"),
            project_agent_keys: vec!["tech_lead".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("tech_lead", tech_lead_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            project_implementer_keys: vec!["backend_engineer".into()],
            ..minimal_ctx()
        })
        .expect("resolve Ready Tech Lead handoff");

        assert!(action.new_status.is_none());
        assert_eq!(action.new_assignee_id, Some(Some(engineer_agent_id())));
        assert!(matches!(action.pending_recommendation, Some(None)));
        assert!(action.enqueue_jobs.is_empty());
        assert!(action.system_comments.is_empty());
    }

    #[test]
    fn ready_tech_lead_manual_policy_records_pending_implementer_recommendation() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Ready,
            agent_role: "Technical Lead".into(),
            agent_key: "tech_lead".into(),
            assignee_agent_id: Some(tech_lead_agent_id()),
            auto_assign_enabled: false,
            contract: done_with_assign_to("backend_engineer"),
            project_agent_keys: vec!["tech_lead".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("tech_lead", tech_lead_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            project_implementer_keys: vec!["backend_engineer".into()],
            ..minimal_ctx()
        })
        .expect("resolve manual Ready Tech Lead handoff");

        assert!(action.new_status.is_none());
        assert!(action.new_assignee_id.is_none());
        let pending = action
            .pending_recommendation
            .expect("pending recommendation action")
            .expect("pending recommendation");
        assert_eq!(pending.recommended_agent_key, "backend_engineer");
        assert!(action.enqueue_jobs.is_empty());
        assert!(action.system_comments.is_empty());
    }

    #[test]
    fn ready_tech_lead_missing_assign_to_stays_ready_with_actionable_comment() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Ready,
            agent_role: "Technical Lead".into(),
            agent_key: "tech_lead".into(),
            assignee_agent_id: Some(tech_lead_agent_id()),
            project_agent_keys: vec!["tech_lead".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("tech_lead", tech_lead_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            project_implementer_keys: vec!["backend_engineer".into()],
            ..minimal_ctx()
        })
        .expect("resolve missing Ready Tech Lead handoff");

        assert!(action.new_status.is_none());
        assert!(action.new_assignee_id.is_none());
        assert!(action.pending_recommendation.is_none());
        assert!(action.enqueue_jobs.is_empty());
        assert_eq!(action.system_comments.len(), 1);
        assert!(action.system_comments[0].contains("did not return `assignTo`"));
        assert!(action.system_comments[0].contains("backend_engineer"));
        assert!(action.system_comments[0].contains("remains in Ready"));
    }

    #[test]
    fn ready_tech_lead_unknown_assign_to_under_manual_policy_starts_nobody() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Ready,
            agent_role: "Technical Lead".into(),
            agent_key: "tech_lead".into(),
            assignee_agent_id: Some(tech_lead_agent_id()),
            auto_assign_enabled: false,
            contract: done_with_assign_to("missing_engineer"),
            project_agent_keys: vec!["tech_lead".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("tech_lead", tech_lead_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            project_implementer_keys: vec!["backend_engineer".into()],
            ..minimal_ctx()
        })
        .expect("resolve unknown Ready Tech Lead handoff");

        assert!(action.new_status.is_none());
        assert!(action.new_assignee_id.is_none());
        assert!(action.pending_recommendation.is_none());
        assert!(action.enqueue_jobs.is_empty());
        assert_eq!(action.system_comments.len(), 1);
        assert!(action.system_comments[0].contains("missing_engineer"));
        assert!(action.system_comments[0].contains("unknown or disabled"));
    }

    #[test]
    fn ready_tech_lead_cannot_handoff_to_enabled_non_implementer() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Ready,
            agent_role: "Technical Lead".into(),
            agent_key: "tech_lead".into(),
            assignee_agent_id: Some(tech_lead_agent_id()),
            auto_assign_enabled: true,
            contract: done_with_assign_to("pm"),
            project_agent_keys: vec!["pm".into(), "tech_lead".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("pm", pm_agent_id()),
                ("tech_lead", tech_lead_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            project_implementer_keys: vec!["backend_engineer".into()],
            ..minimal_ctx()
        })
        .expect("resolve non-implementer Ready Tech Lead handoff");

        assert!(action.new_status.is_none());
        assert!(action.new_assignee_id.is_none());
        assert!(action.pending_recommendation.is_none());
        assert!(action.enqueue_jobs.is_empty());
        assert_eq!(action.system_comments.len(), 1);
        assert!(action.system_comments[0].contains("not an implementer"));
    }

    #[test]
    fn ready_tech_lead_cannot_handoff_to_its_own_implementer_alias() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Ready,
            agent_role: "Technical Lead Engineer".into(),
            agent_key: "tech_lead".into(),
            assignee_agent_id: Some(tech_lead_agent_id()),
            auto_assign_enabled: true,
            contract: done_with_assign_to("tech_lead"),
            project_agent_keys: vec!["tech_lead".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("tech_lead", tech_lead_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            // Defend even if a caller accidentally classifies this dual-role agent
            // as an implementer.
            project_implementer_keys: vec!["tech_lead".into(), "backend_engineer".into()],
            ..minimal_ctx()
        })
        .expect("resolve self-targeted Ready Tech Lead handoff");

        assert!(action.new_status.is_none());
        assert!(action.new_assignee_id.is_none());
        assert!(action.pending_recommendation.is_none());
        assert!(action.enqueue_jobs.is_empty());
        assert_eq!(action.system_comments.len(), 1);
        assert!(action.system_comments[0].contains("cannot hand off to itself"));
    }

    #[test]
    fn ready_tech_lead_blocked_pm_mention_uses_clarification_resume_path() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Ready,
            agent_role: "Technical Lead".into(),
            agent_key: "tech_lead".into(),
            assignee_agent_id: Some(tech_lead_agent_id()),
            run_outcome: RunOutcome::Blocked,
            contract: blocked_with_mentions(&["pm"]),
            project_agent_keys: vec!["pm".into(), "tech_lead".into()],
            project_agent_ids: agent_map(&[
                ("pm", pm_agent_id()),
                ("tech_lead", tech_lead_agent_id()),
            ]),
            ..minimal_ctx()
        })
        .expect("resolve Ready Tech Lead clarification");

        assert!(action.new_status.is_none());
        assert!(action.new_assignee_id.is_none());
        assert_eq!(action.substatus, Some(Some(Substatus::WaitingForAgent)));
        assert_eq!(action.enqueue_jobs.len(), 1);
        assert_eq!(action.enqueue_jobs[0].job_type, "respond_to_mention");
        assert_eq!(action.enqueue_jobs[0].agent_id, pm_agent_id());
        assert_eq!(
            action.enqueue_jobs[0].resume_agent_id,
            Some(tech_lead_agent_id())
        );
    }

    #[test]
    fn case4_missing_assign_to_agent_blocks() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            auto_assign_enabled: true,
            contract: done_with_assign_to("frontend_engineer"),
            project_agent_keys: vec!["pm".into()],
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::Blocked));
        assert_eq!(action.system_comments.len(), 1);
        assert!(action.system_comments[0].contains("frontend_engineer"));
        assert!(action.system_comments[0].contains("Workflow blocked"));
    }

    #[test]
    fn case2_engineer_backlog_succeeded_moves_to_in_review() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::Backlog,
            agent_role: "Backend Engineer".into(),
            agent_key: "backend_engineer".into(),
            assignee_agent_id: Some(engineer_agent_id()),
            project_agent_keys: vec!["backend_engineer".into()],
            project_agent_ids: agent_map(&[("backend_engineer", engineer_agent_id())]),
            contract: AgentRunResult::Done {
                summary: "Implemented".into(),
                changed_files: vec![],
                tests_run: vec![],
                next_status: None,
                assign_to: None,
                updated_description: None,
                acceptance_criteria: None,
                mention_agents: vec![],
                agent_requests: vec![],
                blockers: vec![],
                split_tickets: vec![],
            },
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::InReview));
    }

    #[test]
    fn continued_run_does_not_change_ticket_status() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InProgress,
            agent_role: "Backend Engineer".into(),
            agent_key: "backend_engineer".into(),
            assignee_agent_id: Some(engineer_agent_id()),
            project_agent_keys: vec!["backend_engineer".into()],
            project_agent_ids: agent_map(&[("backend_engineer", engineer_agent_id())]),
            contract: AgentRunResult::Continued {
                summary: "Checkpoint".into(),
                progress_note: Some("Partial work".into()),
                changed_files: vec![],
                tests_run: vec![],
                blockers: vec![],
            },
            ..minimal_ctx()
        })
        .expect("resolve");
        assert!(action.new_status.is_none());
        assert!(action.new_assignee_id.is_none());
        assert!(action.pending_recommendation.is_none());
    }

    #[test]
    fn in_progress_implementer_succeeded_moves_to_in_review() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InProgress,
            agent_role: "Backend Engineer".into(),
            agent_key: "backend_engineer".into(),
            assignee_agent_id: Some(engineer_agent_id()),
            project_agent_keys: vec!["backend_engineer".into()],
            project_agent_ids: agent_map(&[("backend_engineer", engineer_agent_id())]),
            contract: AgentRunResult::Done {
                summary: "Done".into(),
                changed_files: vec![],
                tests_run: vec![],
                next_status: None,
                assign_to: None,
                updated_description: None,
                acceptance_criteria: None,
                mention_agents: vec![],
                agent_requests: vec![],
                blockers: vec![],
                split_tickets: vec![],
            },
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::InReview));
    }

    #[test]
    fn implementer_unknown_assign_to_still_moves_to_in_review() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InProgress,
            agent_role: "Backend Engineer".into(),
            agent_key: "backend_engineer".into(),
            assignee_agent_id: Some(engineer_agent_id()),
            auto_assign_enabled: true,
            contract: done_with_assign_to("frontend_engineer"),
            project_agent_keys: vec!["backend_engineer".into()],
            project_agent_ids: agent_map(&[("backend_engineer", engineer_agent_id())]),
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::InReview));
        assert!(action.new_assignee_id.is_none());
        assert_eq!(action.system_comments.len(), 1);
        assert!(action.system_comments[0].contains("frontend_engineer"));
        assert!(action.system_comments[0].contains("ignored"));
    }

    #[test]
    fn post_clarification_implementer_done_skips_to_wait_for_final_review() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InProgress,
            agent_role: "Backend Engineer".into(),
            agent_key: "backend_engineer".into(),
            assignee_agent_id: Some(engineer_agent_id()),
            clarification_round: 1,
            contract: AgentRunResult::Done {
                summary: "Resume complete".into(),
                changed_files: vec![],
                tests_run: vec![],
                next_status: None,
                assign_to: None,
                updated_description: None,
                acceptance_criteria: None,
                mention_agents: vec![],
                agent_requests: vec![],
                blockers: vec![],
                split_tickets: vec![],
            },
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::WaitForFinalReview));
        assert_eq!(action.new_assignee_id, Some(None));
    }

    #[test]
    fn in_review_tech_lead_succeeded_moves_to_in_qa() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InReview,
            agent_role: "Technical Lead".into(),
            agent_key: "tech_lead".into(),
            contract: AgentRunResult::Done {
                summary: "## Verdict\n**Approved**".into(),
                changed_files: vec![],
                tests_run: vec![],
                next_status: None,
                assign_to: None,
                updated_description: None,
                acceptance_criteria: None,
                mention_agents: vec![],
                agent_requests: vec![],
                blockers: vec![],
                split_tickets: vec![],
            },
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::InQa));
    }

    #[test]
    fn scope_b_in_review_implementer_moves_to_wait_for_final_review_and_unassigns() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InReview,
            agent_role: "Backend Engineer".into(),
            agent_key: "backend_engineer".into(),
            assignee_agent_id: Some(engineer_agent_id()),
            project_agent_keys: vec!["backend_engineer".into()],
            project_agent_ids: agent_map(&[("backend_engineer", engineer_agent_id())]),
            contract: AgentRunResult::Done {
                summary: "Resume complete".into(),
                changed_files: vec![],
                tests_run: vec![],
                next_status: None,
                assign_to: None,
                updated_description: None,
                acceptance_criteria: None,
                mention_agents: vec![],
                agent_requests: vec![],
                blockers: vec![],
                split_tickets: vec![],
            },
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::WaitForFinalReview));
        assert_eq!(action.new_assignee_id, Some(None));
    }

    #[test]
    fn verification_done_with_mentions_and_blockers_returns_to_in_progress() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InQa,
            agent_role: "QC".into(),
            agent_key: "qc".into(),
            assignee_agent_id: Some(Uuid::from_u128(0x300)),
            run_outcome: RunOutcome::Succeeded,
            contract: AgentRunResult::Done {
                summary: "Defects found.".into(),
                changed_files: vec![],
                tests_run: vec![],
                next_status: None,
                assign_to: None,
                updated_description: None,
                acceptance_criteria: None,
                mention_agents: vec!["backend_engineer".into()],
                agent_requests: vec![],
                blockers: vec!["Missing tests".into()],
                split_tickets: vec![],
            },
            project_agent_keys: vec!["qc".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("qc", Uuid::from_u128(0x300)),
                ("backend_engineer", engineer_agent_id()),
            ]),
            auto_assign_enabled: true,
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::InProgress));
        assert_eq!(action.new_assignee_id, Some(Some(engineer_agent_id())));
        assert_eq!(action.enqueue_jobs.len(), 1);
        assert_eq!(action.enqueue_jobs[0].job_type, "work_on_ticket");
        assert_eq!(action.enqueue_jobs[0].agent_id, engineer_agent_id());
    }

    #[test]
    fn qc_pass_advances_in_qa_to_wait_for_final_review() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InQa,
            agent_role: "QC".into(),
            agent_key: "qc".into(),
            assignee_agent_id: Some(Uuid::from_u128(0x300)),
            run_outcome: RunOutcome::Succeeded,
            contract: AgentRunResult::Done {
                summary: "No defects — all checks pass.".into(),
                changed_files: vec![],
                tests_run: vec!["cargo test -p coppice-server --lib".into()],
                next_status: None,
                assign_to: None,
                updated_description: None,
                acceptance_criteria: None,
                mention_agents: vec![],
                agent_requests: vec![],
                blockers: vec![],
                split_tickets: vec![],
            },
            project_agent_keys: vec!["qc".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("qc", Uuid::from_u128(0x300)),
                ("backend_engineer", engineer_agent_id()),
            ]),
            auto_assign_enabled: true,
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::WaitForFinalReview));
        // Pass path unassigns and does not enqueue a fix run.
        assert_eq!(action.new_assignee_id, Some(None));
        assert!(action.enqueue_jobs.is_empty());
    }

    #[test]
    fn verification_blocked_with_mentions_returns_to_in_progress() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InReview,
            agent_role: "Technical Lead".into(),
            agent_key: "tech_lead".into(),
            assignee_agent_id: Some(Uuid::from_u128(0x400)),
            run_outcome: RunOutcome::Blocked,
            contract: blocked_with_mentions(&["backend_engineer"]),
            project_agent_keys: vec!["tech_lead".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("tech_lead", Uuid::from_u128(0x400)),
                ("backend_engineer", engineer_agent_id()),
            ]),
            auto_assign_enabled: true,
            ..minimal_ctx()
        })
        .expect("resolve");
        assert_eq!(action.new_status, Some(TicketStatus::InProgress));
        assert_eq!(action.new_assignee_id, Some(Some(engineer_agent_id())));
        assert_eq!(action.enqueue_jobs.len(), 1);
        assert_eq!(action.enqueue_jobs[0].job_type, "work_on_ticket");
    }

    #[test]
    fn respond_to_mention_does_not_change_status() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            job_type: "respond_to_mention".into(),
            run_outcome: RunOutcome::Succeeded,
            ..minimal_ctx()
        })
        .expect("resolve");
        assert!(action.new_status.is_none());
    }

    #[test]
    fn blocked_with_mention_agents_sets_waiting_substatus_and_enqueues_jobs() {
        let action = WorkflowService::resolve_transition(TransitionContext {
            current_status: TicketStatus::InProgress,
            agent_role: "Backend Engineer".into(),
            agent_key: "backend_engineer".into(),
            assignee_agent_id: Some(engineer_agent_id()),
            run_outcome: RunOutcome::Blocked,
            contract: blocked_with_mentions(&["pm"]),
            project_agent_keys: vec!["pm".into(), "backend_engineer".into()],
            project_agent_ids: agent_map(&[
                ("pm", pm_agent_id()),
                ("backend_engineer", engineer_agent_id()),
            ]),
            ..minimal_ctx()
        })
        .expect("resolve");
        assert!(action.new_status.is_none());
        assert_eq!(action.substatus, Some(Some(Substatus::WaitingForAgent)));
        assert_eq!(action.enqueue_jobs.len(), 1);
        assert_eq!(action.enqueue_jobs[0].job_type, "respond_to_mention");
        assert_eq!(action.enqueue_jobs[0].agent_id, pm_agent_id());
        assert_eq!(
            action.enqueue_jobs[0].resume_agent_id,
            Some(engineer_agent_id())
        );
    }

    #[test]
    fn resolve_run_start_backlog_engineer_to_in_progress() {
        assert_eq!(
            WorkflowService::resolve_run_start_transition(
                TicketStatus::Backlog,
                "backend_engineer",
                "Backend Engineer",
                "work_on_ticket",
                ContextProfile::Full,
            ),
            Some(TicketStatus::InProgress)
        );
    }

    #[test]
    fn resolve_run_start_ready_engineer_to_in_progress() {
        assert_eq!(
            WorkflowService::resolve_run_start_transition(
                TicketStatus::Ready,
                "research",
                "Researcher",
                "work_on_ticket",
                ContextProfile::Full,
            ),
            Some(TicketStatus::InProgress)
        );
    }

    #[test]
    fn ready_tech_lead_run_start_does_not_move_to_in_progress() {
        assert_eq!(
            WorkflowService::resolve_run_start_transition(
                TicketStatus::Ready,
                "tech_lead",
                "Technical Lead Engineer",
                "work_on_ticket",
                ContextProfile::Full,
            ),
            None
        );
    }

    #[test]
    fn ready_tech_lead_preset_run_start_ignores_edited_implementer_role() {
        assert_eq!(
            WorkflowService::resolve_run_start_transition(
                TicketStatus::Ready,
                "tech_lead",
                "Lead Engineer",
                "work_on_ticket",
                ContextProfile::Full,
            ),
            None
        );
    }

    #[test]
    fn ready_tech_lead_human_agent_run_uses_existing_no_transition_behavior() {
        assert_eq!(
            WorkflowService::resolve_run_start_transition(
                TicketStatus::Ready,
                "tech_lead",
                "Technical Lead Engineer",
                "work_on_ticket",
                ContextProfile::HumanAgent,
            ),
            None
        );
    }

    #[test]
    fn resolve_run_start_ignores_non_work_jobs() {
        assert_eq!(
            WorkflowService::resolve_run_start_transition(
                TicketStatus::Backlog,
                "backend_engineer",
                "Backend Engineer",
                "respond_to_mention",
                ContextProfile::Full,
            ),
            None
        );
    }

    #[test]
    fn final_approve_from_wait_for_final_review() {
        assert_eq!(
            WorkflowService::final_approve(TicketStatus::WaitForFinalReview),
            Ok(TicketStatus::Done)
        );
    }

    #[test]
    fn final_approve_rejects_other_status() {
        assert!(WorkflowService::final_approve(TicketStatus::InReview).is_err());
    }

    #[test]
    fn is_implementer_matches_engineer_and_research_roles() {
        assert!(is_implementer("Backend Engineer"));
        assert!(is_implementer("frontend engineer"));
        assert!(is_implementer("Researcher"));
        assert!(!is_implementer("PM"));
        assert!(!is_implementer("Reviewer"));
    }
}
