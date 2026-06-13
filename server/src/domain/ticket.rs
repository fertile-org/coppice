use crate::domain::substatus::{Substatus, TicketStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct Ticket {
    pub id: Uuid,
    pub project_id: Uuid,
    pub repo_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: TicketStatus,
    pub substatus: Option<Substatus>,
    pub substatus_metadata: Option<Value>,
    pub priority: Option<TicketPriority>,
    pub assignee_agent_id: Option<Uuid>,
    pub owner_user_id: Option<Uuid>,
    pub branch_name: Option<String>,
    pub pending_assign_recommendation: Option<Value>,
    pub parent_ticket_id: Option<Uuid>,
    pub pending_split_recommendation: Option<Value>,
    pub clarification_round: i32,
    pub created_by: String,
    pub created_by_id: Option<Uuid>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

pub fn status_to_str(status: TicketStatus) -> &'static str {
    match status {
        TicketStatus::Backlog => "backlog",
        TicketStatus::Ready => "ready",
        TicketStatus::InProgress => "in_progress",
        TicketStatus::InReview => "in_review",
        TicketStatus::InQa => "in_qa",
        TicketStatus::WaitForFinalReview => "wait_for_final_review",
        TicketStatus::Done => "done",
        TicketStatus::Blocked => "blocked",
    }
}

pub fn status_from_str(s: &str) -> Option<TicketStatus> {
    match s {
        "backlog" => Some(TicketStatus::Backlog),
        "ready" => Some(TicketStatus::Ready),
        "in_progress" => Some(TicketStatus::InProgress),
        "in_review" => Some(TicketStatus::InReview),
        "in_qa" => Some(TicketStatus::InQa),
        "wait_for_final_review" => Some(TicketStatus::WaitForFinalReview),
        "done" => Some(TicketStatus::Done),
        "blocked" => Some(TicketStatus::Blocked),
        _ => None,
    }
}

pub fn substatus_to_str(substatus: Substatus) -> &'static str {
    match substatus {
        Substatus::WaitingForAgent => "waiting_for_agent",
        Substatus::WaitingForHuman => "waiting_for_human",
        Substatus::WaitingForOwner => "waiting_for_owner",
        Substatus::WaitingForCi => "waiting_for_ci",
        Substatus::BlockedByMissingCapability => "blocked_by_missing_capability",
        Substatus::BlockedByMissingSecret => "blocked_by_missing_secret",
        Substatus::BlockedByPermission => "blocked_by_permission",
        Substatus::BlockedByError => "blocked_by_error",
    }
}

pub fn substatus_from_str(s: &str) -> Option<Substatus> {
    match s {
        "waiting_for_agent" => Some(Substatus::WaitingForAgent),
        "waiting_for_human" => Some(Substatus::WaitingForHuman),
        "waiting_for_owner" => Some(Substatus::WaitingForOwner),
        "waiting_for_ci" => Some(Substatus::WaitingForCi),
        "blocked_by_missing_capability" => Some(Substatus::BlockedByMissingCapability),
        "blocked_by_missing_secret" => Some(Substatus::BlockedByMissingSecret),
        "blocked_by_permission" => Some(Substatus::BlockedByPermission),
        "blocked_by_error" => Some(Substatus::BlockedByError),
        _ => None,
    }
}

pub fn priority_to_str(priority: TicketPriority) -> &'static str {
    match priority {
        TicketPriority::Low => "low",
        TicketPriority::Medium => "medium",
        TicketPriority::High => "high",
        TicketPriority::Critical => "critical",
    }
}

pub fn priority_from_str(s: &str) -> Option<TicketPriority> {
    match s {
        "low" => Some(TicketPriority::Low),
        "medium" => Some(TicketPriority::Medium),
        "high" => Some(TicketPriority::High),
        "critical" => Some(TicketPriority::Critical),
        _ => None,
    }
}
