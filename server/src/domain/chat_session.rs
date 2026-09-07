use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSessionStatus {
    Active,
    Archived,
    Cutoff,
}

impl ChatSessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Cutoff => "cutoff",
        }
    }
}

impl std::str::FromStr for ChatSessionStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "cutoff" => Ok(Self::Cutoff),
            other => Err(format!("unknown chat session status: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatSession {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub owner_user_id: Uuid,
    pub agent_id: Uuid,
    pub repo_id: Option<Uuid>,
    pub status: ChatSessionStatus,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn status_roundtrip() {
        for status in [
            ChatSessionStatus::Active,
            ChatSessionStatus::Archived,
            ChatSessionStatus::Cutoff,
        ] {
            assert_eq!(ChatSessionStatus::from_str(status.as_str()), Ok(status));
        }
    }
}
