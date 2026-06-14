#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextProfile {
    Full,
    HumanAgent,
    HumanChat,
}

impl ContextProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::HumanAgent => "human_agent",
            Self::HumanChat => "human_chat",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "human_agent" => Some(Self::HumanAgent),
            "human_chat" => Some(Self::HumanChat),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_profile_roundtrip() {
        for profile in [
            ContextProfile::Full,
            ContextProfile::HumanAgent,
            ContextProfile::HumanChat,
        ] {
            assert_eq!(ContextProfile::from_str(profile.as_str()), Some(profile));
        }
    }
}
