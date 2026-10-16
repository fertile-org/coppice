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
    pub board_id: Option<Uuid>,
    pub owner_user_id: Uuid,
    pub agent_id: Uuid,
    pub repo_id: Option<Uuid>,
    pub parent_session_id: Option<Uuid>,
    pub status: ChatSessionStatus,
    pub provider_session_id: Option<String>,
    pub provider_session_connector: Option<String>,
    pub last_message_preview: String,
    pub last_message_at: Option<OffsetDateTime>,
    pub last_message_role: Option<String>,
    pub has_active_run: bool,
    pub active_run_id: Option<Uuid>,
    /// First human message, trimmed to one line. Absent until that message exists.
    pub title: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

const CHAT_TITLE_MAX_CHARS: usize = 80;

/// Title a chat from its first human message. Blank input stays untitled.
pub fn title_from_first_message(body: &str) -> Option<String> {
    let line = body.lines().map(str::trim).find(|line| !line.is_empty())?;
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let count = collapsed.chars().count();
    if count <= CHAT_TITLE_MAX_CHARS {
        return Some(collapsed);
    }
    let trimmed: String = collapsed
        .chars()
        .take(CHAT_TITLE_MAX_CHARS.saturating_sub(1))
        .collect();
    Some(format!("{trimmed}…"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn title_comes_from_the_first_line() {
        assert_eq!(title_from_first_message("   "), None);
        assert_eq!(
            title_from_first_message("  Hello   there \nmore"),
            Some("Hello there".into())
        );
        let long = "a".repeat(90);
        let title = title_from_first_message(&long).unwrap();
        assert_eq!(title.chars().count(), 80);
        assert!(title.ends_with('…'));
    }

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
