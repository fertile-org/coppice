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
}

impl std::str::FromStr for ContextProfile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "full" => Ok(Self::Full),
            "human_agent" => Ok(Self::HumanAgent),
            "human_chat" => Ok(Self::HumanChat),
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
        ] {
            assert_eq!(ContextProfile::from_str(profile.as_str()), Ok(profile));
        }
    }
}
