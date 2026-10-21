use crate::domain::context_profile::ContextProfile;
use crate::domain::substatus::{Substatus, TicketStatus};
use crate::providers::AgentRunResult;
use serde_json::Value;
use std::collections::HashMap;
use time::OffsetDateTime;
use uuid::Uuid;

/// Planning run. It does not create a worktree or commit.
pub const JOB_TYPE_PLAN_TICKET: &str = "plan_ticket";
/// Read-only run that checks a split parent against its acceptance criteria.
pub const JOB_TYPE_VERIFY_PARENT: &str = "verify_parent";

/// Runs that only read: planning and verifying a split parent. They get
/// read-only tools and the CLI's plan mode where it has one.
pub fn is_read_only_job(job_type: &str) -> bool {
    job_type == JOB_TYPE_PLAN_TICKET || job_type == JOB_TYPE_VERIFY_PARENT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Succeeded,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct TransitionContext {
    pub ticket_id: Uuid,
    pub current_status: TicketStatus,
    pub assignee_agent_id: Option<Uuid>,
    pub agent_role: String,
    pub agent_key: String,
    pub job_type: String,
    pub run_outcome: RunOutcome,
    pub contract: AgentRunResult,
    pub board_agent_keys: Vec<String>,
    pub board_agent_ids: HashMap<String, Uuid>,
    pub board_implementer_keys: Vec<String>,
    pub auto_assign_enabled: bool,
    pub clarification_round: i32,
    pub context_profile: ContextProfile,
    /// When false, workflow must not enter Ready. Planning or Skip planning
    /// is what makes Ready legal.
    pub plan_allows_ready: bool,
    /// Enabled board agents, oldest first. Picks the In Review / In QA owner.
    pub board_agents: Vec<BoardAgentInfo>,
}

/// An enabled board agent as the workflow sees it. `key` resolves to `id`
/// in `board_agent_ids`.
#[derive(Debug, Clone)]
pub struct BoardAgentInfo {
    pub key: String,
    pub id: Uuid,
    pub role: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitTicketSpec {
    pub title: String,
    pub description: String,
    #[serde(default, rename = "acceptanceCriteria")]
    pub acceptance_criteria: Option<String>,
    #[serde(default, rename = "assignTo")]
    pub assign_to: Option<String>,
    /// Short plan steps for the child, when proposed at Plan Review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSplitRecommendation {
    pub recommended_by_agent_id: Uuid,
    pub recommended_at: String,
    pub splits: Vec<SplitTicketSpec>,
    /// Proposed by a planning run: created only by approving that plan.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub from_plan: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRecommendation {
    pub recommended_agent_key: String,
    pub recommended_by_agent_id: Uuid,
    pub recommended_at: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobRequest {
    pub job_type: String,
    pub agent_id: Uuid,
    pub resume_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default)]
pub struct TransitionAction {
    pub new_status: Option<TicketStatus>,
    pub new_assignee_id: Option<Option<Uuid>>,
    pub pending_recommendation: Option<Option<PendingRecommendation>>,
    pub substatus: Option<Option<Substatus>>,
    pub substatus_metadata: Option<Option<Value>>,
    pub enqueue_jobs: Vec<JobRequest>,
    pub increment_clarification_round: bool,
    /// System comments explaining workflow issues (unknown assignee, etc.).
    pub system_comments: Vec<String>,
}

pub fn is_ready_tech_lead_refinement(
    context_profile: ContextProfile,
    job_type: &str,
    ticket_status: &str,
    agent_key: &str,
    agent_role: &str,
) -> bool {
    context_profile == ContextProfile::Full
        && job_type == "work_on_ticket"
        && ticket_status.eq_ignore_ascii_case("ready")
        && is_tech_lead_identity(agent_key, agent_role)
}

pub fn is_tech_lead_identity(agent_key: &str, agent_role: &str) -> bool {
    let role = agent_role.to_ascii_lowercase();
    agent_key.eq_ignore_ascii_case("tech_lead")
        || role.contains("tech lead")
        || role.contains("technical lead")
}

pub fn is_pm_identity(agent_key: &str, agent_role: &str) -> bool {
    if agent_key.eq_ignore_ascii_case("pm") {
        return true;
    }
    let role = agent_role.to_ascii_lowercase();
    role == "pm" || role.contains("product manager")
}

pub fn is_reviewer_identity(agent_key: &str, agent_role: &str) -> bool {
    if agent_key.eq_ignore_ascii_case("reviewer") {
        return true;
    }
    agent_role.to_ascii_lowercase().contains("review")
}

pub fn is_in_review_review_task(ticket_status: &str, agent_key: &str, agent_role: &str) -> bool {
    ticket_status.eq_ignore_ascii_case("in_review")
        && (is_tech_lead_identity(agent_key, agent_role)
            || is_reviewer_identity(agent_key, agent_role))
}

pub fn is_qc(role: &str) -> bool {
    let r = role.to_lowercase();
    r == "qc" || r.contains("quality")
}

pub fn is_qc_identity(agent_key: &str, agent_role: &str) -> bool {
    agent_key.eq_ignore_ascii_case("qc") || is_qc(agent_role)
}

/// Statuses whose owner the workflow picks when a ticket enters them.
pub fn is_owned_stage(status: TicketStatus) -> bool {
    matches!(status, TicketStatus::InReview | TicketStatus::InQa)
}

/// Whether an agent may own a ticket in `status`: a Tech Lead or reviewer for
/// In Review, QC for In QA.
pub fn is_stage_eligible(status: TicketStatus, agent_key: &str, agent_role: &str) -> bool {
    match status {
        TicketStatus::InReview => {
            !is_qc_identity(agent_key, agent_role)
                && (is_tech_lead_identity(agent_key, agent_role)
                    || is_reviewer_identity(agent_key, agent_role))
        }
        TicketStatus::InQa => is_qc_identity(agent_key, agent_role),
        _ => false,
    }
}

/// The oldest eligible board agent for `status`, other than `exclude`.
pub fn stage_owner_for(
    status: TicketStatus,
    agents: &[BoardAgentInfo],
    exclude: Option<Uuid>,
) -> Option<&BoardAgentInfo> {
    agents
        .iter()
        .filter(|agent| Some(agent.id) != exclude)
        .filter(|agent| is_stage_eligible(status, &agent.key, &agent.role))
        .min_by_key(|agent| agent.created_at)
}

/// The role a board needs to own `status`, for notes.
pub fn stage_owner_label(status: TicketStatus) -> &'static str {
    match status {
        TicketStatus::InQa => "QC",
        _ => "Tech Lead or reviewer",
    }
}

pub fn is_in_qa_qc_task(ticket_status: &str, agent_key: &str, agent_role: &str) -> bool {
    if !ticket_status.eq_ignore_ascii_case("in_qa") {
        return false;
    }
    if agent_key.eq_ignore_ascii_case("qc") {
        return true;
    }
    agent_role.to_ascii_lowercase().contains("quality")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_agent(id: u128, key: &str, role: &str, created_secs: i64) -> BoardAgentInfo {
        BoardAgentInfo {
            key: key.into(),
            id: Uuid::from_u128(id),
            role: role.into(),
            created_at: OffsetDateTime::from_unix_timestamp(created_secs).unwrap(),
        }
    }

    #[test]
    fn stage_eligibility_by_preset_key_then_role() {
        use TicketStatus::*;
        assert!(is_stage_eligible(InReview, "tech_lead", "Lead Engineer"));
        assert!(is_stage_eligible(InReview, "lead", "Technical Lead"));
        assert!(is_stage_eligible(InReview, "reviewer", "Reviewer"));
        assert!(is_stage_eligible(
            InReview,
            "code-reviewer",
            "Code Reviewer"
        ));
        assert!(!is_stage_eligible(InReview, "qc", "QA Review"));
        assert!(!is_stage_eligible(
            InReview,
            "backend_engineer",
            "Backend Engineer"
        ));
        assert!(!is_stage_eligible(InReview, "pm", "PM"));

        assert!(is_stage_eligible(InQa, "qc", "Tester"));
        assert!(is_stage_eligible(InQa, "tester", "QC"));
        assert!(is_stage_eligible(InQa, "qa", "Quality Engineer"));
        assert!(!is_stage_eligible(InQa, "tech_lead", "Technical Lead"));

        assert!(!is_stage_eligible(Ready, "tech_lead", "Technical Lead"));
    }

    #[test]
    fn stage_owner_is_oldest_eligible_other_than_excluded() {
        let agents = vec![
            board_agent(1, "backend_engineer", "Backend Engineer", 10),
            board_agent(2, "reviewer", "Reviewer", 30),
            board_agent(3, "tech_lead", "Technical Lead", 20),
            board_agent(4, "qc", "QC", 40),
        ];
        let owner = stage_owner_for(TicketStatus::InReview, &agents, None).unwrap();
        assert_eq!(owner.id, Uuid::from_u128(3));
        let owner =
            stage_owner_for(TicketStatus::InReview, &agents, Some(Uuid::from_u128(3))).unwrap();
        assert_eq!(owner.id, Uuid::from_u128(2));
        let owner = stage_owner_for(TicketStatus::InQa, &agents, None).unwrap();
        assert_eq!(owner.id, Uuid::from_u128(4));
        assert!(stage_owner_for(TicketStatus::InQa, &agents[..3], None).is_none());
    }

    #[test]
    fn ready_tech_lead_refinement_requires_full_work_context() {
        assert!(is_ready_tech_lead_refinement(
            ContextProfile::Full,
            "work_on_ticket",
            "ready",
            "tech_lead",
            "Lead Engineer",
        ));
        assert!(!is_ready_tech_lead_refinement(
            ContextProfile::HumanAgent,
            "work_on_ticket",
            "ready",
            "tech_lead",
            "Technical Lead",
        ));
        assert!(!is_ready_tech_lead_refinement(
            ContextProfile::Full,
            "respond_to_mention",
            "ready",
            "tech_lead",
            "Technical Lead",
        ));
    }
}
