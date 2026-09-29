//! Fail-closed approval policy for agent-proposed knowledge candidates.

use crate::config::KnowledgeConfig;
use crate::domain::knowledge::{
    is_high_impact, type_to_str, KnowledgeConfidence, KnowledgeScope, KnowledgeStatus,
    KnowledgeType,
};

#[derive(Debug, Clone, Copy)]
pub struct PolicyInput {
    pub knowledge_type: KnowledgeType,
    pub scope: KnowledgeScope,
    pub confidence: KnowledgeConfidence,
    pub requires_human_approval: bool,
    pub supersedes: bool,
}

pub fn policy_decision(
    config: &KnowledgeConfig,
    input: PolicyInput,
) -> (KnowledgeStatus, &'static str, String) {
    let human = |reason: &str| (KnowledgeStatus::Pending, "human_review", reason.to_string());
    if is_high_impact(input.knowledge_type) {
        return human("high-impact type always requires human approval");
    }
    if input.scope == KnowledgeScope::Workspace {
        return human("workspace-scoped candidates always require human approval");
    }
    if input.supersedes {
        return human("supersession candidates always require human approval");
    }
    if input.requires_human_approval {
        return human("agent requested human approval");
    }
    let allowed = config.auto_save.enabled
        && input.confidence == KnowledgeConfidence::High
        && config
            .auto_save
            .allowed_types
            .iter()
            .any(|value| value == type_to_str(input.knowledge_type));
    if allowed {
        (
            KnowledgeStatus::Approved,
            "auto_saved",
            "explicit low-risk allowlist and high confidence matched".into(),
        )
    } else {
        human("auto-save policy did not explicitly allow this candidate")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(knowledge_type: KnowledgeType) -> PolicyInput {
        PolicyInput {
            knowledge_type,
            scope: KnowledgeScope::Board,
            confidence: KnowledgeConfidence::High,
            requires_human_approval: false,
            supersedes: false,
        }
    }

    fn auto_save_config(types: &[&str]) -> KnowledgeConfig {
        let mut config = KnowledgeConfig::default();
        config.auto_save.enabled = true;
        config.auto_save.allowed_types = types.iter().map(|value| value.to_string()).collect();
        config
    }

    #[test]
    fn default_policy_is_fail_closed() {
        let config = KnowledgeConfig::default();
        assert_eq!(
            policy_decision(&config, input(KnowledgeType::TestCommand)).0,
            KnowledgeStatus::Pending
        );
    }

    #[test]
    fn explicit_low_risk_allowlist_can_auto_save_but_security_never_does() {
        let config = auto_save_config(&["test_command", "security_rule"]);
        assert_eq!(
            policy_decision(&config, input(KnowledgeType::TestCommand)).0,
            KnowledgeStatus::Approved
        );
        let (status, decision, reason) =
            policy_decision(&config, input(KnowledgeType::SecurityRule));
        assert_eq!(status, KnowledgeStatus::Pending);
        assert_eq!(decision, "human_review");
        assert!(reason.contains("high-impact"));
    }

    #[test]
    fn workspace_scope_supersession_and_agent_requests_stay_pending() {
        let config = auto_save_config(&["test_command"]);
        let mut workspace = input(KnowledgeType::TestCommand);
        workspace.scope = KnowledgeScope::Workspace;
        assert_eq!(
            policy_decision(&config, workspace).0,
            KnowledgeStatus::Pending
        );

        let mut supersedes = input(KnowledgeType::TestCommand);
        supersedes.supersedes = true;
        assert_eq!(
            policy_decision(&config, supersedes).0,
            KnowledgeStatus::Pending
        );

        let mut requested = input(KnowledgeType::TestCommand);
        requested.requires_human_approval = true;
        assert_eq!(
            policy_decision(&config, requested).0,
            KnowledgeStatus::Pending
        );
    }

    #[test]
    fn medium_confidence_never_auto_saves() {
        let config = auto_save_config(&["coding_convention"]);
        let mut medium = input(KnowledgeType::CodingConvention);
        medium.confidence = KnowledgeConfidence::Medium;
        assert_eq!(policy_decision(&config, medium).0, KnowledgeStatus::Pending);
    }
}
