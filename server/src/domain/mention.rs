use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionStatus {
    Pending,
    Handled,
    Ignored,
}

pub struct TicketMention {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub comment_id: Uuid,
    pub mentioned_agent_id: Uuid,
    pub resume_agent_id: Option<Uuid>,
    pub status: MentionStatus,
}
