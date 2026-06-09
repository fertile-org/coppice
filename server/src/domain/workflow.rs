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
    pub auto_assign_enabled: bool,
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

#[derive(Debug, Clone)]
pub struct TransitionAction {
    pub new_status: Option<TicketStatus>,
    pub new_assignee_id: Option<Option<Uuid>>,
    pub pending_recommendation: Option<Option<PendingRecommendation>>,
    pub substatus: Option<Option<Substatus>>,
    pub substatus_metadata: Option<Option<Value>>,
    pub enqueue_jobs: Vec<JobRequest>,
    pub increment_clarification_round: bool,
}
