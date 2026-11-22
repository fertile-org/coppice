use crate::domain::Id;
use time::OffsetDateTime;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
    AgentRunFinished,
    AgentMentioned,
    KnowledgeCompactionFailed,
    GitNeedsAction,
}

impl NotificationType {
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationType::AgentRunFinished => "agent_run_finished",
            NotificationType::AgentMentioned => "agent_mentioned",
            NotificationType::KnowledgeCompactionFailed => "knowledge_compaction_failed",
            NotificationType::GitNeedsAction => "git_needs_action",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "agent_run_finished" => Some(NotificationType::AgentRunFinished),
            "agent_mentioned" => Some(NotificationType::AgentMentioned),
            "knowledge_compaction_failed" => Some(NotificationType::KnowledgeCompactionFailed),
            "git_needs_action" => Some(NotificationType::GitNeedsAction),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: Id,
    pub recipient_user_id: Id,
    pub kind: NotificationType,
    pub title: String,
    pub body: Option<String>,
    pub ticket_id: Option<Id>,
    pub run_id: Option<Id>,
    pub agent_id: Option<Id>,
    pub comment_id: Option<Id>,
    pub mention_id: Option<Id>,
    pub source_key: String,
    pub read_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}
