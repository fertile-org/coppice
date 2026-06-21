use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
    AgentRunFinished,
    AgentMentioned,
}

impl NotificationType {
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationType::AgentRunFinished => "agent_run_finished",
            NotificationType::AgentMentioned => "agent_mentioned",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "agent_run_finished" => Some(NotificationType::AgentRunFinished),
            "agent_mentioned" => Some(NotificationType::AgentMentioned),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: Uuid,
    pub recipient_user_id: Uuid,
    pub kind: NotificationType,
    pub title: String,
    pub body: Option<String>,
    pub ticket_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub comment_id: Option<Uuid>,
    pub mention_id: Option<Uuid>,
    pub source_key: String,
    pub read_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}
