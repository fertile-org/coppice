use crate::domain::Id;
use time::OffsetDateTime;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMessageRole {
    Human,
    Agent,
    System,
}

impl ChatMessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::System => "system",
        }
    }
}

impl std::str::FromStr for ChatMessageRole {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "human" => Ok(Self::Human),
            "agent" => Ok(Self::Agent),
            "system" => Ok(Self::System),
            other => Err(format!("unknown chat message role: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: Id,
    pub session_id: Id,
    pub seq: i64,
    pub role: ChatMessageRole,
    pub body: String,
    pub agent_run_id: Option<Id>,
    pub action_metadata: Option<serde_json::Value>,
    pub attachment_ids: Vec<Id>,
    pub created_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn role_roundtrip() {
        for role in [
            ChatMessageRole::Human,
            ChatMessageRole::Agent,
            ChatMessageRole::System,
        ] {
            assert_eq!(ChatMessageRole::from_str(role.as_str()), Ok(role));
        }
    }
}
