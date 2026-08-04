use crate::domain::context_profile::ContextProfile;
use crate::domain::substatus::{Substatus, TicketStatus};
use crate::providers::AgentRunResult;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

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
    pub project_agent_keys: Vec<String>,
    pub project_agent_ids: HashMap<String, Uuid>,
    pub project_implementer_keys: Vec<String>,
    pub auto_assign_enabled: bool,
    pub clarification_round: i32,
    pub context_profile: ContextProfile,
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
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSplitRecommendation {
    pub recommended_by_agent_id: Uuid,
    pub recommended_at: String,
    pub splits: Vec<SplitTicketSpec>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
