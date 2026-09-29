#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextProfile {
    Full,
    HumanAgent,
    HumanChat,
    Conversation,
    KnowledgeCompaction,
}

impl ContextProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::HumanAgent => "human_agent",
            Self::HumanChat => "human_chat",
            Self::Conversation => "conversation",
            Self::KnowledgeCompaction => "knowledge_compaction",
        }
    }
}

impl std::str::FromStr for ContextProfile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "full" => Ok(Self::Full),
            "human_agent" => Ok(Self::HumanAgent),
            "human_chat" => Ok(Self::HumanChat),
            "conversation" => Ok(Self::Conversation),
            "knowledge_compaction" => Ok(Self::KnowledgeCompaction),
            other => Err(format!("unknown context profile: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn context_profile_roundtrip() {
        for profile in [
            ContextProfile::Full,
            ContextProfile::HumanAgent,
            ContextProfile::HumanChat,
            ContextProfile::Conversation,
            ContextProfile::KnowledgeCompaction,
        ] {
            assert_eq!(ContextProfile::from_str(profile.as_str()), Ok(profile));
        }
    }
}
