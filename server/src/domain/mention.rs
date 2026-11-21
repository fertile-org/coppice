use crate::domain::Id;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionStatus {
    Pending,
    Handled,
    Ignored,
}

pub struct TicketMention {
    pub id: Id,
    pub ticket_id: Id,
    pub comment_id: Id,
    pub mentioned_agent_id: Id,
    pub resume_agent_id: Option<Id>,
    pub status: MentionStatus,
}
